#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/out"
ISO="$OUT/android-x86_64-9.0-r2.iso"
URL="https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download"
EXPECTED_SHA256="f7eb8fc56f29ad5432335dc054183acf086c539f3990f0b6e9ff58bd6df4604e"

rm -rf "$OUT"
mkdir -p "$OUT"

echo "Source: Android-x86 9.0-r2 (Android 9, Linux kernel 4.19.110)"
echo "Downloading official upstream ISO..."
curl --fail --location --retry 3 --retry-all-errors --connect-timeout 30 --output "$ISO" "$URL"

echo "Checking pinned SHA-256..."
printf '%s  %s\n' "$EXPECTED_SHA256" "$ISO" | sha256sum --check --status || {
  echo "ERROR: upstream ISO checksum mismatch" >&2
  exit 1
}

cat > "$OUT/SOURCE.txt" <<'EOF'
Artifact: android-x86_64-9.0-r2.iso
Origin: https://www.android-x86.org/releases/releasenote-9-0-r2.html
Download: https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download
SHA-256: f7eb8fc56f29ad5432335dc054183acf086c539f3990f0b6e9ff58bd6df4604e
Base: Android-x86 9.0-r2 (Android 9 Pie), Linux kernel 4.19.110
This is an unmodified upstream image, not a custom-built Android distribution.
Hardware boot and AH532 driver compatibility have not been verified by CI.
EOF

file "$ISO"
sha256sum "$ISO"
echo "ISO=$ISO"
