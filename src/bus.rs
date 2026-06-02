//! S-100 system bus implementation for the IMSAI 8080 emulator
//!
//! The S-100 bus is a passive backplane. Cards plug in and communicate
//! via address lines, data lines, and control signals. The bus dispatches
//! each transaction to the first card that claims it.
//!
//! Default card configuration (matching a real IMSAI 8080):
//! - MemoryCard: 64K RAM (full address space)
//! - SerialCard: IMSAI SIO-2 (2x 8251A UART, ports 0x00-0x03)
//! - TarbellCard: floppy disk controller (FD1771, ports 0x48-0x4B)

use intel8080::Bus;
use crate::cards::{Card, MemoryCard, SerialCard, TarbellCard};

/// The S-100 system bus: a passive backplane connecting CPU and cards.
pub struct ImsaiBus {
    cards: Vec<Box<dyn Card>>,
}

impl ImsaiBus {
    /// Create a new bus with the standard IMSAI card set:
    /// MemoryCard, SerialCard, and TarbellCard.
    pub fn new() -> Self {
        let mut bus = Self { cards: Vec::new() };
        bus.insert_card(Box::new(MemoryCard::new()));
        bus.insert_card(Box::new(SerialCard::new()));
        bus.insert_card(Box::new(TarbellCard::new()));
        bus
    }

    /// Insert an S-100 card into the bus.
    pub fn insert_card(&mut self, card: Box<dyn Card>) {
        self.cards.push(card);
    }

    /// Load a block of data into memory at the given address.
    pub fn load(&mut self, start: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.mem_write(start.wrapping_add(i as u16), byte);
        }
    }

    /// Read a byte from memory.
    pub fn mem_read(&self, addr: u16) -> u8 {
        for card in &self.cards {
            if card.owns_address(addr) {
                return card.mem_read(addr).unwrap_or(0xFF);
            }
        }
        0xFF
    }

    /// Write a byte to memory.
    pub fn mem_write(&mut self, addr: u16, value: u8) {
        for card in &mut self.cards {
            if card.owns_address(addr) {
                card.mem_write(addr, value);
                return;
            }
        }
    }

    /// Get a mutable reference to the serial card.
    /// This is the primary interface for console I/O.
    pub fn serial(&mut self) -> &mut SerialCard {
        self.card_mut::<SerialCard>().expect("Serial card not installed")
    }

    /// Convenience alias for serial(). Used by code that thinks in
    /// terms of "the console".
    pub fn console(&mut self) -> &mut SerialCard {
        self.serial()
    }

    /// Get a mutable reference to the Tarbell disk card.
    pub fn tarbell(&mut self) -> &mut TarbellCard {
        self.card_mut::<TarbellCard>().expect("Tarbell card not installed")
    }

    /// Get a mutable reference to the memory card.
    pub fn memory(&mut self) -> &mut MemoryCard {
        self.card_mut::<MemoryCard>().expect("Memory card not installed")
    }

    /// Find the first card of a specific type.
    pub fn card_mut<T: Card + 'static>(&mut self) -> Option<&mut T> {
        for card in &mut self.cards {
            if let Some(typed) = card.as_any_mut().downcast_mut::<T>() {
                return Some(typed);
            }
        }
        None
    }
}

impl Default for ImsaiBus {
    fn default() -> Self { Self::new() }
}

impl Bus for ImsaiBus {
    fn mem_read(&self, addr: u16) -> u8 {
        self.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    fn io_in(&mut self, port: u8) -> u8 {
        for card in &mut self.cards {
            if card.owns_port(port) {
                return card.io_read(port);
            }
        }
        0xFF
    }

    fn io_out(&mut self, port: u8, value: u8) {
        for card in &mut self.cards {
            if card.owns_port(port) {
                card.io_write(port, value);
                return;
            }
        }
    }
}