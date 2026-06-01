//! I/O subsystem components for the IMSAI 8080 emulator
//!
//! The keyboard and video display modules are standalone I/O components
//! that are composed into S-100 cards (see `cards/` module).
//!
//! The `TarbellController` module is retained for backward compatibility
//! but is superseded by the chip-level `Fd1771` model (see `chips/fd1771.rs`).

pub mod keyboard;
pub mod tarbell;
pub mod video;

pub use keyboard::Keyboard;
pub use tarbell::TarbellController;
pub use video::VideoDisplay;