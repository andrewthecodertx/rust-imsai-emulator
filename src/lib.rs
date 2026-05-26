//! # IMSAI 8080 Emulator
//!
//! A Rust-based emulator for the IMSAI 8080 microcomputer system.
//! This emulator uses the Intel 8080 CPU core and implements the
//! necessary hardware components to simulate the IMSAI 8080.

#![warn(missing_docs)]

/// BIOS implementation
pub mod bios;
/// CP/M loader and execution
pub mod cpm;
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
pub use cpm::CpMLoader;
pub use emulator::Imsai8080;
pub use memory::Memory;

/// Create a new IMSAI 8080 emulator instance
pub fn new() -> Imsai8080 {
    Imsai8080::new()
}
