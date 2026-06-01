//! # IMSAI 8080 Emulator
//!
//! A Rust-based emulator for the IMSAI 8080 microcomputer system.
//! Implements the Intel 8080 CPU, Tarbell FD1771 disk controller,
//! and a CP/M 2.2 BIOS to run CP/M programs.

#![warn(missing_docs)]

/// S-100 bus card trait and standard card implementations
pub mod card;
/// CP/M 2.2 BIOS implementation (17-call BIOS with Tarbell controller)
pub mod bios;
/// S-100 system bus
pub mod bus;
/// CP/M disk image creation and management
pub mod disk;
/// Disk parameter block definitions
pub mod dpb;
/// The main emulator system
pub mod emulator;
/// I/O subsystem (keyboard, video, Tarbell controller)
pub mod io;
/// Memory subsystem
pub mod memory;
/// System components
pub mod system;

// Re-export the main components
pub use bios::Bios;
pub use bus::ImsaiBus;
pub use card::{Card, ConsoleCard, TarbellCard};
pub use disk::DiskImage;
pub use emulator::Imsai8080;
pub use io::TarbellController;
pub use memory::Memory;

/// Create a new IMSAI 8080 emulator instance
pub fn new() -> Imsai8080 {
    Imsai8080::new()
}