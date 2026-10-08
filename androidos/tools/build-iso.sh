#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/out"
rm -rf "$OUT"
mkdir -p "$OUT/iso/boot/grub"

rustup toolchain install nightly --profile minimal
rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu

cd "$ROOT"
cargo +nightly build -Z build-std=core --target target.json --release -Z unstable-options

cp target/target/release/androidos "$OUT/iso/boot/androidos"
cp grub/grub.cfg "$OUT/iso/boot/grub/grub.cfg"

grub-mkrescue -o "$OUT/android-like-os.iso" "$OUT/iso"
echo "ISO=$OUT/android-like-os.iso"
