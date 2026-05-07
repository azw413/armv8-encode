#!/bin/sh
# One-shot setup for the ELF runtime harness.
#
# - Verifies Docker is reachable.
# - Installs QEMU user-mode binfmt handlers for arm64 if the host is
#   x86_64 and they're missing (no-op on aarch64 hosts and on macOS,
#   where Docker Desktop ships them).
# - Builds the `armv8-encode-runtime` image from this directory.
#
# Idempotent: re-running re-uses cached layers and skips the binfmt
# install if it's already present. Safe to run on every CI job.
#
# Usage:
#     tests/elf_runtime/setup.sh
#     ARMV8_RUNTIME_IMAGE=my/tag tests/elf_runtime/setup.sh

set -eu

IMAGE_TAG="${ARMV8_RUNTIME_IMAGE:-armv8-encode-runtime}"
HARNESS_DIR="$(cd "$(dirname "$0")" && pwd)"

# 1. Docker present?
if ! command -v docker >/dev/null 2>&1; then
    echo "error: docker not found on PATH." >&2
    echo "  Install Docker Desktop (macOS/Windows) or Docker Engine (Linux)." >&2
    exit 1
fi
if ! docker version >/dev/null 2>&1; then
    echo "error: docker is installed but the daemon isn't reachable." >&2
    echo "  Start Docker Desktop, or check \`systemctl status docker\`." >&2
    exit 1
fi

# 2. QEMU user-mode emulation, if needed.
#
# Docker Desktop on macOS ships binfmt handlers; native aarch64 hosts
# don't need them; only x86_64 Linux without them needs the install.
host_arch="$(uname -m)"
host_os="$(uname -s)"
case "$host_os/$host_arch" in
    Linux/x86_64|Linux/i?86)
        # Probe by trying to run a tiny arm64 image. If it works,
        # binfmt is set up; if it errors, install it.
        if ! docker run --rm --platform=linux/arm64 \
                hello-world >/dev/null 2>&1; then
            echo "Installing QEMU arm64 binfmt handlers..."
            docker run --privileged --rm tonistiigi/binfmt --install arm64
        else
            echo "QEMU arm64 binfmt already configured."
        fi
        ;;
    Darwin/*|Linux/aarch64|Linux/arm64)
        # Docker Desktop on macOS ships handlers; native arm64 needs
        # nothing.
        ;;
    *)
        echo "warning: unrecognised host $host_os/$host_arch — assuming" \
             "linux/arm64 emulation is already configured." >&2
        ;;
esac

# 3. Build the image. `buildx` is the modern path and ships with all
# recent Docker installs; fall back to plain `docker build` for the
# unusual case where it's missing.
echo "Building image $IMAGE_TAG from $HARNESS_DIR ..."
if docker buildx version >/dev/null 2>&1; then
    docker buildx build --platform linux/arm64 \
        --load \
        -t "$IMAGE_TAG" \
        "$HARNESS_DIR"
else
    docker build --platform linux/arm64 \
        -t "$IMAGE_TAG" \
        "$HARNESS_DIR"
fi

echo
echo "Done. Run the harness with:"
echo "  cargo test --test elf_runtime -- --ignored --nocapture"
