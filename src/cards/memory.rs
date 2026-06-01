//! Memory card: 64KB static RAM on the S-100 bus
//!
//! In a real IMSAI 8080, the 64KB RAM would be spread across multiple
//! S-100 RAM cards (typically 4-8 cards of 8K or 16K each), each decoding
//! their own address range. For now we model it as one card owning the
//! full 64K address space.
//!
//! A future enhancement would split this into configurable banked memory
//! cards that can be selectively enabled/disabled (e.g., for testing
//! partial RAM configurations or ROM overlays).

/// Memory card: 64KB static RAM on the S-100 bus.
///
/// Uninitialized bus reads return 0xFF (floating bus state).
/// The RAM array is initialized to 0xFF on creation.
pub struct MemoryCard {
    /// 64K RAM, initialized to 0xFF (floating bus state)
    pub ram: [u8; 65536],
}

impl MemoryCard {
    /// Create a new memory card with all bytes set to 0xFF (floating bus).
    pub fn new() -> Self {
        Self { ram: [0xFF; 65536] }
    }

    /// Create a new memory card with all bytes zeroed.
    pub fn new_zeroed() -> Self {
        Self { ram: [0x00; 65536] }
    }

    /// Read a byte from RAM.
    pub fn read(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    /// Write a byte to RAM.
    pub fn write(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }
}

impl Default for MemoryCard {
    fn default() -> Self { Self::new() }
}

impl super::Card for MemoryCard {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Card;

    #[test]
    fn test_memory_card_new_is_ff() {
        let card = MemoryCard::new();
        assert_eq!(card.read(0x0000), 0xFF);
        assert_eq!(card.read(0xFFFF), 0xFF);
    }

    #[test]
    fn test_memory_card_new_zeroed() {
        let card = MemoryCard::new_zeroed();
        assert_eq!(card.read(0x0000), 0x00);
        assert_eq!(card.read(0xFFFF), 0x00);
    }

    #[test]
    fn test_memory_card_read_write() {
        let mut card = MemoryCard::new();
        card.write(0x0100, 0x42);
        assert_eq!(card.read(0x0100), 0x42);
    }

    #[test]
    fn test_memory_card_bus_interface() {
        let mut card = MemoryCard::new();
        // Memory card doesn't own any I/O ports
        assert!(!card.owns_port(0x00));
        assert!(!card.owns_port(0xFF));
        // IO reads return 0xFF (no ports)
        assert_eq!(card.io_read(0x00), 0xFF);
        // Memory card owns all addresses
        assert!(card.owns_address(0x0000));
        assert!(card.owns_address(0xFFFF));
        // Bus read/write through Card trait
        assert_eq!(card.mem_read(0x0100).unwrap(), 0xFF);
        card.mem_write(0x0100, 0xAA);
        assert_eq!(card.mem_read(0x0100).unwrap(), 0xAA);
    }

    #[test]
    fn test_memory_card_name() {
        let card = MemoryCard::new();
        assert_eq!(card.name(), "64K Memory");
    }
}