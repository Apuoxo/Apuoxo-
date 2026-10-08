# Android-like OS

First experimental bootable foundation for a laptop-oriented Android-like operating system.

## Commit 1 goal

Boot through GRUB and reach a real kernel entry point that writes directly to the VGA text buffer.

The next stages will replace the text-mode proof with framebuffer graphics, input, a window/compositor layer, and the Android-like desktop.

## Build

Requires Rust nightly, GRUB utilities, and xorriso.

    ./tools/build-iso.sh

The result is:

    out/android-like-os.iso
