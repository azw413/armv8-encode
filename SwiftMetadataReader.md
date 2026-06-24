# Swift metadata reader for `armv8-encode` — design spec

Status: design proposal, not yet implemented. Companion piece to the
existing read-only Objective-C metadata reader
(`container::macho_objc::read_objc_metadata`, commit
[`6311f3a`](https://github.com/azw413/armv8-encode/commit/6311f3a)).

## Why

Modern iOS / macOS binaries are predominantly Swift. Swift classes
that inherit from `NSObject` or are marked `@objc` register
themselves with the Objective-C runtime and so already appear in
`__objc_classlist` — the existing reader catches those. But:

- **Pure Swift classes / structs / enums** (no `@objc`, no
  `NSObject` inheritance — the common SwiftUI / value-type case)
  never appear in `__objc_classlist`. They live exclusively in
  `__swift5_types`.
- **Swift protocols** and their **conformances** live in
  `__swift5_proto` / `__swift5_protos`.
- **Field names** for structs / classes / enums live in
  `__swift5_fieldmd` / `__swift5_reflstr`.

A reverse-engineer looking at a Swift app today sees a huge gap
between what the symbol table tells them (mostly mangled symbol
names) and the source structure. A typed reader closes that gap
the same way `read_objc_metadata` did for ObjC.

## Scope

This spec covers the **read-only** path, matching the existing
ObjC reader's scope. No mutation API. The reader walks
`MachOImage`, returns an owned `SwiftMetadata` snapshot, and the
caller (Glass, or any other consumer) reads freely from there.

Phase 1 (this spec) covers:

- `__swift5_types` — nominal type descriptors for **classes,
  structs, enums**.
- `__swift5_proto` — protocol descriptors.
- `__swift5_protos` — protocol conformance records.
- `__swift5_fieldmd` — field descriptors (names + type mangled
  references).
- `__swift5_reflstr` — reflection string table.

Out of scope for phase 1 (deferred):

- `__swift5_capture` — captured-variable descriptors for closures.
- `__swift5_mpenum` — multi-payload enum metadata.
- `__swift5_assocty` — associated-type metadata.
- Generic instantiation patterns and witness table contents.
- Mangled-name demangling **inside this crate** — the reader
  returns raw mangled strings; downstream tooling (Glass uses
  `symbolic-demangle`) handles presentation.

## What makes Swift metadata hard

Three structural quirks that anyone writing this needs to
internalise up front:

1. **Relative-offset references everywhere.** A Swift descriptor
   doesn't store absolute addresses — every field that "points
   to" something else is a 32-bit signed offset from the address
   of that field itself. Sometimes the low bit indicates an
   indirect reference (the offset points to a pointer rather
   than directly to the target). Walking the metadata is
   essentially a series of `(field_addr + offset_value) → target_addr`
   computations.

2. **Conditional fields driven by flag bits.** A nominal type
   descriptor's header carries a 32-bit `Flags` word. Whether
   subsequent fields exist (generic parameter descriptor,
   resilient witness table, vtable, etc.) is determined by
   bit-masking that flag. Misreading a flag walks the descriptor
   into adjacent metadata and produces garbage. The Swift runtime
   source (`include/swift/ABI/Metadata.h` and
   `include/swift/ABI/MetadataValues.h`) is the only authoritative
   reference; expect to keep both files open while implementing.

3. **Section content is an array of *pointers*, not records.**
   `__swift5_types` is **not** a packed array of type descriptors
   — it's an array of 32-bit relative offsets, each pointing
   *to* a type descriptor that lives elsewhere in `__TEXT,__const`
   or similar. Same pattern for `__swift5_proto` and friends.
   The reader has to (a) walk the section as an offset array,
   (b) resolve each entry to the target descriptor's vaddr,
   (c) parse the descriptor at that target.

## Proposed public API

Living in `crate::container::macho_swift`, re-exported from
`crate::container::*` alongside the ObjC types.

```rust
/// Owned snapshot of an image's Swift metadata, built once via
/// [`read_swift_metadata`] and then read freely. Mirrors
/// [`ObjCMetadata`] in shape and lifetime conventions.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct SwiftMetadata {
    pub types: Vec<SwiftType>,
    pub protocols: Vec<SwiftProtocol>,
    pub conformances: Vec<SwiftConformance>,
}

/// A nominal Swift type — class, struct, or enum. Resolved from
/// a type-descriptor pointer in `__swift5_types`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftType {
    /// Mangled name as it appears in the descriptor's name slot.
    /// E.g. `"$s5MyApp7MyClassC"` — callers demangle for display.
    pub mangled_name: String,
    /// Address of the type descriptor itself in the image
    /// (`__TEXT,__const` or similar — wherever the descriptor lives).
    pub descriptor_vaddr: u64,
    /// `class`, `struct`, or `enum`. From `Flags & Kind_Mask`.
    pub kind: SwiftTypeKind,
    /// Parent context's descriptor vaddr — usually a module
    /// descriptor (parent name = module name) or a parent class
    /// for nested types. `None` for top-level types whose parent
    /// is the module's anonymous root.
    pub parent_vaddr: Option<u64>,
    /// Fields declared on this type. Resolved from the type's
    /// `FieldDescriptor` reference. Empty if no field metadata
    /// was emitted (rare; the Swift compiler emits it by default).
    pub fields: Vec<SwiftField>,
    /// vaddr of the type's metadata accessor function, when
    /// present. The accessor is what runtime code calls to get
    /// the metadata for this type; treating it as a function
    /// symbol gives a useful name in the listing.
    pub metadata_accessor_vaddr: Option<u64>,
    /// Class-specific: the v-table function pointers (one per
    /// virtual method, in declaration order). Empty for
    /// structs/enums and for classes with no virtual methods.
    pub vtable: Vec<SwiftVTableEntry>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum SwiftTypeKind {
    Class,
    Struct,
    Enum,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftField {
    /// Field name from the reflection string table, e.g.
    /// `"isReady"`. Plain ASCII; no mangling.
    pub name: String,
    /// Mangled type reference for the field's declared type.
    /// E.g. `"$sSbD"` for `Swift.Bool`. Caller demangles for
    /// display.
    pub mangled_type: String,
    /// `Flags` bits from the field descriptor: `IsIndirectCase`,
    /// `IsVar`, etc. See Swift's `FieldDescriptorKind`.
    pub flags: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftVTableEntry {
    /// vaddr of the function the v-table slot points to.
    pub impl_vaddr: u64,
    /// Method descriptor flags: throws / async / mutating /
    /// dynamic-replacement etc.
    pub flags: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftProtocol {
    pub mangled_name: String,
    pub descriptor_vaddr: u64,
    /// Total required method count (instance + class).
    pub num_requirements: u32,
    /// Mangled-name references for protocols this protocol
    /// inherits from.
    pub inherited: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SwiftConformance {
    /// vaddr of the conformance descriptor itself.
    pub descriptor_vaddr: u64,
    /// The protocol being conformed to. Either an in-image
    /// vaddr (when the protocol is defined in this image) or
    /// the symbol name it's bound to (cross-image).
    pub protocol_ref: SwiftRef,
    /// The type conforming. Same in-image / external split.
    pub type_ref: SwiftRef,
    /// vaddr of the witness table, when the conformance is
    /// generic (witness tables for non-generic conformances
    /// are inline and don't need a separate vaddr).
    pub witness_table_vaddr: Option<u64>,
}

/// In-image / external reference, paralleling the ObjC reader's
/// "either in-image vaddr or bound import name" pattern.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SwiftRef {
    /// Resolves to a descriptor inside this image.
    InImage { vaddr: u64 },
    /// Resolves to an import named in the chained-fixup table.
    Imported { symbol_name: String },
}

#[derive(Debug)]
pub enum SwiftReadError {
    NotMachO,
    /// No `LC_DYLD_CHAINED_FIXUPS` — Swift relies on the modern
    /// fixup format for type-descriptor cross-references.
    ChainedFixupsMissing,
    /// No `__swift5_types` section, or it's empty. Not a real
    /// error per se — the caller can treat `Ok(SwiftMetadata::default())`
    /// as equivalent.
    NoSwiftMetadata,
    Truncated(String),
    UnresolvedPointer { at_file_offset: u64, raw: u64 },
}

/// Build the snapshot. Mirrors `read_objc_metadata` in error
/// shape and lifetime.
pub fn read_swift_metadata(image: &MachOImage) -> Result<SwiftMetadata, SwiftReadError>;
```

## Implementation outline

### 1. Section discovery

Walk `image.layout.segments` looking for sections named
`__swift5_types`, `__swift5_proto`, `__swift5_protos`,
`__swift5_fieldmd`, `__swift5_reflstr`. These all live in
`__TEXT` on iOS / macOS. Each section's `addr` + `size` defines
its vaddr range; the file offset is derived the same way the
ObjC reader does.

Build a small helper `vaddr_to_file_offset(vaddr) -> Option<usize>`
that walks the segments looking for one whose vaddr range
contains the address. Used everywhere below.

### 2. Reflection string table

`__swift5_reflstr` is a tightly-packed NUL-terminated string
table — same format as ELF `.strtab`. Build a
`HashMap<u64, &str>` from "string-vaddr" → "string contents" by
walking the section once and recording each NUL-terminated run's
start vaddr. Field-descriptor name pointers point into this
section.

### 3. Field descriptors

`__swift5_fieldmd` is an array of `FieldDescriptor` records:

```text
struct FieldDescriptor {
    int32_t MangledTypeName;      // relative offset to mangled name
    int32_t Superclass;           // relative offset, optional
    uint16_t Kind;                // FieldDescriptorKind
    uint16_t FieldRecordSize;     // size of each FieldRecord below
    uint32_t NumFields;
    FieldRecord Fields[NumFields];
};

struct FieldRecord {
    uint32_t Flags;
    int32_t MangledTypeName;      // relative offset to mangled type
    int32_t FieldName;            // relative offset to ASCII name in __swift5_reflstr
};
```

Build a `HashMap<u64, ParsedFieldDescriptor>` keyed on the
descriptor's vaddr. Each parsed descriptor carries the resolved
`mangled_type_name`, `kind`, and the array of `(field_name,
mangled_type)` pairs. Type descriptors reference field
descriptors by vaddr, so this is the map we look into.

### 4. Type descriptors

`__swift5_types` is an array of `int32_t` relative offsets,
each pointing to a `TypeContextDescriptor`. The base type
descriptor layout:

```text
struct TargetContextDescriptor {
    uint32_t Flags;               // ContextDescriptorKind | bits
    int32_t Parent;               // relative offset to parent descriptor (or 0)
};

// Class adds:
struct TargetClassDescriptor : TargetTypeContextDescriptor {
    int32_t Name;                 // relative offset to mangled name
    int32_t AccessFunction;       // relative offset to metadata-accessor fn
    int32_t FieldDescriptor;      // relative offset to FieldDescriptor (or 0)
    int32_t Superclass;
    uint32_t MetadataNegativeSizeInWords;
    uint32_t MetadataPositiveSizeInWords;
    uint32_t NumImmediateMembers;
    uint32_t NumFields;
    uint32_t FieldOffsetVectorOffset;
    // ...followed by optional generic / resilient / vtable trailing fields,
    // gated on Flags.
};
```

Struct and enum descriptors are simpler — same prefix, different
trailing fields.

The reader visits each entry pointer in `__swift5_types`:

1. Resolve the pointer to a descriptor vaddr.
2. Read the `Flags` word; mask `Kind_Mask` (low 5 bits) to decide
   class / struct / enum.
3. Read the common prefix: parent offset, name offset, access
   function offset, field descriptor offset.
4. For classes, check the trailing-field flags (`HasVTable`,
   `HasResilientSuperclass`, etc.) and parse what's present.
5. Resolve the field descriptor offset into the
   `ParsedFieldDescriptor` map from step 3.

### 5. Protocol + conformance descriptors

`__swift5_proto` is an offset array → protocol descriptors:

```text
struct TargetProtocolDescriptor : TargetContextDescriptor {
    int32_t Name;
    uint32_t NumRequirementsInSignature;
    uint32_t NumRequirements;
    int32_t AssociatedTypeNames;
    // ProtocolRequirement[NumRequirements] trailing
};
```

`__swift5_protos` is an offset array → conformance descriptors:

```text
struct TargetProtocolConformanceDescriptor {
    int32_t ProtocolDescriptor;        // points to protocol (in-image) or import
    int32_t TypeRef;                   // points to type descriptor or import
    int32_t WitnessTablePattern;
    uint32_t ConformanceFlags;
    // ...trailing fields gated on flags
};
```

The `ProtocolDescriptor` / `TypeRef` low bit indicates **indirect
reference**: when set, the offset points to a pointer in
`__DATA,__got` or `__DATA_CONST` that the chained-fixup map then
resolves to either an in-image target (rebase) or an imported
symbol (bind). This is where the existing
`build_chained_fixup_map` infrastructure plugs in directly.

### 6. Error handling

Match the ObjC reader's conservative posture:

- Any `vaddr` we can't resolve to a section produces
  `Truncated(...)` with the offending vaddr.
- Missing chained-fixup table → `ChainedFixupsMissing`.
- Missing `__swift5_types` → return `Ok(SwiftMetadata::default())`
  rather than `Err`, since "this image just has no Swift" is a
  legitimate state.
- Non-Mach-O input → `NotMachO`.

The caller (Glass and any other consumer) should treat all errors
as "no Swift names available, fall back to what you had before"
— same convention as the ObjC reader.

## What armv8-encode infrastructure this leans on

- `MachOImage` — already present.
- Per-section vaddr → file offset lookup — exists internally,
  may need to be `pub` if not already exposed.
- `ChainedFixupMap` — already present, exported. Used for
  indirect references in conformance descriptors.
- Endianness handling — already `little-endian` per platform.

## What it does NOT lean on

No demangling. The reader returns raw mangled strings (`$s...`)
and callers handle presentation. This matches the ObjC reader's
treatment of selectors.

No reflection-introspection beyond what's needed to extract
names + vaddrs. Field types are mangled strings; we don't try
to parse the type grammar.

No `__swift5_capture` / `__swift5_mpenum` / `__swift5_assocty`
in phase 1. These are needed for closure capture descriptors,
multi-payload enum payload layouts, and protocol associated
types. None are required for the dominant "show me the classes,
fields, methods, protocols" use case. Add in phase 2 if a real
consumer needs them.

## Testing strategy

Vendor 2-3 small Swift Mach-O fixtures into `tests/`:

- A pure-Swift class with two fields and a method (struct/class/enum
  parser coverage).
- A class with a v-table (vtable parser coverage).
- A protocol with two methods + a type conforming to it
  (conformance + protocol parser coverage).
- Optional: a Swift class that's `@objc` and inherits from
  `NSObject` — should appear in both the ObjC reader and the
  Swift reader, useful for cross-check tests.

Each fixture is the output of `swiftc -emit-library
-target arm64-apple-ios16.0 fixture.swift`, then run through
`xcrun strip -x` to shrink. Asserting against exact byte counts
isn't useful (Swift version drift); assert against type / method
/ field names + counts.

## Open questions for the maintainer

1. **Section discovery API.** The existing ObjC reader has
   private section-lookup helpers. Are those willing to be made
   `pub(crate)` so the Swift reader can share, or should the
   Swift reader duplicate? (Duplication is fine if you'd
   rather keep the ObjC reader's surface narrow.)

2. **Cross-image vs in-image distinction.** The proposed
   `SwiftRef` enum mirrors the ObjC reader's `Option<String>`
   for class names. Happy to match the ObjC reader's exact
   convention (e.g. `Option<String> + Option<u64>`) instead of
   the enum I proposed — whichever you prefer.

3. **Phase 1 cutoff.** Is the proposed scope (`__swift5_types` +
   `__swift5_proto` + `__swift5_protos` + `__swift5_fieldmd` +
   `__swift5_reflstr`) the right initial cut, or would you
   prefer an even smaller landing (e.g. just `__swift5_types`
   first)?

## Glass-side consumption (informational)

For context — this is what Glass will do with the metadata once
landed; not something armv8-encode needs to implement.

1. **Symbol map enrichment.** Same pattern as the ObjC pass:
   walk types + protocols + conformances, synthesise function
   symbols for `metadata_accessor_vaddr`, every vtable entry's
   `impl_vaddr`, and conformance witness tables. Names are the
   demangled Swift form via `symbolic-demangle`.

2. **Tree view.** A "Swift" group in the left navigator peer to
   the existing "Objective-C" group. Class / struct / enum leaves
   open a viewer tab showing fields, vtable methods, and
   conformed protocols.

3. **Bidirectional navigation.** Click an `impl_vaddr` in the
   class viewer → jump to listing. Right-click a Swift symbol
   in the listing → "Open type view".

These consumer features are well-trodden by the existing ObjC
implementation; once `read_swift_metadata` lands the wiring is
mostly mechanical.
