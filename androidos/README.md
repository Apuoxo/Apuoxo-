# Android-x86 baseline for Fujitsu LIFEBOOK AH532

## Current milestone: verified upstream boot image

The project now uses the real upstream **Android-x86 9.0-r2** ISO as its baseline instead of trying to boot a freestanding Rust kernel as if it were Android. The source release is Android 9 Pie with Linux kernel 4.19.110.

This milestone does **not** claim that a custom Android OS has been built. GitHub Actions downloads the upstream image, verifies its pinned SHA-256, inspects the ISO/El Torito boot metadata, and publishes the verified ISO plus a source manifest as an artifact. The Rust files still present in the repository are not included in this ISO.

- Upstream release notes: https://www.android-x86.org/releases/releasenote-9-0-r2.html
- Upstream download: https://sourceforge.net/projects/android-x86/files/Release%209.0/android-x86_64-9.0-r2.iso/download
- Expected SHA-256: `f7eb8fc56f29ad5432335dc054183acf086c539f3990f0b6e9ff58bd6df4604e`

## Download

Open the latest successful **Android-x86 baseline ISO** workflow run and download the artifact named `android-x86-9.0-r2-ah532-baseline`.

## Required AH532 hardware test

1. Write the ISO to a USB stick using a raw-image writing tool.
2. Boot from USB using the laptop firmware boot menu.
3. First test the live boot only; do not install to the internal disk.
4. Report the last visible screen or send a photo. We need to verify GRUB, Android startup, Intel HD Graphics 3000 at 1366×768, keyboard/touchpad, audio, Ethernet, and Wi-Fi separately.

A green Actions run verifies the download checksum and ISO structure only. It does **not** prove that the image boots or that its drivers work on the physical AH532.

## Important limitations

Android-x86 9.0-r2 is an old upstream release, not a current Android version. It is selected as a conservative hardware-compatibility baseline; the AH532 has not yet been tested with this image. Custom branding, desktop changes, APK compatibility work, and a newer Android base come only after the real-hardware baseline is confirmed.
