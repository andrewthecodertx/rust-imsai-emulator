//! Chip-level models for S-100 bus card components
//!
//! These models implement the actual silicon chips used on S-100 cards:
//! - Uart8251: Intel 8251A programmable communication interface
//! - Fd1771: Western Digital FD1771 floppy disk formatter/controller
//!
//! Cards compose these chips and add board-level logic (address decoding,
//! wait state generators, etc.).

pub mod fd1771;
pub mod uart8251;

pub use fd1771::Fd1771;
pub use uart8251::Uart8251;