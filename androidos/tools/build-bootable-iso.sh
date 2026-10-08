#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/out"
rm -rf "$OUT"
mkdir -p "$OUT/iso/EFI/BOOT" "$OUT/iso/boot/grub"
grub-mkstandalone -O x86_64-efi -o "$OUT/iso/EFI/BOOT/BOOTX64.EFI" "boot/grub/grub.cfg=$ROOT/grub/grub.cfg"
grub-mkrescue -o "$OUT/android-os-bootable.iso" "$OUT/iso"
test -s "$OUT/android-os-bootable.iso"
echo "ISO=$OUT/android-os-bootable.iso"
