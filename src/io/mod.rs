//! I/O subsystem components for the IMSAI 8080 emulator
//!
//! The keyboard and video display modules are standalone I/O components
//! that are composed into S-100 cards (see `cards/` module).

pub mod keyboard;
pub mod video;

pub use keyboard::Keyboard;
pub use video::VideoDisplay;