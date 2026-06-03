//! S-100 system bus implementation for the IMSAI 8080 emulator
//!
//! The S-100 bus is a passive backplane. Cards plug in and communicate
//! via address lines, data lines, and control signals.
//!
//! The IMSAI 8080 has a fixed card configuration known at compile time:
//! - MemoryCard: 64K RAM (full address space) -- inlined for speed
//! - SerialCard: IMSAI SIO-2 (2x 8251A UART, ports 0x00-0x03)
//! - TarbellCard: floppy disk controller (FD1771, ports 0x48-0x4B)
//!
//! Memory accesses go directly to the RAM array (no dispatch). I/O
//! accesses dispatch by port number using a match statement.

use intel8080::Bus;
use crate::cards::{MemoryCard, SerialCard, TarbellCard};

/// The S-100 system bus: a passive backplane connecting CPU and cards.
///
/// Cards are stored inline (not behind trait objects) so memory
/// accesses bypass any dynamic dispatch. I/O dispatches by port number
/// with a match statement.
pub struct ImsaiBus {
    /// 64K RAM -- always present, accessed directly
    pub memory: MemoryCard,
    /// IMSAI SIO-2 serial card (console I/O)
    serial: SerialCard,
    /// Tarbell 1011 floppy controller
    tarbell: TarbellCard,
}

impl ImsaiBus {
    /// Create a new bus with the standard IMSAI card set:
    /// MemoryCard, SerialCard, and TarbellCard.
    pub fn new() -> Self {
        Self {
            memory: MemoryCard::new(),
            serial: SerialCard::new(),
            tarbell: TarbellCard::new(),
        }
    }

    /// Load a block of data into memory at the given address.
    pub fn load(&mut self, start: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.memory.ram[start.wrapping_add(i as u16) as usize] = byte;
        }
    }

    /// Read a byte from memory (direct RAM access, no dispatch).
    pub fn mem_read(&self, addr: u16) -> u8 {
        self.memory.ram[addr as usize]
    }

    /// Write a byte to memory (direct RAM access, no dispatch).
    pub fn mem_write(&mut self, addr: u16, value: u8) {
        self.memory.ram[addr as usize] = value;
    }

    /// Get a mutable reference to the serial card.
    pub fn serial(&mut self) -> &mut SerialCard {
        &mut self.serial
    }

    /// Convenience alias for serial(). Used by code that thinks in
    /// terms of "the console".
    pub fn console(&mut self) -> &mut SerialCard {
        &mut self.serial
    }

    /// Get a mutable reference to the Tarbell disk card.
    pub fn tarbell(&mut self) -> &mut TarbellCard {
        &mut self.tarbell
    }

    /// Get a reference to the Tarbell disk card.
    pub fn tarbell_ref(&self) -> &TarbellCard {
        &self.tarbell
    }

    /// Get a mutable reference to the memory card.
    pub fn memory_mut(&mut self) -> &mut MemoryCard {
        &mut self.memory
    }

    /// Insert a disk image into a drive on the Tarbell card.
    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        self.tarbell.insert_disk(drive, path)
    }
}

impl Default for ImsaiBus {
    fn default() -> Self { Self::new() }
}

/// I/O port dispatch for the Intel 8080 bus trait.
///
/// Port mapping (known at compile time):
///
/// | Port(s)       | Card       | Device                |
/// |---------------|------------|-----------------------|
/// | 0x00-0x03     | SerialCard | 8251A UART channels   |
/// | 0x48-0x4B     | TarbellCard| FD1771 registers      |
/// | 0x79, 0x7B    | SerialCard | UART aliases          |
/// | 0xF8-0xFB     | TarbellCard| FD1771 aliases         |
/// | 0xFC-0xFF     | TarbellCard| Auxiliary ports        |
///
/// Unclaimed ports return 0xFF on read and are ignored on write,
/// matching the S-100 bus floating behavior.
impl Bus for ImsaiBus {
    fn mem_read(&self, addr: u16) -> u8 {
        self.memory.ram[addr as usize]
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        self.memory.ram[addr as usize] = value;
    }

    fn io_in(&mut self, port: u8) -> u8 {
        match port {
            // Serial card: Channel A data
            0x00 | 0x7B => self.serial.io_read(port),
            // Serial card: Channel A status
            0x01 | 0x79 => self.serial.io_read(port),
            // Serial card: Channel B data
            0x02 => self.serial.io_read(port),
            // Serial card: Channel B status
            0x03 => self.serial.io_read(port),
            // Tarbell card: FD1771 + auxiliary
            0x48..=0x4B | 0xF8..=0xFF => self.tarbell.io_read(port),
            _ => 0xFF,
        }
    }

    fn io_out(&mut self, port: u8, value: u8) {
        match port {
            0x00 | 0x7B => self.serial.io_write(port, value),
            0x01 | 0x79 => self.serial.io_write(port, value),
            0x02 => self.serial.io_write(port, value),
            0x03 => self.serial.io_write(port, value),
            0x48..=0x4B | 0xF8..=0xFF => self.tarbell.io_write(port, value),
            _ => {}
        }
    }
}