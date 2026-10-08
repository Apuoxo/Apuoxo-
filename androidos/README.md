# Android-like OS for Fujitsu LIFEBOOK AH532

## Current milestone: boot the custom kernel

The ISO now includes the freestanding Rust x86_64 kernel and a GRUB Multiboot2 entry. This is a kernel bring-up milestone, not yet a complete Android distribution: there is no Android userspace, Linux compatibility layer, APK runtime, Wi-Fi stack, or desktop environment at this stage.

## Build

GitHub Actions builds the kernel with the nightly Rust toolchain and `build-std=core`, checks that GRUB recognizes the kernel as Multiboot2, creates the UEFI-capable ISO, inspects the ISO, and uploads it as the `android-os-bootable` artifact.

## Hardware test

1. Download the ISO artifact from the latest successful **Android bootable image** workflow run.
2. Boot the AH532 from USB using its firmware boot menu.
3. Record the exact last visible message or take a photo of the screen.
4. Do not treat a green CI run as proof of a successful physical-hardware boot.

## Known limits

The current kernel's first milestone is a text-mode marker and a stable halt loop. Hardware drivers, memory management, interrupts, storage, networking, sound, Android/Linux userspace, and application execution remain future milestones.
