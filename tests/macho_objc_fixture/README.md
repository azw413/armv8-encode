# ObjC fixture for `container::macho_objc`

Tiny ARM64 dylib with one class, one protocol, one category, and
one ivar/property. Built natively on Apple Silicon — no Docker.

The category extends `NSObject` rather than `Greet`, because
clang/ld64 merges same-image categories directly into the
class's method list (no `__objc_catlist` emitted). Extending an
external class forces `__objc_catlist` to appear so the
category-reading test path is exercised.

## Rebuild

```sh
tests/macho_objc_fixture/build.sh
```

Requires Xcode command-line tools (`clang` + the ObjC runtime
headers under `/usr/include/objc/`). The resulting
`libgreet_objc.dylib` is checked in so the tests don't depend
on having a working toolchain at test-run time.
