#!/usr/bin/env bash
# Cross-build quack-navd for the duck's board: Radxa Zero 3 (RK3566,
# aarch64) running Armbian 26.2.x with the Debian 13 (Trixie) userland,
# glibc 2.41.
#
# Usage: scripts/cross-build.sh [extra cargo arguments]
# Output: target/aarch64-unknown-linux-gnu/release/quack-navd
#
# Needs rustup with the aarch64-unknown-linux-gnu target, zig and
# cargo-zigbuild (on a Mac: brew install rustup zig cargo-zigbuild, then
# rustup toolchain install stable --target aarch64-unknown-linux-gnu).
# No Docker: `zig cc` is the cross linker and brings the glibc stubs.
#
# The `.2.31` suffix pins the glibc floor (a cargo-zigbuild feature), as
# microduck's own `cargo board` does: unpinned, the binary would link
# against the build host's glibc and could refuse to load on the board.
set -euo pipefail

TARGET=aarch64-unknown-linux-gnu
GLIBC_FLOOR=2.31

cd "$(dirname "$0")/.."

has_target() {
    [ -d "$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$TARGET" ]
}

# Homebrew's `rust` has no std for other targets; Homebrew's rustup is
# keg-only, so its cargo may not be first on PATH.
if ! has_target && [ -x /opt/homebrew/opt/rustup/bin/rustup ]; then
    PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi
if ! has_target; then
    echo "no Rust std for $TARGET on this PATH (rustc: $(command -v rustc || echo none))" >&2
    echo "  rustup target add $TARGET" >&2
    exit 1
fi
for tool in zig cargo-zigbuild; do
    command -v "$tool" >/dev/null || { echo "$tool is not installed (brew install zig cargo-zigbuild)" >&2; exit 1; }
done

echo "building quack-navd for $TARGET, glibc >= $GLIBC_FLOOR ($(rustc --version))"
cargo zigbuild --release -p quack-nav --bin quack-navd --target "$TARGET.$GLIBC_FLOOR" "$@"

BIN="target/$TARGET/release/quack-navd"
command -v file >/dev/null && file "$BIN"
if command -v objdump >/dev/null; then
    echo "glibc required: $(objdump -T "$BIN" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)"
fi
echo "built $BIN"
