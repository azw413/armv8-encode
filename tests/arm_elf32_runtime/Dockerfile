# ARMv7 (armhf) Linux test environment for the ELF32 runtime
# smoke test. Mirrors tests/elf_runtime/Dockerfile but targets
# linux/arm/v7 so the rewriter's ELF32 code paths get
# exercised end-to-end under qemu-user.
#
# Build:
#   docker buildx build --platform linux/arm/v7 \
#     -t armv7-encode-runtime tests/arm_elf32_runtime
#
# When the host is x86_64 or arm64 (Apple Silicon), Docker
# Desktop's QEMU integration handles the armv7 binfmt
# automatically.

FROM --platform=linux/arm/v7 ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        clang \
        lld \
        libc6-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /work

# The harness mounts the host's working directory at /work at
# runtime; no COPY of fixtures here. Build scripts live next
# to the C sources under tests/arm_elf32_runtime/fixtures/.
