#!/usr/bin/env bash
# Build the Swift fixture dylib used by macho_swift tests.
# Run on macOS with the Xcode toolchain installed. The resulting
# dylib is checked in so CI doesn't need swiftc.
set -euo pipefail
cd "$(dirname "$0")"
swiftc \
  -emit-library \
  -module-name greet_swift \
  -target arm64-apple-macos12 \
  -O \
  -Xlinker -fixup_chains \
  -o libgreet_swift.dylib \
  fixture.swift
