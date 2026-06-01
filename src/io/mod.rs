//! I/O subsystem components for the IMSAI 8080 emulator
//!
//! Individual I/O components are composed into S-100 cards (see `card.rs`).
//! The keyboard, video display, and Tarbell controller are each standalone
//! modules that are combined into `ConsoleCard` and `TarbellCard`.

pub mod keyboard;
pub mod tarbell;
pub mod video;

pub use keyboard::Keyboard;
pub use tarbell::TarbellController;
pub use video::VideoDisplay;