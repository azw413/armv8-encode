//! Symbolic, relocation-aware code rewriting.
//!
//! The rewrite layer turns a decoded code region into an editable IR whose
//! branch operands are *symbolic*: instead of pointing at a hard-coded
//! address, each PC-relative target carries a [`Target`] — a reference to
//! a basic block, an extern symbol, a constant pool entry, or a literal
//! address. This is what lets the layout pass freely insert, delete, and
//! grow code without invalidating displacements.
//!
//! ## Pipeline
//!
//! ```text
//!   bytes ──► sweep ──► instructions ──► CFG
//!                                         │
//!                                         ▼
//!                                  RewritePlan::lift
//!                                         │
//!                                         ▼  edit operations
//!                                  (mutate operands, blocks, terminators)
//!                                         │
//!                                         ▼
//!                                    lay_out(plan, base)
//!                                         │
//!                                         ▼
//!                                    emit(plan, layout) ──► bytes
//! ```
//!
//! ## Status
//!
//! - In-place patches (no shifting), single-block edits, and inserts that
//!   trigger conditional-branch widening: implemented and tested.
//! - Out-of-range unconditional branches (>128 MiB), branch islands, and
//!   literal-pool emission: not yet implemented; layout returns
//!   [`LayoutError::DisplacementTooLarge`] in those cases.
//! - Symbol and constant target resolution: types are defined, but no
//!   resolver exists until binary-container ingest lands.

pub mod commit;
pub mod data;
pub mod editor;
pub mod emit;
pub mod ir;
pub mod layout;
pub mod plan;

pub use commit::commit_to_container;
pub use data::{
    commit_to_data_container, emit_data_section, DataEditError, DataEmitOutput, DataItem,
    DataLift, DataLiftError, DataPayload, DataSection,
};
pub use editor::{
    BinaryEditor, BinaryState, InitialiserPosition, LiftedTextSection, TextEditorError,
};
#[allow(deprecated)]
pub use editor::TextEditor;
pub use emit::{emit, EmitError, EmitOutput, EmittedRelocation};
pub use ir::{
    ConstantId, MacroOp as MacroOpGeneric, RewriteBlock as RewriteBlockGeneric,
    RewriteInstruction as RewriteInstructionGeneric, RewriteOp as RewriteOpGeneric,
    RewriteOperand as RewriteOperandGeneric, SymbolId, Target,
};
pub use crate::isa::aarch64::AarchMacroKind as MacroKind;

// AArch64-typed aliases — back-compat for code written before
// the rewrite layer was made generic. Use these for aarch64
// rewriting; for other ISAs, use the `Generic` variants
// directly with the appropriate `Isa` parameter.
use crate::isa::aarch64::Aarch64Isa;
pub type RewritePlanAArch64 = plan::RewritePlan<Aarch64Isa>;
pub type RewriteInstructionAArch64 = ir::RewriteInstruction<Aarch64Isa>;
pub type RewriteOperandAArch64 = ir::RewriteOperand<Aarch64Isa>;
pub type RewriteOpAArch64 = ir::RewriteOp<Aarch64Isa>;
pub type RewriteBlockAArch64 = ir::RewriteBlock<Aarch64Isa>;
pub type MacroOpAArch64 = ir::MacroOp<Aarch64Isa>;
// Default exports for back-compat — these were aarch64-specific
// before the refactor.
pub use RewriteBlockAArch64 as RewriteBlock;
pub use RewriteInstructionAArch64 as RewriteInstruction;
pub use RewriteOpAArch64 as RewriteOp;
pub use RewriteOperandAArch64 as RewriteOperand;
pub use RewritePlanAArch64 as RewritePlan;
pub use MacroOpAArch64 as MacroOp;
pub use layout::{lay_out, EmitStrategy, InstructionLayout, Layout, LayoutError};
pub use plan::EditError;

pub use crate::mc::ControlFlowGraph;
