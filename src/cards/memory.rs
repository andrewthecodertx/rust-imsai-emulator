
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

/// Save memory to a JSON file. Only non-0xFF regions are stored (sparse format).
pub fn save_memory_to_file(ram: &[u8; 65536], path: &Path) -> std::io::Result<()> {
    let mut segments: Vec<MemorySegment> = Vec::new();
    let mut i: usize = 0;
    while i < 65536 {
        if ram[i] != 0xFF {
            let start = i;
            let mut end = i;
            while end < 65536 && ram[end] != 0xFF {
                end += 1;
            }
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
        if seg.data.len() % 2 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Odd-length hex string at address 0x{:04X}", seg.addr),
            ));
        }
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
    fn test_save_load_round_trip() {
        let dir = std::env::temp_dir().join("imsai_memory_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_memory.json");

        let mut ram = [0xFFu8; 65536];
        ram[0x0000] = 0xC3;
        ram[0x0001] = 0x00;
        ram[0x0002] = 0x01;
        ram[0x0100] = 0x3E;
        ram[0x0101] = 0x41;
        ram[0xFFFF] = 0x00;

        save_memory_to_file(&ram, &path).unwrap();
        assert!(path.exists());

        let mut ram2 = [0xFFu8; 65536];
        load_memory_from_file(&mut ram2, &path).unwrap();
        assert_eq!(ram2[0x0000], 0xC3);
        assert_eq!(ram2[0x0001], 0x00);
        assert_eq!(ram2[0x0002], 0x01);
        assert_eq!(ram2[0x0100], 0x3E);
        assert_eq!(ram2[0x0101], 0x41);
        assert_eq!(ram2[0xFFFF], 0x00);
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
        assert_eq!(contents.trim(), "[]");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_nonexistent_file_is_ok() {
        let mut ram = [0xFFu8; 65536];
        let result = load_memory_from_file(&mut ram, Path::new("/nonexistent/file.json"));
        assert!(result.is_ok());
        assert_eq!(ram[0], 0xFF);
    }

    #[test]
    fn test_load_rejects_odd_length_hex() {
        let dir = std::env::temp_dir().join("imsai_memory_test_odd_hex");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("odd_hex.json");

        std::fs::write(&path, r#"[{"addr":0,"data":"ABC"}]"#).unwrap();

        let mut ram = [0xFFu8; 65536];
        let result = load_memory_from_file(&mut ram, &path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

        std::fs::remove_dir_all(&dir).ok();
    }
}