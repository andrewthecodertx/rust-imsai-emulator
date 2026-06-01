//! Tarbell 1011/1011B floppy disk controller card (WD FD1771 + board logic)
//!
//! The Tarbell 1011 is an S-100 bus floppy disk controller board built around
//! the Western Digital FD1771 floppy disk formatter/controller chip. The board
//! adds address decoding, drive select logic, a wait state generator, and
//! auxiliary ports beyond the FD1771's four registers.
//!
//! I/O port map:
//!
//! | Port | Read            | Write              |
//! |------|-----------------|--------------------|
//! | 0x48 | FD1771 status   | FD1771 command     |
//! | 0x49 | FD1771 track    | FD1771 track       |
//! | 0x4A | FD1771 sector   | FD1771 sector      |
//! | 0x4B | FD1771 data     | FD1771 data        |
//! | 0xF8 | Tarbell (0x48)  | Tarbell (0x48)     |
//! | 0xF9 | Tarbell (0x49)  | Tarbell (0x49)     |
//! | 0xFA | Tarbell (0x4A)  | Tarbell (0x4A)     |
//! | 0xFB | Tarbell (0x4B)  | Tarbell (0x4B)     |
//! | 0xFC | DRQ/wait status | (unused)           |
//! | 0xFD | (returns 0x00)  | (unused)           |
//! | 0xFF | (returns 0x03)  | (unused)           |
//!
//! Ports 0xF8-0xFB are aliases for 0x48-0x4B, used by some CP/M BIOS versions.
//! Port 0xFC returns the DRQ/wait status (bit 7 = DRQ active).
//! Ports 0xFD and 0xFF return fixed values used by certain boot ROMs.

use crate::chips::Fd1771;
use crate::disk::DiskImage;

/// Tarbell 1011/1011B floppy disk controller card for the S-100 bus.
///
/// Wraps an FD1771 chip with board-level address decoding and
/// auxiliary port logic specific to the Tarbell board design.
pub struct TarbellCard {
    /// The FD1771 floppy disk controller chip
    fdc: Fd1771,
}

impl TarbellCard {
    /// Create a new Tarbell card with an FD1771 controller.
    pub fn new() -> Self {
        Self { fdc: Fd1771::new() }
    }

    /// Insert a disk image file into a drive (0-3).
    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        let disk = DiskImage::load(std::path::Path::new(path))?;
        self.fdc.insert_disk(drive, disk)
    }

    /// Insert a blank formatted disk into a drive (0-3).
    pub fn insert_blank_disk(&mut self, drive: usize) -> Result<(), String> {
        self.fdc.insert_blank_disk(drive)
    }

    /// Get a reference to the disk in a drive.
    pub fn get_disk(&self, drive: usize) -> Option<&DiskImage> {
        self.fdc.get_disk(drive)
    }

    /// Get a mutable reference to the disk in a drive.
    pub fn get_disk_mut(&mut self, drive: usize) -> Option<&mut DiskImage> {
        self.fdc.get_disk_mut(drive)
    }

    /// Get a mutable reference to the FD1771 controller.
    pub fn fdc_mut(&mut self) -> &mut Fd1771 {
        &mut self.fdc
    }

    /// Get a reference to the FD1771 controller.
    pub fn fdc(&self) -> &Fd1771 {
        &self.fdc
    }

    /// Get the current track register value.
    pub fn current_track(&self) -> u8 {
        self.fdc.track_register()
    }

    /// Get the current sector register value.
    pub fn current_sector(&self) -> u8 {
        self.fdc.sector_register()
    }

    /// Check if a drive has a disk inserted.
    pub fn has_disk(&self, drive: usize) -> bool {
        self.fdc.has_disk(drive)
    }

    /// Map a Tarbell board port to the FD1771 register offset (0-3).
    ///
    /// Ports 0x48-0x4B map directly. Ports 0xF8-0xFB are aliases.
    /// Returns None for ports that don't map to FD1771 registers.
    fn port_to_fd1771_offset(port: u8) -> Option<u8> {
        match port {
            0x48..=0x4B => Some(port - 0x48),
            0xF8..=0xFB => Some(port - 0xF8),
            _ => None,
        }
    }
}

impl Default for TarbellCard {
    fn default() -> Self { Self::new() }
}

impl super::Card for TarbellCard {
    fn io_read(&mut self, port: u8) -> u8 {
        match port {
            // FD1771 register ports (primary + aliases)
            0x48..=0x4B | 0xF8..=0xFB => {
                let offset = Self::port_to_fd1771_offset(port).unwrap();
                self.fdc.read_register(offset)
            }
            // Tarbell board DRQ/wait status port (CMI5619 boot ROM compatible)
            // Bit 7 set = DRQ active (data byte ready to read/write)
            // Bit 7 clear = transfer complete
            0xFC => self.fdc.is_drq_active() as u8 | 0x00,
            // Fixed-value ports used by certain boot ROMs
            0xFD => 0x00,
            0xFF => 0x03,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u8, value: u8) {
        match port {
            // FD1771 register ports (primary + aliases)
            0x48..=0x4B | 0xF8..=0xFB => {
                let offset = Self::port_to_fd1771_offset(port).unwrap();
                self.fdc.write_register(offset, value);
            }
            // Auxiliary ports: read-only or unused, ignore writes
            0xFC | 0xFD | 0xFF => {}
            _ => {}
        }
    }

    fn owns_port(&self, port: u8) -> bool {
        matches!(port, 0x48..=0x4B | 0xF8..=0xFD | 0xFF)
    }

    fn mem_read(&self, _addr: u16) -> Option<u8> { None }
    fn mem_write(&mut self, _addr: u16, _value: u8) -> bool { false }
    fn owns_address(&self, _addr: u16) -> bool { false }

    fn name(&self) -> &'static str { "Tarbell 1011" }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Card;

    #[test]
    fn test_tarbell_card_port_decode() {
        let card = TarbellCard::new();
        // Primary FD1771 ports
        assert!(card.owns_port(0x48));
        assert!(card.owns_port(0x49));
        assert!(card.owns_port(0x4A));
        assert!(card.owns_port(0x4B));
        // Alias ports (Tarbell board)
        assert!(card.owns_port(0xF8));
        assert!(card.owns_port(0xF9));
        assert!(card.owns_port(0xFA));
        assert!(card.owns_port(0xFB));
        // Auxiliary ports
        assert!(card.owns_port(0xFC));
        assert!(card.owns_port(0xFD));
        assert!(card.owns_port(0xFF));
        // Unowned ports
        assert!(!card.owns_port(0x00));
        assert!(!card.owns_port(0x01));
        assert!(!card.owns_port(0x47));
        assert!(!card.owns_port(0x4C));
    }

    #[test]
    fn test_tarbell_card_does_not_own_memory() {
        let card = TarbellCard::new();
        assert!(!card.owns_address(0x0000));
        assert!(!card.owns_address(0xFFFF));
    }

    #[test]
    fn test_tarbell_card_name() {
        let card = TarbellCard::new();
        assert_eq!(card.name(), "Tarbell 1011");
    }

    #[test]
    fn test_tarbell_card_fd1771_register_access() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();

        // Write to track register via primary port
        card.io_write(0x49, 5);
        assert_eq!(card.io_read(0x49), 5);

        // Write to track register via alias port
        card.io_write(0xF9, 10);
        assert_eq!(card.io_read(0xF9), 10);

        // Primary and alias ports should access the same register
        assert_eq!(card.io_read(0x49), 10);
    }

    #[test]
    fn test_tarbell_card_sector_register() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();

        card.io_write(0x4A, 15); // Primary port
        assert_eq!(card.io_read(0x4A), 15);
        assert_eq!(card.io_read(0xFA), 15); // Alias port
    }

    #[test]
    fn test_tarbell_card_auxiliary_ports() {
        let mut card = TarbellCard::new();

        // Port 0xFD returns 0x00
        assert_eq!(card.io_read(0xFD), 0x00);
        // Port 0xFF returns 0x03
        assert_eq!(card.io_read(0xFF), 0x03);
        // Port 0xFC returns DRQ status (no disk, no DRQ)
        // Without a disk, DRQ should be inactive (0x00)
        assert_eq!(card.io_read(0xFC), 0x00);
    }

    #[test]
    fn test_tarbell_card_restore_command() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();

        // Seek to track 5 first
        card.io_write(0x4B, 5); // Data register = 5
        card.io_write(0x48, 0x10); // SEEK command

        // Verify track register is 5
        assert_eq!(card.io_read(0x49), 5);

        // RESTORE command (seek to track 0)
        card.io_write(0x48, 0x00);

        // Track register should be 0
        assert_eq!(card.io_read(0x49), 0);
        // Status should have TRACK0 set
        let status = card.io_read(0x48);
        assert_eq!(status & 0x04, 0x04); // TRACK0 bit
    }

    #[test]
    fn test_tarbell_card_read_sector() {
        let mut card = TarbellCard::new();

        // Insert a disk and write a known pattern
        card.insert_blank_disk(0).unwrap();
        let pattern = [0xAA; 128];
        card.get_disk_mut(0).unwrap().write_sector(0, 1, &pattern).unwrap();

        // Seek to track 0
        card.io_write(0x49, 0); // Track = 0
        card.io_write(0x4A, 1);  // Sector = 1
        card.io_write(0x48, 0x88); // READ SECTOR command

        // Read data register 128 times
        let mut read_data = [0u8; 128];
        for i in 0..128 {
            read_data[i] = card.io_read(0x4B);
        }
        assert_eq!(read_data, pattern);
    }

    #[test]
    fn test_tarbell_card_disk_insert_eject() {
        let mut card = TarbellCard::new();
        assert!(!card.has_disk(0));

        card.insert_blank_disk(0).unwrap();
        assert!(card.has_disk(0));

        // Eject via FD1771 directly
        card.fdc_mut().eject_disk(0);
        assert!(!card.has_disk(0));
    }

    #[test]
    fn test_tarbell_card_unowned_port_returns_ff() {
        let mut card = TarbellCard::new();
        assert_eq!(card.io_read(0x47), 0xFF);
        assert_eq!(card.io_read(0x4C), 0xFF);
        assert_eq!(card.io_read(0x00), 0xFF);
    }
}