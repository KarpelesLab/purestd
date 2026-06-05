#!/usr/bin/env bash
# Build purestd examples as fully-static, libc-free ELFs for a Linux target,
# using the toolchain's bundled rust-lld. Portable: works both natively on Linux
# and cross-compiled from macOS (the toolchain just needs the target's std).
#
# Usage:
#   scripts/build-linux.sh                       # all examples, host-ish default x86_64
#   scripts/build-linux.sh <example> <arch>      # one example (arch: x86_64|aarch64)
#   TARGET=aarch64-unknown-linux-gnu scripts/build-linux.sh --all
set -euo pipefail

ARCH="${2:-x86_64}"
case "${TARGET:-}" in
  "") case "$ARCH" in
        x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
        aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
        *) echo "arch must be x86_64 or aarch64" >&2; exit 2 ;;
      esac ;;
esac

# rust-lld ships inside the toolchain; locate it under the sysroot.
SYSROOT="$(rustc --print sysroot)"
RUSTLLD="$(find "$SYSROOT" -name rust-lld 2>/dev/null | head -1)"
if [ -z "$RUSTLLD" ]; then
  echo "rust-lld not found under $SYSROOT (try: rustup component add llvm-tools)" >&2
  exit 1
fi

export RUSTFLAGS="-Clinker-flavor=ld.lld -Clinker=$RUSTLLD -Crelocation-model=static -Clink-arg=-static -Clink-arg=-no-pie"

if [ "${1:-}" = "" ] || [ "${1:-}" = "--all" ]; then
  cargo build --examples --target "$TARGET"
  echo "built all examples for $TARGET:"
  ls "target/$TARGET/debug/examples"/*.d 2>/dev/null | sed 's#.*/##; s/\.d$//' | sort -u
else
  cargo build --example "$1" --target "$TARGET"
  file "target/$TARGET/debug/examples/$1"
fi
