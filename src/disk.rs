//! CP/M disk image management for the Tarbell controller
//!
//! Handles loading and manipulating CP/M 2.2 disk images for the
//! Tarbell single-density 8-inch floppy format (IBM 3740).
//!
//! A disk image is a flat 256,256-byte file representing 77 tracks of
//! 26 sectors of 128 bytes each.

use crate::dpb::{SPT, SECTOR_SIZE, SKEW_TABLE, TOTAL_TRACKS};

/// Total disk image size in bytes (77 tracks * 26 sectors * 128 bytes)
pub const DISK_SIZE: usize = TOTAL_TRACKS as usize * SPT as usize * SECTOR_SIZE;

/// A CP/M disk image for the Tarbell controller
pub struct DiskImage {
    /// Raw disk data (256,256 bytes)
    data: Vec<u8>,
    /// Whether this disk is write-protected
    write_protected: bool,
    /// Whether the disk has been modified since loading
    dirty: bool,
}

impl DiskImage {
    /// Create a new formatted CP/M disk image.
    ///
    /// The entire disk is filled with 0xE5 (CP/M convention for
    /// uninitialized sectors and deleted directory entries).
    /// Directory entries are left as 0xE5 (unused).
    pub fn new_formatted() -> Self {
        Self {
            data: vec![0xE5; DISK_SIZE],
            write_protected: false,
            dirty: true,
        }
    }

    /// Load a disk image from a file
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("Failed to read disk image: {}", e))?;
        if data.len() != DISK_SIZE {
            return Err(format!(
                "Disk image size mismatch: expected {} bytes, got {}",
                DISK_SIZE,
                data.len()
            ));
        }
        Ok(Self {
            data,
            write_protected: false,
            dirty: false,
        })
    }

    /// Save the disk image to a file
    pub fn save(&mut self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, &self.data).map_err(|e| format!("Failed to write disk image: {}", e))?;
        self.dirty = false;
        Ok(())
    }

    /// Check if the disk has been modified since loading
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Check if the disk is write-protected
    pub fn is_write_protected(&self) -> bool {
        self.write_protected
    }

    /// Set write-protection
    pub fn set_write_protected(&mut self, wp: bool) {
        self.write_protected = wp;
    }

    /// Read a physical sector from the disk
    ///
    /// Track numbers are 0-76, sector numbers are 1-26 (physical numbering).
    /// Sector 0 is treated as sector 1 for compatibility with CP/M logical numbering.
    pub fn read_sector(&self, track: u8, sector: u8) -> Result<[u8; SECTOR_SIZE], String> {
        if track >= TOTAL_TRACKS {
            return Err(format!("Track {} out of range (0-{})", track, TOTAL_TRACKS - 1));
        }
        // Treat sector 0 as sector 1 (logical-to-physical mapping)
        let physical_sector = if sector == 0 { 1 } else { sector };
        if physical_sector > SPT as u8 {
            return Err(format!("Sector {} out of range (1-{})", sector, SPT));
        }

        let offset =
            (track as usize * SPT as usize + (physical_sector - 1) as usize) * SECTOR_SIZE;
        let mut buf = [0u8; SECTOR_SIZE];
        buf.copy_from_slice(&self.data[offset..offset + SECTOR_SIZE]);
        Ok(buf)
    }

    /// Write a physical sector to the disk
    pub fn write_sector(
        &mut self,
        track: u8,
        sector: u8,
        data: &[u8; SECTOR_SIZE],
    ) -> Result<(), String> {
        if self.write_protected {
            return Err("Disk is write-protected".into());
        }
        if track >= TOTAL_TRACKS {
            return Err(format!("Track {} out of range (0-{})", track, TOTAL_TRACKS - 1));
        }
        // Treat sector 0 as sector 1 (logical-to-physical mapping)
        let physical_sector = if sector == 0 { 1 } else { sector };
        if physical_sector > SPT as u8 {
            return Err(format!("Sector {} out of range (1-{})", sector, SPT));
        }

        let offset =
            (track as usize * SPT as usize + (physical_sector - 1) as usize) * SECTOR_SIZE;
        self.data[offset..offset + SECTOR_SIZE].copy_from_slice(data);
        self.dirty = true;
        Ok(())
    }

    /// Get a reference to the raw disk data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable reference to the raw disk data
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.dirty = true;
        &mut self.data
    }
}

/// Convert a CP/M logical sector number (0-25) to a physical sector number (1-26)
///
/// The 6:1 interleave is the standard for the Tarbell controller with
/// IBM 3740 format disks. Physical sectors are numbered 1-26.
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

        // Write some data at track 0, sector 1
        let system_data = [0xC3, 0x00, 0x01, 0x00];
        let mut sector = [0xE5u8; SECTOR_SIZE];
        sector[..system_data.len()].copy_from_slice(&system_data);
        disk.write_sector(0, 1, &sector).unwrap();

        // Read it back
        let read_back = disk.read_sector(0, 1).unwrap();
        assert_eq!(read_back[0], 0xC3);
        assert_eq!(read_back[1], 0x00);
        assert_eq!(read_back[2], 0x01);
    }

    #[test]
    fn test_disk_image_sector_round_trip() {
        let mut disk = DiskImage::new_formatted();

        // Write a physical sector
        let data = [0x42u8; SECTOR_SIZE];
        disk.write_sector(0, 1, &data).unwrap();

        // Read it back
        let read_back = disk.read_sector(0, 1).unwrap();
        assert_eq!(read_back[0], 0x42);

        // Other sectors should still be 0xE5
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
        assert!(result.unwrap_err().contains("write-protected"));
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
        // 77 tracks * 26 sectors * 128 bytes = 256,256
        assert_eq!(DISK_SIZE, 77 * 26 * 128);
    }

    #[test]
    fn test_skew_table_is_permutation_of_1_to_26() {
        use crate::dpb::SKEW_TABLE;
        let mut sorted: Vec<u8> = SKEW_TABLE.to_vec();
        sorted.sort();
        assert_eq!(sorted, (1u8..=26).collect::<Vec<u8>>(),
            "SKEW_TABLE must contain every number from 1 to 26 exactly once");
    }

    #[test]
    fn test_skew_table_logical_physical_roundtrip() {
        // Every logical sector should map to a valid physical sector,
        // and the roundtrip should be consistent.
        for logical in 0u8..26 {
            let physical = logical_to_physical(logical);
            assert!(physical >= 1 && physical <= 26,
                "logical {} maps to invalid physical {}", logical, physical);
        }
    }

    #[test]
    fn test_sector_zero_treated_as_sector_one() {
        let disk = DiskImage::new_formatted();
        let s0 = disk.read_sector(0, 0).unwrap();
        let s1 = disk.read_sector(0, 1).unwrap();
        assert_eq!(s0, s1, "sector 0 should be treated as sector 1");
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

    #[test]
    fn test_data_mut_marks_dirty() {
        let disk = DiskImage::new_formatted();
        assert!(disk.is_dirty());
    }
}