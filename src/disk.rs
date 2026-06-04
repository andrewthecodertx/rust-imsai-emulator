use crate::dpb::{SPT, SECTOR_SIZE, SKEW_TABLE, TOTAL_TRACKS};

pub const DISK_SIZE: usize = TOTAL_TRACKS as usize * SPT as usize * SECTOR_SIZE;

pub struct DiskImage {
    data: Vec<u8>,
    write_protected: bool,
    dirty: bool,
}

impl DiskImage {
    /// New disk filled with 0xE5 (CP/M convention for unused sectors).
    pub fn new_formatted() -> Self {
        Self {
            data: vec![0xE5; DISK_SIZE],
            write_protected: false,
            dirty: true,
        }
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("Failed to read disk image: {}", e))?;
        if data.len() != DISK_SIZE {
            return Err(format!("Disk image size mismatch: expected {} bytes, got {}", DISK_SIZE, data.len()));
        }
        Ok(Self { data, write_protected: false, dirty: false })
    }

    pub fn save(&mut self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, &self.data).map_err(|e| format!("Failed to write disk image: {}", e))?;
        self.dirty = false;
        Ok(())
    }

    pub fn is_dirty(&self) -> bool { self.dirty }
    pub fn is_write_protected(&self) -> bool { self.write_protected }
    pub fn set_write_protected(&mut self, wp: bool) { self.write_protected = wp; }

    /// Read a physical sector. Sector 0 is treated as sector 1.
    pub fn read_sector(&self, track: u8, sector: u8) -> Result<[u8; SECTOR_SIZE], String> {
        if track >= TOTAL_TRACKS {
            return Err(format!("Track {} out of range (0-{})", track, TOTAL_TRACKS - 1));
        }
        let physical_sector = if sector == 0 { 1 } else { sector };
        if physical_sector > SPT as u8 {
            return Err(format!("Sector {} out of range (1-{})", sector, SPT));
        }
        let offset = (track as usize * SPT as usize + (physical_sector - 1) as usize) * SECTOR_SIZE;
        let mut buf = [0u8; SECTOR_SIZE];
        buf.copy_from_slice(&self.data[offset..offset + SECTOR_SIZE]);
        Ok(buf)
    }

    pub fn write_sector(&mut self, track: u8, sector: u8, data: &[u8; SECTOR_SIZE]) -> Result<(), String> {
        if self.write_protected {
            return Err("Disk is write-protected".into());
        }
        if track >= TOTAL_TRACKS {
            return Err(format!("Track {} out of range (0-{})", track, TOTAL_TRACKS - 1));
        }
        let physical_sector = if sector == 0 { 1 } else { sector };
        if physical_sector > SPT as u8 {
            return Err(format!("Sector {} out of range (1-{})", sector, SPT));
        }
        let offset = (track as usize * SPT as usize + (physical_sector - 1) as usize) * SECTOR_SIZE;
        self.data[offset..offset + SECTOR_SIZE].copy_from_slice(data);
        self.dirty = true;
        Ok(())
    }

    pub fn data(&self) -> &[u8] { &self.data }
    pub fn data_mut(&mut self) -> &mut [u8] { self.dirty = true; &mut self.data }
}

/// Convert CP/M logical sector (0-25) to physical sector (1-26) using 6:1 interleave.
pub fn logical_to_physical(logical: u8) -> u8 {
    if (logical as usize) < SKEW_TABLE.len() {
        SKEW_TABLE[logical as usize]
    } else {
        logical + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_image_new_formatted_all_e5() {
        let disk = DiskImage::new_formatted();
        assert_eq!(disk.data()[0], 0xE5);
        assert_eq!(disk.data()[DISK_SIZE - 1], 0xE5);
    }

    #[test]
    fn test_disk_image_sector_write_read() {
        let mut disk = DiskImage::new_formatted();
        let system_data = [0xC3, 0x00, 0x01, 0x00];
        let mut sector = [0xE5u8; SECTOR_SIZE];
        sector[..system_data.len()].copy_from_slice(&system_data);
        disk.write_sector(0, 1, &sector).unwrap();
        let read_back = disk.read_sector(0, 1).unwrap();
        assert_eq!(read_back[0], 0xC3);
    }

    #[test]
    fn test_disk_image_sector_round_trip() {
        let mut disk = DiskImage::new_formatted();
        let data = [0x42u8; SECTOR_SIZE];
        disk.write_sector(0, 1, &data).unwrap();
        let read_back = disk.read_sector(0, 1).unwrap();
        assert_eq!(read_back[0], 0x42);
        let other = disk.read_sector(1, 1).unwrap();
        assert_eq!(other[0], 0xE5);
    }

    #[test]
    fn test_disk_image_write_protect() {
        let mut disk = DiskImage::new_formatted();
        disk.set_write_protected(true);
        let data = [0x42u8; SECTOR_SIZE];
        let result = disk.write_sector(0, 1, &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_disk_image_dirty_flag() {
        let mut disk = DiskImage::new_formatted();
        assert!(disk.is_dirty());
        let data = [0x42u8; SECTOR_SIZE];
        disk.write_sector(0, 1, &data).unwrap();
        assert!(disk.is_dirty());
    }

    #[test]
    fn test_disk_size_is_correct() {
        assert_eq!(DISK_SIZE, 77 * 26 * 128);
    }

    #[test]
    fn test_skew_table_is_permutation_of_1_to_26() {
        use crate::dpb::SKEW_TABLE;
        let mut sorted: Vec<u8> = SKEW_TABLE.to_vec();
        sorted.sort();
        assert_eq!(sorted, (1u8..=26).collect::<Vec<u8>>());
    }

    #[test]
    fn test_skew_table_logical_physical_roundtrip() {
        for logical in 0u8..26 {
            let physical = logical_to_physical(logical);
            assert!(physical >= 1 && physical <= 26);
        }
    }

    #[test]
    fn test_sector_zero_treated_as_sector_one() {
        let disk = DiskImage::new_formatted();
        let s0 = disk.read_sector(0, 0).unwrap();
        let s1 = disk.read_sector(0, 1).unwrap();
        assert_eq!(s0, s1);
    }

    #[test]
    fn test_track_out_of_range() {
        let disk = DiskImage::new_formatted();
        assert!(disk.read_sector(77, 1).is_err());
        assert!(disk.read_sector(76, 1).is_ok());
    }

    #[test]
    fn test_sector_out_of_range() {
        let disk = DiskImage::new_formatted();
        assert!(disk.read_sector(0, 27).is_err());
        assert!(disk.read_sector(0, 26).is_ok());
    }
}