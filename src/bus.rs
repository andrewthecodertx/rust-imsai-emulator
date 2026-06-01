//! S-100 bus implementation for the IMSAI 8080 emulator
//!
//! The S-100 bus is the passive backplane that connects all cards. The CPU
//! drives memory and I/O transactions on the bus, and cards respond to the
//! ports they own.
//!
//! The bus owns:
//! - 64KB memory (always present)
//! - A collection of S-100 cards (dynamic, pluggable)
//!
//! Cards are added at startup and the bus dispatches I/O operations to the
//! appropriate card based on port address. No card internals are accessed
//! directly through the bus.

use intel8080::Bus;
use crate::card::{Card, ConsoleCard, TarbellCard};
use crate::memory::Memory;

/// The S-100 system bus connecting CPU, memory, and I/O cards.
pub struct ImsaiBus {
    /// 64KB addressable memory
    pub memory: Memory,
    /// Pluggable S-100 cards
    cards: Vec<Box<dyn Card>>,
}

impl ImsaiBus {
    /// Create a new bus with empty memory and no cards.
    pub fn new() -> Self {
        Self {
            memory: Memory::new(),
            cards: Vec::new(),
        }
    }

    /// Insert an S-100 card into the bus.
    ///
    /// Cards are checked in insertion order. The first card that claims
    /// a port gets the I/O operation.
    pub fn insert_card(&mut self, card: Box<dyn Card>) {
        self.cards.push(card);
    }

    /// Load a block of data into memory at the given address.
    pub fn load(&mut self, start: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.memory.write(start + i as u16, byte);
        }
    }

    /// Get a reference to the console card (for keyboard/video access).
    pub fn console(&mut self) -> &mut ConsoleCard {
        self.card_mut::<ConsoleCard>().expect("Console card not installed")
    }

    /// Get a reference to the Tarbell disk card (for disk operations).
    pub fn tarbell(&mut self) -> &mut TarbellCard {
        self.card_mut::<TarbellCard>().expect("Tarbell card not installed")
    }

    /// Find the first card of a specific type.
    ///
    /// Returns a mutable reference to the card if found.
    /// Used by main.rs for boot loading (get disk to read system tracks)
    /// and by terminal mode (feed keyboard input, get display output).
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
        for card in &mut self.cards {
            if card.owns_port(port) {
                return card.io_read(port);
            }
        }
        0xFF // Unclaimed ports return 0xFF
    }

    fn io_out(&mut self, port: u8, value: u8) {
        for card in &mut self.cards {
            if card.owns_port(port) {
                card.io_write(port, value);
                return;
            }
        }
        // Unclaimed ports: ignore
    }
}