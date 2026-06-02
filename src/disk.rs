//! CP/M disk image creation and management
//!
//! Handles creating, formatting, and manipulating CP/M 2.2 disk images
//! for the Tarbell single-density 8-inch floppy format (IBM 3740).
//!
//! A disk image is a flat 256,256-byte file representing 77 tracks of
//! 26 sectors of 128 bytes each. The CP/M file system structure is:
//!
//! | Tracks     | Contents                                    |
//! |------------|---------------------------------------------|
//! | 0-1        | System tracks (boot + CCP + BDOS + BIOS)  |
//! | 2-76       | Data area (directory + file data)          |
//!
//! The directory occupies the first two allocation blocks (2KB) of the
//! data area, giving 64 directory entries.

use crate::dpb::{BLM, DRM, OFF, SPT, SECTOR_SIZE, SKEW_TABLE, TOTAL_TRACKS};

/// Total disk image size in bytes (77 tracks * 26 sectors * 128 bytes)
pub const DISK_SIZE: usize = TOTAL_TRACKS as usize * SPT as usize * SECTOR_SIZE;

/// CP/M directory entry size in bytes
const DIR_ENTRY_SIZE: usize = 32;

/// Number of directory entries (DRM + 1)
const NUM_DIR_ENTRIES: usize = DRM as usize + 1;

/// Directory allocation blocks (from AL0/AL1)
/// AL0 = 0xC0 means blocks 0 and 1 are reserved for directory
/// That gives us 2 * 1024 = 2048 bytes for directory = 64 entries of 32 bytes
#[allow(dead_code)]
const DIR_BLOCKS: usize = 2;

/// Directory size in bytes
#[allow(dead_code)]
const DIR_SIZE: usize = DIR_BLOCKS * SECTOR_SIZE * (BLM as usize + 1);

/// Byte offset where the data area begins (after reserved tracks)
const DATA_OFFSET: usize = OFF as usize * SPT as usize * SECTOR_SIZE;

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
    /// Create a new blank (unformatted) disk image
    ///
    /// The entire disk is filled with 0xE5, which is CP/M's convention
    /// for uninitialized/unused sectors (also the value for deleted
    /// directory entries).
    pub fn new_blank() -> Self {
        Self {
            data: vec![0xE5; DISK_SIZE],
            write_protected: false,
            dirty: true,
        }
    }

    /// Create a new formatted CP/M disk image
    ///
    /// Initializes the directory area and writes an empty directory.
    /// Does NOT write system tracks (use `write_system()` for that).
    pub fn new_formatted() -> Self {
        let mut disk = Self::new_blank();
        disk.format_directory();
        disk
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

    /// Format the directory area on the disk
    ///
    /// Writes the CP/M directory structure into the data area.
    /// Directory entries are 32 bytes each; unused entries are
    /// filled with 0xE5. Allocation blocks 0 and 1 are marked
    /// as reserved for the directory in AL0/AL1.
    fn format_directory(&mut self) {
        // The directory starts at the beginning of the data area
        // (track OFF, sector 0 in logical numbering)
        let dir_start = DATA_OFFSET;

        // Fill directory entries with 0xE5 (unused)
        for i in 0..NUM_DIR_ENTRIES {
            let entry_offset = dir_start + i * DIR_ENTRY_SIZE;
            if entry_offset + DIR_ENTRY_SIZE <= self.data.len() {
                // First byte of an unused entry is 0xE5
                self.data[entry_offset] = 0xE5;
                // Rest of entry remains 0xE5 from new_blank()
            }
        }
    }

    /// Read a physical sector from the disk
    ///
    /// Track numbers are 0-76, sector numbers are 1-26 (physical numbering).
    /// Some CP/M implementations pass sector 0 (logical), so we treat
    /// sector 0 as sector 1 for compatibility.
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

    /// Read a logical sector (CP/M numbering: track, sector 0-25)
    ///
    /// Uses the 6:1 interleave skew table to convert logical sector
    /// numbers to physical sector numbers.
    pub fn read_logical_sector(
        &self,
        track: u8,
        logical_sector: u8,
    ) -> Result<[u8; SECTOR_SIZE], String> {
        let physical = logical_to_physical(logical_sector);
        self.read_sector(track, physical)
    }

    /// Write a logical sector (CP/M numbering)
    pub fn write_logical_sector(
        &mut self,
        track: u8,
        logical_sector: u8,
        data: &[u8; SECTOR_SIZE],
    ) -> Result<(), String> {
        let physical = logical_to_physical(logical_sector);
        self.write_sector(track, physical, data)
    }

    /// Write system data onto the reserved tracks
    ///
    /// This writes the CP/M system image (CCP + BDOS + BIOS) starting
    /// at track 0, sector 1. The data is written sequentially across
    /// the reserved tracks.
    pub fn write_system(&mut self, system_data: &[u8]) -> Result<(), String> {
        if self.write_protected {
            return Err("Disk is write-protected".into());
        }

        let reserved_bytes = OFF as usize * SPT as usize * SECTOR_SIZE;
        if system_data.len() > reserved_bytes {
            return Err(format!(
                "System data too large: {} bytes exceeds reserved area of {} bytes",
                system_data.len(),
                reserved_bytes
            ));
        }

        // Write system data starting at track 0, physical sector 1
        for (i, &byte) in system_data.iter().enumerate() {
            let track = (i / (SPT as usize * SECTOR_SIZE)) as u8;
            let offset_in_track = i % (SPT as usize * SECTOR_SIZE);
            let sector = (offset_in_track / SECTOR_SIZE) as u8 + 1;
            let byte_in_sector = offset_in_track % SECTOR_SIZE;

            let sector_offset = (track as usize * SPT as usize + (sector - 1) as usize)
                * SECTOR_SIZE
                + byte_in_sector;
            self.data[sector_offset] = byte;
        }
        self.dirty = true;
        Ok(())
    }

    /// Read system data from the reserved tracks
    ///
    /// Returns the raw bytes from tracks 0 through (OFF-1).
    pub fn read_system(&self) -> Vec<u8> {
        let reserved_bytes = OFF as usize * SPT as usize * SECTOR_SIZE;
        self.data[..reserved_bytes].to_vec()
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

    /// Calculate the allocation block number from a byte offset in the data area
    #[allow(dead_code)]
    fn byte_to_alloc_block(offset: usize) -> u16 {
        (offset / SECTOR_SIZE) as u16
    }

    /// Check if a directory entry is in use
    pub fn is_dir_entry_used(&self, index: usize) -> bool {
        if index >= NUM_DIR_ENTRIES {
            return false;
        }
        let dir_start = DATA_OFFSET + index * DIR_ENTRY_SIZE;
        self.data[dir_start] != 0xE5
    }

    /// Get the number of used directory entries
    pub fn used_dir_entries(&self) -> usize {
        (0..NUM_DIR_ENTRIES)
            .filter(|&i| self.is_dir_entry_used(i))
            .count()
    }

    /// Create a bootable CP/M disk by writing a system image onto the
    /// reserved tracks and formatting the directory.
    ///
    /// `system_data` is the raw CP/M system (CCP + BDOS + BIOS) that
    /// normally occupies the first 2 tracks.
    pub fn create_bootable(system_data: &[u8]) -> Result<Self, String> {
        let mut disk = Self::new_formatted();
        disk.write_system(system_data)?;
        Ok(disk)
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

/// Convert a physical sector number (1-26) to a CP/M logical sector number (0-25)
#[allow(dead_code)]
fn physical_to_logical(physical: u8) -> u8 {
    for (logical, &phys) in SKEW_TABLE.iter().enumerate() {
        if phys == physical {
            return logical as u8;
        }
    }
    physical.saturating_sub(1)
}

/// Utility to build a CP/M system image from components
///
/// The CP/M system consists of:
/// - CCP (Command Control Program) starting at 0x0000 (after boot)
/// - BDOS (Basic Disk Operating System) following the CCP
/// - BIOS (Basic Input/Output System) following the BDOS
///
/// In memory, the layout after boot is:
/// - 0x0000: JMP WBOOT
/// - 0x0003: IOBYTE
/// - 0x0005: JMP BDOS
/// - 0x0100: CCP start (where user programs load)
/// - CCP+BDOS+BIOS are loaded from the system tracks of the disk
pub struct SystemBuilder {
    /// The system image data being built
    data: Vec<u8>,
}

impl SystemBuilder {
    /// Create a new system builder
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
        }
    }

    /// Load a system image from raw bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
        }
    }

    /// Load a system image from a file
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("Failed to read system file: {}", e))?;
        Ok(Self { data })
    }

    /// Get the system image data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the total size of the system image
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Check if the data looks like a CP/M system image
    ///
    /// A CP/M system always starts with a JMP instruction (0xC3).
    pub fn looks_like_cpm(&self) -> bool {
        !self.data.is_empty() && self.data[0] == 0xC3
    }
}

impl Default for SystemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_image_blank() {
        let disk = DiskImage::new_blank();
        assert_eq!(disk.data().len(), DISK_SIZE);
        // All bytes should be 0xE5 (CP/M uninitialized)
        assert_eq!(disk.data()[0], 0xE5);
        assert_eq!(disk.data()[DISK_SIZE - 1], 0xE5);
    }

    #[test]
    fn test_disk_image_format_directory() {
        let disk = DiskImage::new_formatted();
        // Directory entries should start as unused (0xE5)
        assert_eq!(disk.data()[DATA_OFFSET], 0xE5);
    }

    #[test]
    fn test_disk_image_write_read_system() {
        let mut disk = DiskImage::new_formatted();

        // Write a system image (just some test data)
        let system_data = vec![0xC3, 0x00, 0x01, 0x00]; // JMP 0x0100
        disk.write_system(&system_data).unwrap();

        // Read it back
        let sys = disk.read_system();
        assert_eq!(sys[0], 0xC3);
        assert_eq!(sys[1], 0x00);
        assert_eq!(sys[2], 0x01);
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
    fn test_disk_image_logical_sector() {
        let mut disk = DiskImage::new_formatted();

        // Write to logical sector 0, track 2 (data area)
        let data = [0xAAu8; SECTOR_SIZE];
        disk.write_logical_sector(2, 0, &data).unwrap();

        // Read it back via logical sector
        let read_back = disk.read_logical_sector(2, 0).unwrap();
        assert_eq!(read_back[0], 0xAA);
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

        // After creating formatted, dirty should be true
        // Save would clear it, but we don't have a temp file here
        // Just verify the flag state
        let data = [0x42u8; SECTOR_SIZE];
        disk.write_sector(0, 1, &data).unwrap();
        assert!(disk.is_dirty());
    }

    #[test]
    fn test_disk_image_system_too_large() {
        let mut disk = DiskImage::new_formatted();
        let reserved_bytes = OFF as usize * SPT as usize * SECTOR_SIZE;

        // Create system data larger than reserved tracks
        let oversized = vec![0xFF; reserved_bytes + 1];
        let result = disk.write_system(&oversized);
        assert!(result.is_err());
    }

    #[test]
    fn test_system_builder_from_bytes() {
        let data = vec![0xC3, 0x00, 0x01]; // JMP 0x0100
        let builder = SystemBuilder::from_bytes(&data);
        assert!(builder.looks_like_cpm());
        assert_eq!(builder.size(), 3);
    }

    #[test]
    fn test_system_builder_not_cpm() {
        let data = vec![0x00, 0x00, 0x00];
        let builder = SystemBuilder::from_bytes(&data);
        assert!(!builder.looks_like_cpm());
    }

    #[test]
    fn test_directory_entry_tracking() {
        let disk = DiskImage::new_formatted();
        // After formatting, all directory entries should be unused
        assert_eq!(disk.used_dir_entries(), 0);
    }

    #[test]
    fn test_disk_size_is_correct() {
        // 77 tracks * 26 sectors * 128 bytes = 256,256
        assert_eq!(
            DISK_SIZE,
            77 * 26 * 128
        );
    }
}