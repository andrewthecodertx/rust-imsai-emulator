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

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Sparse segment of non-0xFF memory, serialized as hex strings.
#[derive(Serialize, Deserialize)]
struct MemorySegment {
    addr: u16,
    data: String,
}

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

/// Save memory to a JSON file. Only non-0xFF regions are stored (sparse format).
pub fn save_memory_to_file(ram: &[u8; 65536], path: &Path) -> std::io::Result<()> {
    let mut segments: Vec<MemorySegment> = Vec::new();
    let mut i: usize = 0;
    while i < 65536 {
        if ram[i] != 0xFF {
            // Start of a non-0xFF region
            let start = i;
            let mut end = i;
            while end < 65536 && ram[end] != 0xFF {
                end += 1;
            }
            // Encode as hex string for compactness
            let hex: String = ram[start..end]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect();
            segments.push(MemorySegment {
                addr: start as u16,
                data: hex,
            });
            i = end;
        } else {
            i += 1;
        }
    }
    let json = serde_json::to_string_pretty(&segments)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(path, json)
}

/// Load memory from a JSON file. Regions not present in the file stay 0xFF.
pub fn load_memory_from_file(ram: &mut [u8; 65536], path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let contents = std::fs::read_to_string(path)?;
    let segments: Vec<MemorySegment> = serde_json::from_str(&contents)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    for seg in segments {
        let bytes: Vec<u8> = (0..seg.data.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&seg.data[i..i+2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let start = seg.addr as usize;
        if start + bytes.len() <= 65536 {
            ram[start..start + bytes.len()].copy_from_slice(&bytes);
        }
    }
    Ok(())
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

    #[test]
    fn test_save_load_round_trip() {
        let dir = std::env::temp_dir().join("imsai_memory_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_memory.json");

        let mut ram = [0xFFu8; 65536];
        // Write some data at scattered addresses
        ram[0x0000] = 0xC3; // JMP
        ram[0x0001] = 0x00;
        ram[0x0002] = 0x01;
        ram[0x0100] = 0x3E; // MVI A
        ram[0x0101] = 0x41; // 'A'
        ram[0xFFFF] = 0x00;

        save_memory_to_file(&ram, &path).unwrap();
        assert!(path.exists());

        // Load into fresh RAM
        let mut ram2 = [0xFFu8; 65536];
        load_memory_from_file(&mut ram2, &path).unwrap();
        assert_eq!(ram2[0x0000], 0xC3);
        assert_eq!(ram2[0x0001], 0x00);
        assert_eq!(ram2[0x0002], 0x01);
        assert_eq!(ram2[0x0100], 0x3E);
        assert_eq!(ram2[0x0101], 0x41);
        assert_eq!(ram2[0xFFFF], 0x00);
        // 0xFF regions should remain untouched
        assert_eq!(ram2[0x0200], 0xFF);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_save_empty_memory_produces_tiny_file() {
        let dir = std::env::temp_dir().join("imsai_memory_test_empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty_memory.json");

        let ram = [0xFFu8; 65536];
        save_memory_to_file(&ram, &path).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        // Empty memory produces just "[]"
        assert_eq!(contents.trim(), "[]");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_nonexistent_file_is_ok() {
        let mut ram = [0xFFu8; 65536];
        let result = load_memory_from_file(&mut ram, Path::new("/nonexistent/file.json"));
        assert!(result.is_ok());
        // RAM should still be all 0xFF
        assert_eq!(ram[0], 0xFF);
    }
}