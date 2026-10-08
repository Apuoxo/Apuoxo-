#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/out"
KERNEL="$ROOT/target/target/release/androidos"

rm -rf "$OUT"
mkdir -p "$OUT/iso/boot/grub"

rustup toolchain install nightly --profile minimal
rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu

cd "$ROOT"
cargo +nightly build -Z build-std=core --target target.json --release

test -s "$KERNEL"
file "$KERNEL"
grub-file --is-x86-multiboot2 "$KERNEL"

cp "$KERNEL" "$OUT/iso/boot/androidos"
cp grub/grub.cfg "$OUT/iso/boot/grub/grub.cfg"

grub-mkrescue -o "$OUT/android-os-bootable.iso" "$OUT/iso"
test -s "$OUT/android-os-bootable.iso"
isoinfo -d -i "$OUT/android-os-bootable.iso"
echo "ISO=$OUT/android-os-bootable.iso"
