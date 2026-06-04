use crate::chips::Fd1771;
use crate::disk::DiskImage;

/// Tarbell 1011/1011B floppy controller card wrapping the FD1771.
pub struct TarbellCard {
    fdc: Fd1771,
}

impl TarbellCard {
    pub fn new() -> Self {
        Self { fdc: Fd1771::new() }
    }

    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        let disk = DiskImage::load(std::path::Path::new(path))?;
        self.fdc.insert_disk(drive, disk)
    }

    pub fn insert_blank_disk(&mut self, drive: usize) -> Result<(), String> {
        self.fdc.insert_blank_disk(drive)
    }

    pub fn get_disk(&self, drive: usize) -> Option<&DiskImage> {
        self.fdc.get_disk(drive)
    }
    pub fn get_disk_mut(&mut self, drive: usize) -> Option<&mut DiskImage> {
        self.fdc.get_disk_mut(drive)
    }
    pub fn fdc_mut(&mut self) -> &mut Fd1771 {
        &mut self.fdc
    }
    pub fn fdc(&self) -> &Fd1771 {
        &self.fdc
    }
    pub fn current_track(&self) -> u8 {
        self.fdc.track_register()
    }
    pub fn current_sector(&self) -> u8 {
        self.fdc.sector_register()
    }
    pub fn has_disk(&self, drive: usize) -> bool {
        self.fdc.has_disk(drive)
    }

    /// Map Tarbell board port to FD1771 register offset (0-3). 0x48-0x4B direct, 0xF8-0xFB alias.
    fn port_to_fd1771_offset(port: u8) -> Option<u8> {
        match port {
            0x48..=0x4B => Some(port - 0x48),
            0xF8..=0xFB => Some(port - 0xF8),
            _ => None,
        }
    }
}

impl Default for TarbellCard {
    fn default() -> Self {
        Self::new()
    }
}

impl TarbellCard {
    pub fn io_read(&mut self, port: u8) -> u8 {
        match port {
            0x48..=0x4B | 0xF8..=0xFB => {
                let offset = Self::port_to_fd1771_offset(port).unwrap();
                self.fdc.read_register(offset)
            }
            // DRQ/wait status: bit 7 set = data byte ready, clear = transfer
            // complete. Boot ROMs test the sign bit (RAL/JM) after IN 0xFC.
            0xFC => {
                if self.fdc.is_drq_active() {
                    0x80
                } else {
                    0x00
                }
            }
            0xFD => 0x00,
            0xFF => 0x03,
            _ => 0xFF,
        }
    }

    pub fn io_write(&mut self, port: u8, value: u8) {
        match port {
            0x48..=0x4B | 0xF8..=0xFB => {
                let offset = Self::port_to_fd1771_offset(port).unwrap();
                self.fdc.write_register(offset, value);
            }
            _ => {}
        }
    }

    pub fn owns_port(&self, port: u8) -> bool {
        matches!(port, 0x48..=0x4B | 0xF8..=0xFD | 0xFF)
    }

    pub fn owns_address(&self, _addr: u16) -> bool {
        false
    }

    pub fn name(&self) -> &'static str {
        "Tarbell 1011"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tarbell_card_port_decode() {
        let card = TarbellCard::new();
        assert!(card.owns_port(0x48));
        assert!(card.owns_port(0x49));
        assert!(card.owns_port(0x4A));
        assert!(card.owns_port(0x4B));
        assert!(card.owns_port(0xF8));
        assert!(card.owns_port(0xF9));
        assert!(card.owns_port(0xFA));
        assert!(card.owns_port(0xFB));
        assert!(card.owns_port(0xFC));
        assert!(card.owns_port(0xFD));
        assert!(card.owns_port(0xFF));
        assert!(!card.owns_port(0x00));
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
        card.io_write(0x49, 5);
        assert_eq!(card.io_read(0x49), 5);
        card.io_write(0xF9, 10);
        assert_eq!(card.io_read(0xF9), 10);
        assert_eq!(card.io_read(0x49), 10);
    }

    #[test]
    fn test_tarbell_card_sector_register() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();
        card.io_write(0x4A, 15);
        assert_eq!(card.io_read(0x4A), 15);
        assert_eq!(card.io_read(0xFA), 15);
    }

    #[test]
    fn test_tarbell_card_auxiliary_ports() {
        let mut card = TarbellCard::new();
        assert_eq!(card.io_read(0xFD), 0x00);
        assert_eq!(card.io_read(0xFF), 0x03);
        assert_eq!(card.io_read(0xFC), 0x00);
    }

    #[test]
    fn test_tarbell_card_restore_command() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();
        card.io_write(0x4B, 5);
        card.io_write(0x48, 0x10);
        assert_eq!(card.io_read(0x49), 5);
        card.io_write(0x48, 0x00);
        assert_eq!(card.io_read(0x49), 0);
        let status = card.io_read(0x48);
        assert_eq!(status & 0x04, 0x04);
    }

    #[test]
    fn test_tarbell_card_read_sector() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();
        let pattern = [0xAA; 128];
        card.get_disk_mut(0)
            .unwrap()
            .write_sector(0, 1, &pattern)
            .unwrap();
        card.io_write(0x49, 0);
        card.io_write(0x4A, 1);
        card.io_write(0x48, 0x88);
        let mut read_data = [0u8; 128];
        for i in 0..128 {
            read_data[i] = card.io_read(0x4B);
        }
        assert_eq!(read_data, pattern);
    }

    #[test]
    fn test_tarbell_card_drq_status_in_bit7() {
        let mut card = TarbellCard::new();
        card.insert_blank_disk(0).unwrap();
        // No transfer in progress: DRQ clear.
        assert_eq!(card.io_read(0xFC), 0x00);
        // Issue a READ SECTOR command; a byte becomes available (DRQ active).
        card.io_write(0x49, 0); // track
        card.io_write(0x4A, 1); // sector
        card.io_write(0x48, 0x88); // read command
                                   // DRQ must be reported in bit 7 so `RAL`/`JM` boot-ROM polling sees it.
        assert_eq!(card.io_read(0xFC) & 0x80, 0x80);
    }

    #[test]
    fn test_tarbell_card_disk_insert_eject() {
        let mut card = TarbellCard::new();
        assert!(!card.has_disk(0));
        card.insert_blank_disk(0).unwrap();
        assert!(card.has_disk(0));
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
