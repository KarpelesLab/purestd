#!/usr/bin/env bash
# Build a purestd example as a fully-static, libc-free Linux ELF, cross-compiled
# from this macOS host using rustup's toolchain + rust-lld.
#
# Usage: scripts/build-linux.sh <example> [x86_64|aarch64]
#
# This is a development convenience. The "real" packaging path is the
# cargo-fullrust toolchain, which wires purestd in as the sysroot `std`.
set -euo pipefail

EXAMPLE="${1:-hello}"
ARCH="${2:-x86_64}"

case "$ARCH" in
  x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
  aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
  *) echo "arch must be x86_64 or aarch64" >&2; exit 2 ;;
esac

# rustup's stable toolchain has the Linux std + rust-lld; the Homebrew rustc on
# PATH does not, so we point RUSTC at rustup's rustc explicitly.
TC="$(rustc +stable --print sysroot 2>/dev/null || true)"
if [ -z "$TC" ]; then
  TC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin"
fi
RUSTLLD="$(find "$TC" -name rust-lld | head -1)"

RUSTC="$TC/bin/rustc" "$TC/bin/cargo" build --example "$EXAMPLE" --target "$TARGET" \
  --config "target.$TARGET.linker=\"$RUSTLLD\"" \
  --config "target.$TARGET.rustflags=[\"-Clinker-flavor=ld.lld\",\"-Crelocation-model=static\",\"-Clink-arg=-static\",\"-Clink-arg=-no-pie\"]"

BIN="target/$TARGET/debug/examples/$EXAMPLE"
echo
file "$BIN"
