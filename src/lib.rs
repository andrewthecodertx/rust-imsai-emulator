//! # IMSAI 8080 Emulator
//!
//! A Rust-based emulator for the IMSAI 8080 microcomputer system.
//! Implements hardware-accurate chip and card models for the S-100 bus.
//!
//! Architecture:
//! - Chips: silicon-level models (8251A UART, FD1771 FDC)
//! - Cards: S-100 bus cards composed from chip models
//! - Bus: passive S-100 backplane connecting CPU and cards
//! - Imsai8080: top-level struct holding CPU + bus

#![warn(missing_docs)]

/// S-100 bus card implementations
pub mod cards;
/// Chip-level models (8251A UART, FD1771 FDC)
pub mod chips;
/// S-100 system bus
pub mod bus;
/// CP/M disk image creation and management
pub mod disk;
/// Disk parameter block definitions
pub mod dpb;
/// The main emulator system
pub mod emulator;
/// I/O subsystem (keyboard, video)
pub mod io;

// Re-export the main components
pub use bus::ImsaiBus;
pub use cards::{Card, FrontPanel, IoEvent, MemoryCard, PanelLeds, PanelSwitch, RunState, SerialCard, TarbellCard};
pub use cards::{save_memory_to_file, load_memory_from_file};
pub use chips::Fd1771;
pub use chips::Uart8251;
pub use disk::DiskImage;
pub use emulator::Imsai8080;
pub use io::Keyboard;
pub use io::VideoDisplay;