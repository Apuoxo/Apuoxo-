#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/out"
TARGET_DIR="$ROOT/target"
KERNEL="$TARGET_DIR/x86_64-unknown-none/release/androidos"

rm -rf "$OUT"
mkdir -p "$OUT/iso/boot/grub"

rustup toolchain install nightly --profile minimal
rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu

cd "$ROOT"
cargo +nightly build -Z build-std=core --target target.json --release

test -f "$KERNEL"
file "$KERNEL"

cp "$KERNEL" "$OUT/iso/boot/androidos"
cp grub/grub.cfg "$OUT/iso/boot/grub/grub.cfg"

grub-file --is-x86-multiboot2 "$KERNEL"
grub-mkrescue -o "$OUT/android-like-os.iso" "$OUT/iso"

test -s "$OUT/android-like-os.iso"
echo "ISO=$OUT/android-like-os.iso"
