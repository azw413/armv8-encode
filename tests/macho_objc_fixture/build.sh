#!/bin/sh
# Build the tiny ObjC dylib used by the macho_objc parser test.
# Native arm64 macOS build — no codesign / rpath needed because
# the test only reads the bytes back from disk.
set -eu
cd "$(dirname "$0")"
clang -arch arm64 -dynamiclib -O0 -fobjc-arc greet.m -o libgreet_objc.dylib
