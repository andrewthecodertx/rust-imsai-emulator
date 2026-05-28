//! S-100 bus implementation for the IMSAI 8080 emulator
//!
//! Implements the intel8080 Bus trait, connecting the CPU to memory
//! and I/O devices. I/O port mapping:
//!
//! - Port 0x00: Console data (read: keyboard, write: display)
//! - Port 0x01: Console status (read: bit 0 = key ready, bit 1 = display ready)
//! - Port 0x48: Tarbell disk controller: command/status register
//! - Port 0x49: Tarbell disk controller: track register
//! - Port 0x4A: Tarbell disk controller: sector register
//! - Port 0x4B: Tarbell disk controller: data register

use intel8080::Bus;

use crate::io::IoController;
use crate::memory::Memory;

/// I/O port for CP/M BIOS re-install trigger (magic port)
/// When the WBOOT routine writes to this port, the emulator
/// re-installs the emulator-compatible BIOS after loading system tracks.
const PORT_BIOS_REINSTALL: u8 = 0xFE;

/// Console data port
pub const PORT_CONSOLE_DATA: u8 = 0x00;
/// Console status port
pub const PORT_CONSOLE_STATUS: u8 = 0x01;

/// Tarbell controller port base
const TARBELL_BASE: u8 = 0x48;
/// Tarbell controller port count (4 ports: 0x48-0x4B)
const TARBELL_COUNT: u8 = 4;

/// Status register bits
const STATUS_KEY_READY: u8 = 0x01;
const STATUS_DISPLAY_READY: u8 = 0x02;

/// The S-100 system bus connecting CPU, memory, and I/O
pub struct ImsaiBus {
    /// 64KB addressable memory
    pub memory: Memory,
    /// I/O controller (keyboard, display, disk)
    pub io: IoController,
}

impl ImsaiBus {
    /// Create a new bus with default memory and I/O
    pub fn new() -> Self {
        Self {
            memory: Memory::new(),
            io: IoController::new(),
        }
    }

    /// Load a ROM or program binary into memory at the given start address
    pub fn load(&mut self, start: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.memory.write(start + i as u16, byte);
        }
    }
}

impl Default for ImsaiBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus for ImsaiBus {
    fn mem_read(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value)
    }

    fn io_in(&mut self, port: u8) -> u8 {
        match port {
            PORT_CONSOLE_DATA => self.io.keyboard.read_char(),
            PORT_CONSOLE_STATUS => {
                let mut status = STATUS_DISPLAY_READY;
                if self.io.keyboard.is_char_ready() {
                    status |= STATUS_KEY_READY;
                }
                status
            }
            p if (TARBELL_BASE..TARBELL_BASE + TARBELL_COUNT).contains(&p) => {
                self.io.tarbell.io_in(p)
            }
            _ => 0xFF,
        }
    }

    fn io_out(&mut self, port: u8, value: u8) {
        match port {
            PORT_CONSOLE_DATA => {
                self.io.video.write_char(value);
                self.io.video.render();
            }
            PORT_BIOS_REINSTALL => {
                // Magic port: WBOOT signals that system tracks have been
                // reloaded from disk. Re-install our emulator-compatible BIOS
                // because the loaded data overwrites our patches with hardware-
                // specific code that won't work with our emulator.
                crate::CpmBios::install(self);
            }
            p if (TARBELL_BASE..TARBELL_BASE + TARBELL_COUNT).contains(&p) => {
                self.io.tarbell.io_out(p, value);
            }
            _ => {}
        }
    }
}