//! Build-time bundled read-only media test assets.
//! The Makefile downloads the three CC0 samples into kernel/build/media.

pub static TEST_WAV: &[u8] = include_bytes!("../build/media/TEST.WAV");
pub static TEST_MP3: &[u8] = include_bytes!("../build/media/TEST.MP3");
