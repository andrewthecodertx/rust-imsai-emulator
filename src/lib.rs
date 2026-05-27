//! # IMSAI 8080 Emulator
//!
//! A Rust-based emulator for the IMSAI 8080 microcomputer system.
//! This emulator uses the Intel 8080 CPU core and implements the
//! necessary hardware components to simulate the IMSAI 8080.

#![warn(missing_docs)]

/// S-100 system bus
pub mod bus;
/// BIOS implementation (simplified, for basic console I/O)
pub mod bios;
/// CP/M 2.2 BIOS implementation (full 17-call BIOS with Tarbell controller)
pub mod cpm_bios;
/// CP/M loader and execution
pub mod cpm;
/// CP/M disk image creation and management
pub mod disk;
/// Disk parameter block definitions
pub mod dpb;
/// The main emulator system
pub mod emulator;
/// I/O subsystem
pub mod io;
/// Memory subsystem
pub mod memory;
/// System components
pub mod system;

// Re-export the main components
pub use bios::Bios;
pub use bus::ImsaiBus;
pub use cpm::CpMLoader;
pub use cpm_bios::CpmBios;
pub use disk::DiskImage;
pub use emulator::Imsai8080;
pub use io::TarbellController;
pub use memory::Memory;

/// Create a new IMSAI 8080 emulator instance
pub fn new() -> Imsai8080 {
    Imsai8080::new()
}