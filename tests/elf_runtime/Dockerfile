# aarch64 Linux test environment for the ELF runtime harness.
#
# The image is intentionally small: clang for compiling fixtures, lld so
# we can link without pulling in binutils, plus the runtime libc/libstdc++
# that the rewritten binaries will load via ld.so. Pin the base by digest
# in CI; pin by tag here for local iteration.
#
# Build:
#   docker buildx build --platform linux/arm64 \
#     -t armv8-encode-runtime tests/elf_runtime
#
# When the host is x86_64 the harness uses Docker's QEMU integration
# (binfmt_misc) — see README. The image itself is pure arm64.

FROM --platform=linux/arm64 ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        clang \
        lld \
        libc6-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work

# The harness mounts the host's working directory at /work at runtime.
# No COPY of fixtures here — they're rebuilt or read from the bind mount
# so iteration doesn't require rebuilding the image.
