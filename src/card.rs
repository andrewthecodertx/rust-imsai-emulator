//! S-100 bus card trait and standard card implementations
//!
//! In a real IMSAI 8080, the S-100 bus is a passive backplane. Cards plug in
//! and communicate via address lines, data lines, and control signals. Each
//! card owns a range of I/O ports and/or memory addresses and responds to
//! bus transactions on those addresses/ports.
//!
//! The Card trait is the common interface. MemoryCard owns the full 64K
//! address space. ConsoleCard owns ports 0x00-0x01. TarbellCard owns
//! ports 0x48-0x4B. The bus dispatches to the first card that claims
//! a given address or port.

use crate::io::Keyboard;
use crate::io::VideoDisplay;
use crate::io::TarbellController;

/// An S-100 bus card.
///
/// Cards respond to two kinds of bus transactions:
/// - Memory transactions (mem_read/mem_write) for the address bus
/// - I/O transactions (io_read/io_write) for the port bus
///
/// A memory card (RAM) responds to memory transactions.
/// A peripheral card (Tarbell, console) responds to I/O transactions.
/// Some cards could do both (e.g., memory-mapped I/O).
pub trait Card {
    /// Read from an I/O port this card owns.
    fn io_read(&mut self, port: u8) -> u8;
    /// Write to an I/O port this card owns.
    fn io_write(&mut self, port: u8, value: u8);
    /// Does this card respond to the given I/O port?
    fn owns_port(&self, port: u8) -> bool;

    /// Read from a memory address this card owns.
    fn mem_read(&self, addr: u16) -> Option<u8>;
    /// Write to a memory address this card owns.
    fn mem_write(&mut self, addr: u16, value: u8) -> bool;
    /// Does this card own the given memory address?
    fn owns_address(&self, addr: u16) -> bool;

    /// Human-readable name for diagnostics.
    fn name(&self) -> &'static str;
    /// Downcast support (mutable).
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    /// Downcast support (immutable).
    fn as_any(&self) -> &dyn std::any::Any;
}

// ---------------------------------------------------------------------------
// Memory Card — full 64K address space
// ---------------------------------------------------------------------------

/// Memory card: 64KB static RAM on the S-100 bus.
///
/// In a real IMSAI, you'd have 4-8 RAM cards (8K or 16K each) each decoding
/// their own address range. For simplicity, one card owns the whole 64K.
/// Memory cards don't respond to any I/O ports.
pub struct MemoryCard {
    /// 64K RAM, initialized to 0xFF (unused bus state)
    pub ram: [u8; 65536],
}

impl MemoryCard {
    pub fn new() -> Self {
        Self { ram: [0xFF; 65536] }
    }

    pub fn new_zeroed() -> Self {
        Self { ram: [0x00; 65536] }
    }

    pub fn read(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }
}

impl Default for MemoryCard {
    fn default() -> Self { Self::new() }
}

impl Card for MemoryCard {
    fn io_read(&mut self, _port: u8) -> u8 { 0xFF }
    fn io_write(&mut self, _port: u8, _value: u8) {}
    fn owns_port(&self, _port: u8) -> bool { false }

    fn mem_read(&self, addr: u16) -> Option<u8> {
        Some(self.ram[addr as usize])
    }
    fn mem_write(&mut self, addr: u16, value: u8) -> bool {
        self.ram[addr as usize] = value;
        true
    }
    fn owns_address(&self, _addr: u16) -> bool { true }

    fn name(&self) -> &'static str { "64K Memory" }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// Console Card — ports 0x00-0x01, 0x79, 0x7B
// ---------------------------------------------------------------------------

/// Console card combining keyboard input and video display output.
pub struct ConsoleCard {
    pub keyboard: Keyboard,
    pub video: VideoDisplay,
}

impl ConsoleCard {
    pub fn new() -> Self {
        Self {
            keyboard: Keyboard::new(),
            video: VideoDisplay::new(80, 24),
        }
    }

    pub fn type_text(&mut self, text: &str) { self.keyboard.type_text(text); }
    pub fn is_key_ready(&self) -> bool { self.keyboard.is_char_ready() }
    pub fn video(&self) -> &VideoDisplay { &self.video }
    pub fn video_mut(&mut self) -> &mut VideoDisplay { &mut self.video }
    pub fn set_auto_render(&mut self, enabled: bool) { self.video.auto_render = enabled; }
}

impl Default for ConsoleCard {
    fn default() -> Self { Self::new() }
}

impl Card for ConsoleCard {
    fn io_read(&mut self, port: u8) -> u8 {
        match port {
            0x00 => self.keyboard.read_char(),
            0x01 | 0x79 => {
                let mut status = 0x02;
                if self.keyboard.is_char_ready() { status |= 0x01; }
                status
            }
            _ => 0xFF,
        }
    }
    fn io_write(&mut self, port: u8, value: u8) {
        if port == 0x00 || port == 0x7B {
            self.video.write_char(value);
            if self.video.auto_render { self.video.render(); }
        }
    }
    fn owns_port(&self, port: u8) -> bool {
        port == 0x00 || port == 0x01 || port == 0x79 || port == 0x7B
    }
    fn mem_read(&self, _addr: u16) -> Option<u8> { None }
    fn mem_write(&mut self, _addr: u16, _value: u8) -> bool { false }
    fn owns_address(&self, _addr: u16) -> bool { false }

    fn name(&self) -> &'static str { "Console" }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// Tarbell Disk Controller Card — ports 0x48-0x4B, 0xF8-0xFF
// ---------------------------------------------------------------------------

/// Tarbell 1011/1011B floppy disk controller card.
pub struct TarbellCard {
    controller: TarbellController,
}

impl TarbellCard {
    pub fn new() -> Self { Self { controller: TarbellController::new() } }
    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        self.controller.insert_disk(drive, path)
    }
    pub fn get_disk(&self, drive: usize) -> Option<&crate::disk::DiskImage> {
        self.controller.get_disk(drive)
    }
    pub fn current_track(&self) -> u8 { self.controller.current_track() }
    pub fn current_sector(&self) -> u8 { self.controller.current_sector() }
}

impl Default for TarbellCard {
    fn default() -> Self { Self::new() }
}

impl Card for TarbellCard {
    fn io_read(&mut self, port: u8) -> u8 {
        let tarbell_port = match port {
            0xF8 => 0x48, 0xF9 => 0x49, 0xFA => 0x4A, 0xFB => 0x4B,
            0xFC => return self.controller.wait_port_value(),
            0xFD => return 0x00,
            0xFF => return 0x03,
            _ => port,
        };
        self.controller.io_in(tarbell_port)
    }
    fn io_write(&mut self, port: u8, value: u8) {
        let tarbell_port = match port {
            0xF8 => 0x48, 0xF9 => 0x49, 0xFA => 0x4A, 0xFB => 0x4B,
            0xFC | 0xFD | 0xFF => return,
            _ => port,
        };
        self.controller.io_out(tarbell_port, value);
    }
    fn owns_port(&self, port: u8) -> bool {
        (0x48..=0x4B).contains(&port) || (0xF8..=0xFD).contains(&port) || port == 0xFF
    }
    fn mem_read(&self, _addr: u16) -> Option<u8> { None }
    fn mem_write(&mut self, _addr: u16, _value: u8) -> bool { false }
    fn owns_address(&self, _addr: u16) -> bool { false }

    fn name(&self) -> &'static str { "Tarbell" }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_any(&self) -> &dyn std::any::Any { self }
}