# macho_swift_fixture

Vendored Swift dylib used by `tests::macho_swift` and the
`read_swift_metadata` reader.

The dylib is rebuilt with `./build.sh` (requires Xcode / swiftc on
macOS, arm64). The output `libgreet_swift.dylib` is checked in so
CI doesn't need a Swift toolchain.

The fixture exercises:

- `Greeter` protocol with two requirements (`__swift5_proto`).
- `Point` struct with two `Double` fields (`__swift5_types` + struct path).
- `Mood` enum with three cases (`__swift5_types` + enum path).
- `Greeting` class with three fields and three methods, conforming
  to `Greeter` (`__swift5_types` class path, vtable, `__swift5_protos`).
- `FancyGreeting` subclass overriding `greet()` (vtable override).

`-O` is on so the compiler emits realistic, deduplicated metadata.
