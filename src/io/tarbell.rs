//! Tarbell floppy disk controller for the IMSAI 8080 emulator
//!
//! Implements the Tarbell 1011/1011B floppy disk controller, which was the
//! most common disk interface for S-100 bus machines. It was based on the
//! Western Digital FD1771B floppy disk formatter/controller chip.
//!
//! The Tarbell controller occupies I/O ports 0x48-0x4F (configurable, but
//! 0x48 is the standard default). Only four ports are actually used:
//!
//! | Port | Read              | Write             |
//! |------|-------------------|-------------------|
//! | 0x48 | Status register   | Command register  |
//! | 0x49 | Track register    | Track register    |
//! | 0x4A | Sector register   | Sector register   |
//! | 0x4B | Data register     | Data register     |
//!
//! The controller supports 8-inch single-density floppy disks with the
//! standard IBM 3740 format: 77 tracks, 26 sectors per track, 128 bytes
//! per sector, for a total of 243,712 bytes per disk.
//!
//! CP/M 2.2 for the Tarbell controller uses the following disk layout:
//! - Track 0: reserved (boot + CP/M system)
//! - Track 1-2: CP/M system tracks
//! - Track 2-76: data area (directory + files)
//! - Single 128-byte logical sector numbering (no skew translation
//!   at the BIOS level for CP/M; physical skew handled by controller)

#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// I/O port base address for the Tarbell controller (standard default)
const TARBELL_PORT_BASE: u8 = 0x48;

/// Number of I/O ports the Tarbell controller occupies
const TARBELL_PORT_COUNT: usize = 4;

/// Command register port offset
const CMD_STATUS: u8 = 0;
/// Track register port offset
const TRACK: u8 = 1;
/// Sector register port offset
const SECTOR: u8 = 2;
/// Data register port offset
const DATA: u8 = 3;

/// FD1771 command bits
const CMD_RESTORE: u8 = 0x00;
const CMD_SEEK: u8 = 0x10;
const CMD_STEP: u8 = 0x20;
const CMD_STEP_IN: u8 = 0x40;
const CMD_STEP_OUT: u8 = 0x60;
const CMD_READ_SECTOR: u8 = 0x80;
const CMD_WRITE_SECTOR: u8 = 0xA0;
const CMD_READ_ADDRESS: u8 = 0xC0;
const CMD_READ_TRACK: u8 = 0xE0;
const CMD_WRITE_TRACK: u8 = 0xF0;
const CMD_FORCE_INTERRUPT: u8 = 0xD0;

/// Command type mask (upper 4 bits)
const CMD_TYPE_MASK: u8 = 0xF0;
/// Update flag (bit 0 in step-type commands)
const CMD_UPDATE_FLAG: u8 = 0x01;

/// Status register bits
const STATUS_NOT_READY: u8 = 0x80;
const STATUS_WRITE_PROTECT: u8 = 0x40;
const STATUS_RECORD_TYPE: u8 = 0x20;
const STATUS_SECTOR_ZERO: u8 = 0x02;
const STATUS_BUSY: u8 = 0x01;
const STATUS_INDEX: u8 = 0x02;

/// Standard 8-inch floppy geometry (IBM 3740 format)
const TRACKS_PER_DISK: u8 = 77;
const SECTORS_PER_TRACK: u8 = 26;
const BYTES_PER_SECTOR: usize = 128;
const BYTES_PER_DISK: usize = TRACKS_PER_DISK as usize
    * SECTORS_PER_TRACK as usize
    * BYTES_PER_SECTOR;

/// Physical sector numbers on an IBM 3740 disk run from 1 to 26,
/// but CP/M addresses them as logical sectors 0-25. The standard
/// physical-to-logical mapping for the Tarbell controller uses
/// a 6:1 interleave (physical sector 1 = logical 0, physical 7 = logical 1, etc.)
const SECTOR_SKEW_TABLE: [u8; SECTORS_PER_TRACK as usize] = [
    1, 7, 13, 19, 25, 5, 11, 17, 23, 3, 9, 15, 21, 2, 8, 14, 20, 26, 6, 12, 18, 24, 4, 10, 16,
    22,
];

/// A single floppy disk image
struct FloppyDisk {
    /// Raw disk data (243,712 bytes for a full 77-track image)
    data: Vec<u8>,
    /// Whether this disk is write-protected
    write_protected: bool,
}

impl FloppyDisk {
    /// Create a blank (zeroed) disk image
    fn new_blank() -> Self {
        Self {
            data: vec![0xE5; BYTES_PER_DISK],
            write_protected: false,
        }
    }

    /// Load a disk image from a file
    fn load(path: &Path) -> Result<Self, String> {
        let data = fs::read(path).map_err(|e| format!("Failed to read disk image: {}", e))?;
        if data.len() != BYTES_PER_DISK {
            return Err(format!(
                "Disk image size mismatch: expected {} bytes, got {}",
                BYTES_PER_DISK,
                data.len()
            ));
        }
        Ok(Self {
            data,
            write_protected: false,
        })
    }

    /// Read a sector from the disk
    fn read_sector(&self, track: u8, sector: u8) -> Result<[u8; BYTES_PER_SECTOR], String> {
        if track >= TRACKS_PER_DISK {
            return Err(format!("Track {} out of range (0-{})", track, TRACKS_PER_DISK - 1));
        }
        if sector == 0 || sector > SECTORS_PER_TRACK {
            return Err(format!(
                "Sector {} out of range (1-{})",
                sector,
                SECTORS_PER_TRACK
            ));
        }

        let offset = (track as usize * SECTORS_PER_TRACK as usize + (sector - 1) as usize)
            * BYTES_PER_SECTOR;
        let mut buf = [0u8; BYTES_PER_SECTOR];
        buf.copy_from_slice(&self.data[offset..offset + BYTES_PER_SECTOR]);
        Ok(buf)
    }

    /// Write a sector to the disk
    fn write_sector(
        &mut self,
        track: u8,
        sector: u8,
        data: &[u8; BYTES_PER_SECTOR],
    ) -> Result<(), String> {
        if self.write_protected {
            return Err("Disk is write-protected".into());
        }
        if track >= TRACKS_PER_DISK {
            return Err(format!("Track {} out of range (0-{})", track, TRACKS_PER_DISK - 1));
        }
        if sector == 0 || sector > SECTORS_PER_TRACK {
            return Err(format!(
                "Sector {} out of range (1-{})",
                sector,
                SECTORS_PER_TRACK
            ));
        }

        let offset = (track as usize * SECTORS_PER_TRACK as usize + (sector - 1) as usize)
            * BYTES_PER_SECTOR;
        self.data[offset..offset + BYTES_PER_SECTOR].copy_from_slice(data);
        Ok(())
    }

    /// Read a sector using CP/M logical sector numbering (0-25)
    fn read_logical_sector(
        &self,
        track: u8,
        logical_sector: u8,
    ) -> Result<[u8; BYTES_PER_SECTOR], String> {
        // Convert logical sector to physical sector using skew table
        let physical = logical_to_physical(logical_sector);
        self.read_sector(track, physical)
    }

    /// Write a sector using CP/M logical sector numbering (0-25)
    fn write_logical_sector(
        &mut self,
        track: u8,
        logical_sector: u8,
        data: &[u8; BYTES_PER_SECTOR],
    ) -> Result<(), String> {
        let physical = logical_to_physical(logical_sector);
        self.write_sector(track, physical, data)
    }
}

/// Convert a CP/M logical sector number (0-25) to a physical sector number (1-26)
fn logical_to_physical(logical: u8) -> u8 {
    if (logical as usize) < SECTOR_SKEW_TABLE.len() {
        SECTOR_SKEW_TABLE[logical as usize]
    } else {
        // Should not happen with valid CP/M calls
        logical + 1
    }
}

/// Convert a physical sector number (1-26) to a CP/M logical sector number (0-25)
fn physical_to_logical(physical: u8) -> u8 {
    for (logical, &phys) in SECTOR_SKEW_TABLE.iter().enumerate() {
        if phys == physical {
            return logical as u8;
        }
    }
    // Should not happen
    physical.saturating_sub(1)
}

/// The Tarbell floppy disk controller
pub struct TarbellController {
    /// Floppy disk drives (up to 4 drives, A-D)
    drives: [Option<FloppyDisk>; 4],
    /// Currently selected drive (0-3)
    current_drive: u8,
    /// Track register (current track position)
    track_register: u8,
    /// Sector register (current sector)
    sector_register: u8,
    /// Data register
    data_register: u8,
    /// Status register
    status: u8,
    /// Sector data buffer for read/write operations
    sector_buffer: [u8; BYTES_PER_SECTOR],
    /// Current byte position within the sector buffer during read/write
    buffer_position: usize,
    /// Whether a read operation is in progress
    reading: bool,
    /// Whether a write operation is in progress
    writing: bool,
    /// Whether the last command had an error
    error: bool,
}

impl Default for TarbellController {
    fn default() -> Self {
        Self::new()
    }
}

impl TarbellController {
    /// Create a new Tarbell controller with no disks inserted
    pub fn new() -> Self {
        Self {
            drives: [None, None, None, None],
            current_drive: 0,
            track_register: 0,
            sector_register: 1,
            data_register: 0,
            status: STATUS_NOT_READY, // No disk = not ready
            sector_buffer: [0; BYTES_PER_SECTOR],
            buffer_position: 0,
            reading: false,
            writing: false,
            error: false,
        }
    }

    /// Insert a disk image file into a drive (0-3)
    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        if drive > 3 {
            return Err("Drive number must be 0-3".into());
        }
        let disk = FloppyDisk::load(Path::new(path))?;
        self.drives[drive] = Some(disk);
        // If this is the current drive, update status
        if drive == self.current_drive as usize {
            self.status &= !STATUS_NOT_READY;
        }
        Ok(())
    }

    /// Insert a blank disk into a drive (0-3)
    pub fn insert_blank_disk(&mut self, drive: usize) -> Result<(), String> {
        if drive > 3 {
            return Err("Drive number must be 0-3".into());
        }
        let disk = FloppyDisk::new_blank();
        self.drives[drive] = Some(disk);
        if drive == self.current_drive as usize {
            self.status &= !STATUS_NOT_READY;
        }
        Ok(())
    }

    /// Get the base I/O port address
    pub fn port_base(&self) -> u8 {
        TARBELL_PORT_BASE
    }

    /// Check if this controller handles the given I/O port
    pub fn handles_port(&self, port: u8) -> bool {
        port >= TARBELL_PORT_BASE
            && port < TARBELL_PORT_BASE + TARBELL_PORT_COUNT as u8
    }

    /// Read from an I/O port
    pub fn io_in(&mut self, port: u8) -> u8 {
        let offset = port - TARBELL_PORT_BASE;
        match offset {
            0 => {
                // Status register
                self.update_status();
                self.status
            }
            1 => self.track_register,
            2 => self.sector_register,
            3 => {
                // Data register: if we're in a read operation, return next byte
                if self.reading && self.buffer_position < BYTES_PER_SECTOR {
                    let value = self.sector_buffer[self.buffer_position];
                    self.buffer_position += 1;

                    if self.buffer_position >= BYTES_PER_SECTOR {
                        // Sector read complete
                        self.reading = false;
                        self.status &= !STATUS_BUSY;
                    }
                    value
                } else {
                    self.data_register
                }
            }
            _ => 0xFF,
        }
    }

    /// Write to an I/O port
    pub fn io_out(&mut self, port: u8, value: u8) {
        let offset = port - TARBELL_PORT_BASE;
        match offset {
            0 => {
                // Command register
                self.execute_command(value);
            }
            1 => {
                // Track register
                self.track_register = value;
            }
            2 => {
                // Sector register
                self.sector_register = value;
            }
            3 => {
                // Data register
                if self.writing && self.buffer_position < BYTES_PER_SECTOR {
                    // Writing sector data byte by byte
                    self.sector_buffer[self.buffer_position] = value;
                    self.buffer_position += 1;

                    if self.buffer_position >= BYTES_PER_SECTOR {
                        // Sector write complete
                        self.finish_write_sector();
                    }
                } else {
                    self.data_register = value;
                }
            }
            _ => {}
        }
    }

    /// Update the status register based on current drive state
    fn update_status(&mut self) {
        let drive = self.current_drive as usize;
        match &self.drives[drive] {
            Some(_disk) => {
                self.status &= !STATUS_NOT_READY;
                // Write-protected status
                if self.drives[drive].as_ref().is_some_and(|d| d.write_protected) {
                    self.status |= STATUS_WRITE_PROTECT;
                } else {
                    self.status &= !STATUS_WRITE_PROTECT;
                }
            }
            None => {
                self.status |= STATUS_NOT_READY;
            }
        }
    }

    /// Get a mutable reference to the current drive's disk
    fn current_disk(&mut self) -> Option<&mut FloppyDisk> {
        self.drives[self.current_drive as usize].as_mut()
    }

    /// Get an immutable reference to the current drive's disk
    fn current_disk_ref(&self) -> Option<&FloppyDisk> {
        self.drives[self.current_drive as usize].as_ref()
    }

    /// Execute an FD1771 command
    fn execute_command(&mut self, command: u8) {
        let cmd_type = command & CMD_TYPE_MASK;

        match cmd_type {
            CMD_RESTORE => {
                // Restore (seek to track 0)
                self.track_register = 0;
                self.status |= STATUS_BUSY;
                // Immediately complete (emulated hardware is instant)
                self.status &= !STATUS_BUSY;
                self.error = false;
            }
            CMD_SEEK => {
                // Seek to track in data register
                let target_track = self.data_register;
                if target_track < TRACKS_PER_DISK {
                    self.track_register = target_track;
                    self.status &= !STATUS_BUSY;
                    self.error = false;
                } else {
                    self.error = true;
                    self.status &= !STATUS_BUSY;
                }
            }
            CMD_STEP | CMD_STEP_IN | CMD_STEP_OUT => {
                // Step in/out one track
                if cmd_type == CMD_STEP_IN {
                    if self.track_register < TRACKS_PER_DISK - 1 {
                        self.track_register += 1;
                    }
                } else if cmd_type == CMD_STEP_OUT && self.track_register > 0 {
                    self.track_register -= 1;
                }
                // Step (with update flag) just re-steps
                self.status &= !STATUS_BUSY;
            }
            CMD_READ_SECTOR => {
                self.read_sector();
            }
            CMD_WRITE_SECTOR => {
                self.begin_write_sector();
            }
            CMD_READ_ADDRESS => {
                // Read address mark: returns track number in data register
                self.data_register = self.track_register;
                self.status &= !STATUS_BUSY;
            }
            CMD_READ_TRACK | CMD_WRITE_TRACK => {
                // Full track read/write not needed for CP/M operation
                // Mark complete immediately
                self.status &= !STATUS_BUSY;
            }
            CMD_FORCE_INTERRUPT => {
                // Cancel any ongoing operation
                self.reading = false;
                self.writing = false;
                self.status &= !STATUS_BUSY;
            }
            _ => {
                // Unknown command, ignore
            }
        }
    }

    /// Read the sector at the current track/sector into the buffer
    fn read_sector(&mut self) {
        self.status |= STATUS_BUSY;

        match self.current_disk_ref() {
            Some(disk) => {
                match disk.read_sector(self.track_register, self.sector_register) {
                    Ok(sector_data) => {
                        self.sector_buffer = sector_data;
                        self.buffer_position = 0;
                        self.reading = true;
                        self.error = false;
                        self.status &= !STATUS_BUSY;
                    }
                    Err(_) => {
                        self.error = true;
                        self.status &= !STATUS_BUSY;
                    }
                }
            }
            None => {
                self.error = true;
                self.status &= !STATUS_BUSY;
            }
        }
    }

    /// Begin a write sector operation: prepare the buffer
    fn begin_write_sector(&mut self) {
        self.status |= STATUS_BUSY;
        self.sector_buffer = [0; BYTES_PER_SECTOR];
        self.buffer_position = 0;
        self.writing = true;

        // Check if drive is present and writable
        if self.current_disk_ref().is_none() {
            self.error = true;
            self.writing = false;
            self.status &= !STATUS_BUSY;
            return;
        }

        if self
            .current_disk_ref()
            .is_some_and(|d| d.write_protected)
        {
            self.status |= STATUS_WRITE_PROTECT;
            self.error = true;
            self.writing = false;
            self.status &= !STATUS_BUSY;
        }
    }

    /// Complete a write sector operation
    fn finish_write_sector(&mut self) {
        self.writing = false;

        let track = self.track_register;
        let sector = self.sector_register;
        let data = self.sector_buffer;

        match self.current_disk() {
            Some(disk) => {
                if let Err(e) = disk.write_sector(track, sector, &data) {
                    eprintln!("Tarbell write error: {}", e);
                    self.error = true;
                } else {
                    self.error = false;
                }
            }
            None => {
                self.error = true;
            }
        }
        self.status &= !STATUS_BUSY;
    }

    /// Read a sector using CP/M logical sector addressing
    /// This is the primary interface for CP/M BIOS calls
    pub fn read_logical_sector(
        &mut self,
        drive: u8,
        track: u8,
        logical_sector: u8,
    ) -> Result<[u8; BYTES_PER_SECTOR], String> {
        if drive > 3 || self.drives[drive as usize].is_none() {
            return Err(format!("Drive {} not ready", drive));
        }
        self.current_drive = drive;
        self.track_register = track;
        let physical = logical_to_physical(logical_sector);
        self.sector_register = physical;

        let result = self.drives[drive as usize]
            .as_ref()
            .unwrap()
            .read_logical_sector(track, logical_sector)?;

        Ok(result)
    }

    /// Write a sector using CP/M logical sector addressing
    /// This is the primary interface for CP/M BIOS calls
    pub fn write_logical_sector(
        &mut self,
        drive: u8,
        track: u8,
        logical_sector: u8,
        data: &[u8; BYTES_PER_SECTOR],
    ) -> Result<(), String> {
        if drive > 3 || self.drives[drive as usize].is_none() {
            return Err(format!("Drive {} not ready", drive));
        }

        self.drives[drive as usize]
            .as_mut()
            .unwrap()
            .write_logical_sector(track, logical_sector, data)
    }

    /// Get the number of tracks per disk
    pub fn tracks_per_disk(&self) -> u8 {
        TRACKS_PER_DISK
    }

    /// Get the number of sectors per track
    pub fn sectors_per_track(&self) -> u8 {
        SECTORS_PER_TRACK
    }

    /// Get the number of bytes per sector
    pub fn bytes_per_sector(&self) -> usize {
        BYTES_PER_SECTOR
    }

    /// Check if a drive has a disk inserted
    pub fn has_disk(&self, drive: usize) -> bool {
        drive < 4 && self.drives[drive].is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tarbell_controller_creation() {
        let controller = TarbellController::new();
        assert_eq!(controller.port_base(), 0x48);
        assert_eq!(controller.tracks_per_disk(), 77);
        assert_eq!(controller.sectors_per_track(), 26);
        assert_eq!(controller.bytes_per_sector(), 128);
        assert!(!controller.has_disk(0));
    }

    #[test]
    fn test_blank_disk_creation() {
        let disk = FloppyDisk::new_blank();
        assert_eq!(disk.data.len(), BYTES_PER_DISK);
        // Blank disks are filled with 0xE5 (CP/M format convention)
        assert_eq!(disk.data[0], 0xE5);
    }

    #[test]
    fn test_insert_and_read_blank_disk() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();
        assert!(controller.has_disk(0));

        // Reading from a blank disk should return 0xE5 bytes
        let sector = controller.read_logical_sector(0, 0, 0).unwrap();
        assert_eq!(sector[0], 0xE5);
    }

    #[test]
    fn test_write_and_read_sector() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();

        let data = [0x42u8; BYTES_PER_SECTOR];
        controller.write_logical_sector(0, 0, 0, &data).unwrap();

        let read_back = controller.read_logical_sector(0, 0, 0).unwrap();
        assert_eq!(read_back[0], 0x42);
        // Only first sector should be written, rest still 0xE5
        let other_sector = controller.read_logical_sector(0, 0, 1).unwrap();
        assert_eq!(other_sector[0], 0xE5);
    }

    #[test]
    fn test_logical_to_physical_sector_mapping() {
        // Logical 0 -> Physical 1
        assert_eq!(logical_to_physical(0), 1);
        // Logical 1 -> Physical 7
        assert_eq!(logical_to_physical(1), 7);
        // Verify round-trip
        for logical in 0..26u8 {
            let physical = logical_to_physical(logical);
            let back = physical_to_logical(physical);
            assert_eq!(back, logical, "Round-trip failed for logical sector {}", logical);
        }
    }

    #[test]
    fn test_sector_skew_table_coverage() {
        // Every physical sector 1-26 should appear exactly once
        let mut seen = [false; 27]; // index 0 unused, 1-26 used
        for &phys in &SECTOR_SKEW_TABLE {
            assert!(!seen[phys as usize], "Duplicate physical sector {}", phys);
            seen[phys as usize] = true;
        }
        for i in 1..=26 {
            assert!(seen[i], "Missing physical sector {}", i);
        }
    }

    #[test]
    fn test_tarbell_io_port_range() {
        let controller = TarbellController::new();
        assert!(controller.handles_port(0x48));
        assert!(controller.handles_port(0x49));
        assert!(controller.handles_port(0x4A));
        assert!(controller.handles_port(0x4B));
        assert!(!controller.handles_port(0x47));
        assert!(!controller.handles_port(0x4C));
    }

    #[test]
    fn test_tarbell_restore_command() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();
        controller.track_register = 5;
        controller.io_out(0x48, CMD_RESTORE);
        assert_eq!(controller.track_register, 0);
    }

    #[test]
    fn test_tarbell_seek_command() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();
        controller.data_register = 10;
        controller.io_out(0x48, CMD_SEEK);
        assert_eq!(controller.track_register, 10);
    }

    #[test]
    fn test_multiple_drives() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();
        controller.insert_blank_disk(1).unwrap();

        let data_a = [0xAAu8; BYTES_PER_SECTOR];
        let data_b = [0xBBu8; BYTES_PER_SECTOR];

        controller.write_logical_sector(0, 0, 0, &data_a).unwrap();
        controller.write_logical_sector(1, 0, 0, &data_b).unwrap();

        let read_a = controller.read_logical_sector(0, 0, 0).unwrap();
        let read_b = controller.read_logical_sector(1, 0, 0).unwrap();
        assert_eq!(read_a[0], 0xAA);
        assert_eq!(read_b[0], 0xBB);
    }

    #[test]
    fn test_sector_addressing_bounds() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();

        // Track 0, sector 0 logical should work (maps to physical 1)
        assert!(controller.read_logical_sector(0, 0, 0).is_ok());

        // Track 76 (last track) should work
        assert!(controller.read_logical_sector(0, 76, 0).is_ok());

        // Track 77 should fail (out of range, 0-indexed 0-76)
        assert!(controller.read_logical_sector(0, 77, 0).is_err());

        // Drive without disk should fail
        assert!(controller.read_logical_sector(2, 0, 0).is_err());
    }
}