//! S-100 bus implementation for the IMSAI 8080 emulator
//!
//! Implements the intel8080 Bus trait, connecting the CPU to memory
//! and I/O devices. I/O port mapping:
//!
//! - Port 0x00: Console data (read: keyboard, write: display)
//! - Port 0x01: Console status (read: bit 0 = key ready, bit 1 = display ready)

use intel8080::Bus;

use crate::io::IoController;
use crate::memory::Memory;

/// I/O port addresses
const PORT_CONSOLE_DATA: u8 = 0x00;
const PORT_CONSOLE_STATUS: u8 = 0x01;

/// Status register bits
const STATUS_KEY_READY: u8 = 0x01;
const STATUS_DISPLAY_READY: u8 = 0x02;

/// The S-100 system bus connecting CPU, memory, and I/O
pub struct ImsaiBus {
    /// 64KB addressable memory
    pub memory: Memory,
    /// I/O controller (keyboard + display)
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
            _ => 0x00,
        }
    }

    fn io_out(&mut self, port: u8, value: u8) {
        if port == PORT_CONSOLE_DATA {
            self.io.video.write_char(value);
            self.io.video.render();
        }
    }
}