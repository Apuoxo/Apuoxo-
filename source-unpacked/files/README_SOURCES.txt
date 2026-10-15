Aether OS — source snapshot
===========================
Tree: aether/kernel/src/  (Rust no_std multiboot2 kernel)

Main modules:
  main.rs, desktop.rs, shell.rs, textedit.rs, files_mgr.rs
  fs.rs, fs_ntfs.rs, fs_fat.rs
  drivers/xhci.rs, ehci.rs, usb_phone.rs, ps2.rs, ...

Build (x86_64, freestanding):
  See previous conversation for rustc + nasm + linker.ld + grub-mkrescue ISO.

textedit.rs — Text Editor (New/Open/Save/SaveAs)
Desktop/shell integration may need re-apply if desktop still uses WinKind::About.
