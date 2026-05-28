#!/bin/sh
# One-shot setup for the ARMv7 ELF32 runtime smoke test.
#
# - Verifies Docker is reachable.
# - Installs QEMU user-mode binfmt handlers for arm/v7 if the
#   host is x86_64 and they're missing (no-op on aarch64
#   hosts and on macOS, where Docker Desktop ships them).
# - Builds the `armv7-encode-runtime` image from this dir.
#
# Idempotent: safe to re-run.
#
# Usage:
#     tests/arm_elf32_runtime/setup.sh

set -eu

IMAGE_TAG="${ARMV7_RUNTIME_IMAGE:-armv7-encode-runtime}"
HARNESS_DIR="$(cd "$(dirname "$0")" && pwd)"

# 1. Docker reachable?
if ! docker info >/dev/null 2>&1; then
    echo "error: Docker daemon is not reachable." >&2
    echo "  Start Docker Desktop, or check \`systemctl status docker\`." >&2
    exit 1
fi

# 2. QEMU arm/v7 binfmt, if needed.
host_arch="$(uname -m)"
host_os="$(uname -s)"
case "$host_os/$host_arch" in
    Linux/x86_64|Linux/i?86)
        if ! docker run --rm --platform=linux/arm/v7 \
                hello-world >/dev/null 2>&1; then
            echo "Installing QEMU arm/v7 binfmt handlers..."
            docker run --privileged --rm tonistiigi/binfmt --install arm
        else
            echo "QEMU arm/v7 binfmt already configured."
        fi
        ;;
    Darwin/*|Linux/aarch64|Linux/arm64)
        # Docker Desktop on macOS and native arm64 ship handlers.
        ;;
    *)
        echo "warning: unrecognised host $host_os/$host_arch — assuming" \
             "linux/arm/v7 emulation is already configured." >&2
        ;;
esac

# 3. Build the image.
echo "Building image $IMAGE_TAG from $HARNESS_DIR ..."
if docker buildx version >/dev/null 2>&1; then
    docker buildx build --platform linux/arm/v7 \
        --load \
        -t "$IMAGE_TAG" \
        "$HARNESS_DIR"
else
    docker build --platform linux/arm/v7 \
        -t "$IMAGE_TAG" \
        "$HARNESS_DIR"
fi

echo
echo "Done. Run the smoke test with:"
echo "  cargo test --test arm_elf32_runtime -- --ignored --nocapture"
