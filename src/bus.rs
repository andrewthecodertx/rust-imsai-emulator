//! S-100 bus implementation for the IMSAI 8080 emulator
//!
//! Implements the intel8080 Bus trait, connecting the CPU to memory
//! and I/O devices. I/O port mapping:
//!
//! - Port 0x00: Console data (read: keyboard, write: display)
//! - Port 0x01: Console status (read: bit 0 = key ready, bit 1 = display ready)
//! - Port 0x48-0x4B: Tarbell disk controller (ports 0-3)
//! - Port 0x79: CMI5619 console status (aliased to port 0x01)
//! - Port 0x7B: CMI5619 console data (aliased to port 0x00)
//! - Port 0xF8-0xFB: CMI5619 disk controller (aliased to Tarbell ports 0-3)
//! - Port 0xFC: CMI5619 disk wait/DRQ
//! - Port 0xFD: CMI5619 DMA check / extended disk latch
//! - Port 0xFF: IMSAI front panel / CMI5619 SIO control

use intel8080::Bus;

use crate::io::IoController;
use crate::memory::Memory;

/// Console data port (standard)
pub const PORT_CONSOLE_DATA: u8 = 0x00;
/// Console status port (standard)
pub const PORT_CONSOLE_STATUS: u8 = 0x01;

/// CMI5619 console status port
const CMI5619_CONSTAT: u8 = 0x79;
/// CMI5619 console data port
const CMI5619_CDATA: u8 = 0x7B;

/// Tarbell controller port base
const TARBELL_BASE: u8 = 0x48;
/// Tarbell controller port count (4 ports: 0x48-0x4B)
const TARBELL_COUNT: u8 = 4;

/// CMI5619 disk controller port base (aliased to Tarbell)
const CMI5619_DISK_BASE: u8 = 0xF8;
/// CMI5619 disk port count (4 ports: 0xF8-0xFB)
const CMI5619_DISK_COUNT: u8 = 4;

/// CMI5619 disk wait/DRQ port
const CMI5619_WAIT_PORT: u8 = 0xFC;
/// CMI5619 DMA check / extended disk latch
const CMI5619_DCONT_PORT: u8 = 0xFD;

/// IMSAI front panel / CMI5619 SIO control
const PANEL_PORT: u8 = 0xFF;

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
    pub fn new() -> Self {
        Self {
            memory: Memory::new(),
            io: IoController::new(),
        }
    }

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
            // Standard console
            PORT_CONSOLE_DATA => self.io.keyboard.read_char(),
            PORT_CONSOLE_STATUS => {
                let mut status = STATUS_DISPLAY_READY;
                if self.io.keyboard.is_char_ready() {
                    status |= STATUS_KEY_READY;
                }
                status
            }
            // CMI5619 console ports (aliased to our console)
            CMI5619_CONSTAT => {
                let mut status = STATUS_DISPLAY_READY;
                if self.io.keyboard.is_char_ready() {
                    status |= STATUS_KEY_READY;
                }
                status
            }
            CMI5619_CDATA => self.io.keyboard.read_char(),
            // Tarbell controller ports (0x48-0x4B)
            p if (TARBELL_BASE..TARBELL_BASE + TARBELL_COUNT).contains(&p) => {
                self.io.tarbell.io_in(p)
            }
            // CMI5619 disk ports (0xF8-0xFB) — alias to Tarbell
            p if (CMI5619_DISK_BASE..CMI5619_DISK_BASE + CMI5619_DISK_COUNT).contains(&p) => {
                let tarbell_port = TARBELL_BASE + (p - CMI5619_DISK_BASE);
                self.io.tarbell.io_in(tarbell_port)
            }
            // CMI5619 WAIT port: DRQ signal
            CMI5619_WAIT_PORT => self.io.tarbell.wait_port_value(),
            // CMI5619 DMA check / ext latch
            CMI5619_DCONT_PORT => 0x00,
            // CMI5619 SIO control / front panel: ready status
            PANEL_PORT => STATUS_DISPLAY_READY | STATUS_KEY_READY,
            // DMA ports (0xE0-0xE8): not implemented
            0xE0 | 0xE1 | 0xE8 => 0x00,
            _ => 0xFF,
        }
    }

    fn io_out(&mut self, port: u8, value: u8) {
        match port {
            // Standard console
            PORT_CONSOLE_DATA => {
                self.io.video.write_char(value);
                self.io.video.render();
            }
            // CMI5619 console data
            CMI5619_CDATA => {
                self.io.video.write_char(value);
                self.io.video.render();
            }
            // Tarbell controller ports (0x48-0x4B)
            p if (TARBELL_BASE..TARBELL_BASE + TARBELL_COUNT).contains(&p) => {
                self.io.tarbell.io_out(p, value);
            }
            // CMI5619 disk ports (0xF8-0xFB)
            p if (CMI5619_DISK_BASE..CMI5619_DISK_BASE + CMI5619_DISK_COUNT).contains(&p) => {
                let tarbell_port = TARBELL_BASE + (p - CMI5619_DISK_BASE);
                self.io.tarbell.io_out(tarbell_port, value);
            }
            // CMI5619 WAIT/DCONT/SIO/front panel — write ignored
            CMI5619_WAIT_PORT | CMI5619_DCONT_PORT | PANEL_PORT => {}
            // DMA ports — ignored
            0xE0 | 0xE1 | 0xE8 => {}
            _ => {}
        }
    }
}