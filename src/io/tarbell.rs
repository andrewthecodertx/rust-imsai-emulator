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

#![allow(dead_code)]

use crate::disk::DiskImage;
use crate::dpb::SECTOR_SIZE;

/// I/O port base address for the Tarbell controller (standard default)
const TARBELL_PORT_BASE: u8 = 0x48;

/// Number of I/O ports the Tarbell controller occupies
const TARBELL_PORT_COUNT: usize = 4;

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

/// Status register bits
const STATUS_NOT_READY: u8 = 0x80;
const STATUS_WRITE_PROTECT: u8 = 0x40;
const STATUS_BUSY: u8 = 0x01;

/// The Tarbell floppy disk controller
pub struct TarbellController {
    /// Floppy disk drives (up to 4 drives, A-D)
    drives: [Option<DiskImage>; 4],
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
    sector_buffer: [u8; SECTOR_SIZE],
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
            status: STATUS_NOT_READY,
            sector_buffer: [0; SECTOR_SIZE],
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
        let disk = DiskImage::load(std::path::Path::new(path))?;
        self.drives[drive] = Some(disk);
        if drive == self.current_drive as usize {
            self.status &= !STATUS_NOT_READY;
        }
        Ok(())
    }

    /// Insert a DiskImage directly into a drive
    pub fn insert_disk_image(&mut self, drive: usize, disk: DiskImage) -> Result<(), String> {
        if drive > 3 {
            return Err("Drive number must be 0-3".into());
        }
        self.drives[drive] = Some(disk);
        if drive == self.current_drive as usize {
            self.status &= !STATUS_NOT_READY;
        }
        Ok(())
    }

    /// Insert a blank formatted disk into a drive (0-3)
    pub fn insert_blank_disk(&mut self, drive: usize) -> Result<(), String> {
        if drive > 3 {
            return Err("Drive number must be 0-3".into());
        }
        let disk = DiskImage::new_formatted();
        self.drives[drive] = Some(disk);
        if drive == self.current_drive as usize {
            self.status &= !STATUS_NOT_READY;
        }
        Ok(())
    }

    /// Eject the disk from a drive
    pub fn eject_disk(&mut self, drive: usize) -> Option<DiskImage> {
        if drive > 3 {
            return None;
        }
        let disk = self.drives[drive].take();
        if drive == self.current_drive as usize {
            self.status |= STATUS_NOT_READY;
        }
        disk
    }

    /// Get a mutable reference to the disk in a drive
    pub fn get_disk_mut(&mut self, drive: usize) -> Option<&mut DiskImage> {
        if drive > 3 {
            return None;
        }
        self.drives[drive].as_mut()
    }

    /// Get a reference to the disk in a drive
    pub fn get_disk(&self, drive: usize) -> Option<&DiskImage> {
        if drive > 3 {
            return None;
        }
        self.drives[drive].as_ref()
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
                if self.reading && self.buffer_position < SECTOR_SIZE {
                    let value = self.sector_buffer[self.buffer_position];
                    self.buffer_position += 1;

                    if self.buffer_position >= SECTOR_SIZE {
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
                if self.writing && self.buffer_position < SECTOR_SIZE {
                    self.sector_buffer[self.buffer_position] = value;
                    self.buffer_position += 1;

                    if self.buffer_position >= SECTOR_SIZE {
                        self.finish_write_sector();
                    }
                } else {
                    self.data_register = value;
                }
            }
            _ => {}
        }
    }

    /// Return the CMI5619 WAIT/DRQ port value.
    /// Bit 7 set = DRQ active (data byte ready to read/write).
    /// Bit 7 clear = transfer complete.
    /// The CMI5619 boot code does: IN WAIT; ORA A; JP CHECK (done if positive).
    pub fn wait_port_value(&self) -> u8 {
        if self.reading && self.buffer_position < SECTOR_SIZE {
            0x80 // DRQ active, data available
        } else if self.writing && self.buffer_position < SECTOR_SIZE {
            0x80 // DRQ active, ready for data
        } else {
            0x00 // Transfer complete
        }
    }

    /// Update the status register based on current drive state
    fn update_status(&mut self) {
        let drive = self.current_drive as usize;
        match &self.drives[drive] {
            Some(_disk) => {
                self.status &= !STATUS_NOT_READY;
                if self.drives[drive].as_ref().is_some_and(|d| d.is_write_protected()) {
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

    /// Execute an FD1771 command
    fn execute_command(&mut self, command: u8) {
        let cmd_type = command & CMD_TYPE_MASK;

        match cmd_type {
            CMD_RESTORE => {
                self.track_register = 0;
                self.status &= !STATUS_BUSY;
                self.error = false;
            }
            CMD_SEEK => {
                let target_track = self.data_register;
                if target_track < 77 {
                    self.track_register = target_track;
                    self.status &= !STATUS_BUSY;
                    self.error = false;
                } else {
                    self.error = true;
                    self.status &= !STATUS_BUSY;
                }
            }
            CMD_STEP | CMD_STEP_IN | CMD_STEP_OUT => {
                if cmd_type == CMD_STEP_IN {
                    if self.track_register < 76 {
                        self.track_register += 1;
                    }
                } else if cmd_type == CMD_STEP_OUT && self.track_register > 0 {
                    self.track_register -= 1;
                }
                self.status &= !STATUS_BUSY;
            }
            CMD_READ_SECTOR => {
                self.read_sector();
            }
            CMD_WRITE_SECTOR => {
                self.begin_write_sector();
            }
            CMD_READ_ADDRESS => {
                self.data_register = self.track_register;
                self.status &= !STATUS_BUSY;
            }
            CMD_READ_TRACK | CMD_WRITE_TRACK => {
                self.status &= !STATUS_BUSY;
            }
            CMD_FORCE_INTERRUPT => {
                self.reading = false;
                self.writing = false;
                self.status &= !STATUS_BUSY;
            }
            _ => {}
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

    /// Begin a write sector operation
    fn begin_write_sector(&mut self) {
        self.status |= STATUS_BUSY;
        self.sector_buffer = [0; SECTOR_SIZE];
        self.buffer_position = 0;
        self.writing = true;

        if self.current_disk_ref().is_none() {
            self.error = true;
            self.writing = false;
            self.status &= !STATUS_BUSY;
            return;
        }

        if self
            .current_disk_ref()
            .is_some_and(|d| d.is_write_protected())
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

        match self.current_disk_mut() {
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
    pub fn read_logical_sector(
        &mut self,
        drive: u8,
        track: u8,
        logical_sector: u8,
    ) -> Result<[u8; SECTOR_SIZE], String> {
        if drive > 3 || self.drives[drive as usize].is_none() {
            return Err(format!("Drive {} not ready", drive));
        }
        let physical = crate::disk::logical_to_physical(logical_sector);
        self.drives[drive as usize]
            .as_ref()
            .unwrap()
            .read_sector(track, physical)
    }

    /// Write a sector using CP/M logical sector addressing
    pub fn write_logical_sector(
        &mut self,
        drive: u8,
        track: u8,
        logical_sector: u8,
        data: &[u8; SECTOR_SIZE],
    ) -> Result<(), String> {
        if drive > 3 || self.drives[drive as usize].is_none() {
            return Err(format!("Drive {} not ready", drive));
        }
        let physical = crate::disk::logical_to_physical(logical_sector);
        self.drives[drive as usize]
            .as_mut()
            .unwrap()
            .write_sector(track, physical, data)
    }

    /// Get a reference to the current drive's disk
    fn current_disk_ref(&self) -> Option<&DiskImage> {
        self.drives[self.current_drive as usize].as_ref()
    }

    /// Get a mutable reference to the current drive's disk
    fn current_disk_mut(&mut self) -> Option<&mut DiskImage> {
        self.drives[self.current_drive as usize].as_mut()
    }

    /// Get the number of tracks per disk
    pub fn tracks_per_disk(&self) -> u8 {
        77
    }

    /// Get the number of sectors per track
    pub fn sectors_per_track(&self) -> u8 {
        26
    }

    /// Get the number of bytes per sector
    pub fn bytes_per_sector(&self) -> usize {
        SECTOR_SIZE
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
    fn test_insert_and_read_blank_disk() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();
        assert!(controller.has_disk(0));

        // Reading from a blank disk should return 0xE5 bytes (CP/M format)
        let sector = controller.read_logical_sector(0, 0, 0).unwrap();
        assert_eq!(sector[0], 0xE5);
    }

    #[test]
    fn test_write_and_read_sector() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();

        let data = [0x42u8; SECTOR_SIZE];
        controller.write_logical_sector(0, 0, 0, &data).unwrap();

        let read_back = controller.read_logical_sector(0, 0, 0).unwrap();
        assert_eq!(read_back[0], 0x42);
        // Other sectors should still be 0xE5
        let other_sector = controller.read_logical_sector(0, 0, 1).unwrap();
        assert_eq!(other_sector[0], 0xE5);
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

        let data_a = [0xAAu8; SECTOR_SIZE];
        let data_b = [0xBBu8; SECTOR_SIZE];

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

    #[test]
    fn test_eject_disk() {
        let mut controller = TarbellController::new();
        controller.insert_blank_disk(0).unwrap();
        assert!(controller.has_disk(0));

        let disk = controller.eject_disk(0);
        assert!(disk.is_some());
        assert!(!controller.has_disk(0));
    }

    #[test]
    fn test_insert_disk_image() {
        let mut controller = TarbellController::new();
        let disk = DiskImage::new_formatted();
        controller.insert_disk_image(0, disk).unwrap();
        assert!(controller.has_disk(0));
    }
}