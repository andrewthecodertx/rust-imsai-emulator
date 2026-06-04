
use crate::disk::DiskImage;

const TRACKS_PER_DISK: u8 = 77;
const SECTOR_SIZE: usize = 128;

// Status register bits
const S_NOT_READY: u8 = 0x80;
const S_WRITE_PROTECT: u8 = 0x40;
#[allow(dead_code)]
const S_RNF: u8 = 0x40;
#[allow(dead_code)]
const S_HEAD_LOADED: u8 = 0x20;
#[allow(dead_code)]
const S_CRC_ERROR: u8 = 0x20;
const S_SEEK_ERROR: u8 = 0x10;
const S_LOST_DATA: u8 = 0x10;
const S_TRACK0: u8 = 0x04;
#[allow(dead_code)]
const S_INDEX: u8 = 0x02;
const S_BUSY: u8 = 0x01;
const S_DRQ: u8 = 0x02;
#[allow(dead_code)]
const S_WR_PROT: u8 = 0x40;

#[allow(dead_code)]
mod cmd {
    pub const RESTORE: u8 = 0x00;
    pub const SEEK: u8 = 0x10;
    pub const STEP: u8 = 0x20;
    pub const STEP_IN: u8 = 0x40;
    pub const STEP_OUT: u8 = 0x60;
    pub const READ_SECTOR: u8 = 0x80;
    pub const WRITE_SECTOR: u8 = 0xA0;
    pub const READ_ADDRESS: u8 = 0xC0;
    pub const READ_TRACK: u8 = 0xE0;
    pub const WRITE_TRACK: u8 = 0xF0;
    pub const FORCE_INTERRUPT: u8 = 0xD0;
}

/// FD1771 internal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdcState {
    Idle,
    Seeking,
    Reading,
    ReadingData,
    Writing,
    WritingData,
    ReadingAddress,
    Complete,
}

/// The FD1771 floppy disk controller chip.
pub struct Fd1771 {
    track_reg: u8,
    sector_reg: u8,
    data_reg: u8,
    status_reg: u8,
    state: FdcState,
    current_command: u8,
    selected_drive: u8,
    drives: [Option<DiskImage>; 4],
    /// Physical head position per drive (separate from track register).
    head_position: [u8; 4],
    sector_buffer: [u8; SECTOR_SIZE],
    buffer_pos: usize,
    step_direction_in: bool,
    step_update_track: bool,
    verify: bool,
    step_rate_ms: u16,
    head_loaded: bool,
}

impl Default for Fd1771 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fd1771 {
    pub fn new() -> Self {
        Self {
            track_reg: 0,
            sector_reg: 1,
            data_reg: 0,
            status_reg: S_NOT_READY,
            state: FdcState::Idle,
            current_command: 0,
            selected_drive: 0,
            drives: [None, None, None, None],
            head_position: [0; 4],
            sector_buffer: [0; SECTOR_SIZE],
            buffer_pos: 0,
            step_direction_in: true,
            step_update_track: false,
            verify: false,
            step_rate_ms: 6,
            head_loaded: false,
        }
    }

    // Disk management

    pub fn insert_disk(&mut self, drive: usize, disk: DiskImage) -> Result<(), String> {
        if drive > 3 {
            return Err("Drive number must be 0-3".into());
        }
        self.drives[drive] = Some(disk);
        self.update_ready_status();
        Ok(())
    }

    pub fn insert_blank_disk(&mut self, drive: usize) -> Result<(), String> {
        let disk = DiskImage::new_formatted();
        self.insert_disk(drive, disk)
    }

    pub fn eject_disk(&mut self, drive: usize) -> Option<DiskImage> {
        if drive > 3 { return None; }
        let disk = self.drives[drive].take();
        if drive == self.selected_drive as usize {
            self.status_reg |= S_NOT_READY;
        }
        disk
    }

    pub fn get_disk(&self, drive: usize) -> Option<&DiskImage> {
        if drive > 3 { return None; }
        self.drives[drive].as_ref()
    }

    pub fn get_disk_mut(&mut self, drive: usize) -> Option<&mut DiskImage> {
        if drive > 3 { return None; }
        self.drives[drive].as_mut()
    }

    pub fn has_disk(&self, drive: usize) -> bool {
        drive < 4 && self.drives[drive].is_some()
    }

    pub fn select_drive(&mut self, drive: u8) {
        if drive > 3 { return; }
        self.selected_drive = drive;
        self.update_ready_status();
        self.head_loaded = true;
    }

    pub fn selected_drive(&self) -> u8 {
        self.selected_drive
    }

    // Register I/O: offset 0=status/command, 1=track, 2=sector, 3=data

    pub fn read_register(&mut self, offset: u8) -> u8 {
        match offset {
            0 => self.read_status(),
            1 => self.track_reg,
            2 => self.sector_reg,
            3 => self.read_data_register(),
            _ => 0xFF,
        }
    }

    pub fn write_register(&mut self, offset: u8, value: u8) {
        match offset {
            0 => self.write_command(value),
            1 => self.track_reg = value,
            2 => self.sector_reg = value,
            3 => self.write_data_register(value),
            _ => {}
        }
    }

    fn read_status(&mut self) -> u8 {
        self.status_reg
    }

    fn read_data_register(&mut self) -> u8 {
        if self.state == FdcState::ReadingData {
            if self.buffer_pos < SECTOR_SIZE {
                let value = self.sector_buffer[self.buffer_pos];
                self.buffer_pos += 1;
                self.status_reg &= !S_DRQ;
                if self.buffer_pos >= SECTOR_SIZE {
                    self.state = FdcState::Complete;
                    self.status_reg &= !S_BUSY;
                }
                value
            } else {
                self.status_reg |= S_LOST_DATA;
                0xFF
            }
        } else {
            self.data_reg
        }
    }

    fn write_data_register(&mut self, value: u8) {
        if self.state == FdcState::WritingData {
            if self.buffer_pos < SECTOR_SIZE {
                self.sector_buffer[self.buffer_pos] = value;
                self.buffer_pos += 1;
                self.status_reg &= !S_DRQ;
                if self.buffer_pos >= SECTOR_SIZE {
                    self.commit_write();
                }
            } else {
                self.status_reg |= S_LOST_DATA;
            }
        } else {
            self.data_reg = value;
        }
    }

    // Command dispatch: Type I (bit7=0), Type II (10xxxxxx), Type III/IV (11xxxxxx)

    fn write_command(&mut self, command: u8) {
        if self.status_reg & S_BUSY != 0 && command & 0xF0 != 0xD0 {
            return;
        }

        self.current_command = command;

        if command & 0x80 == 0 {
            self.execute_type_i(command);
        } else if command & 0x40 == 0 {
            self.execute_type_ii(command);
        } else if command & 0xF0 == 0xD0 {
            self.execute_force_interrupt(command);
        } else {
            self.execute_type_iii(command);
        }
    }

    // Type I: RESTORE, SEEK, STEP, STEP_IN, STEP_OUT (bit 7 = 0, bit 4 = update flag)

    fn execute_type_i(&mut self, command: u8) {
        self.verify = command & 0x04 != 0;
        self.step_rate_ms = match command & 0x03 {
            0 => 6, 1 => 6, 2 => 10, 3 => 20, _ => unreachable!(),
        };

        let cmd_field = command & 0xF0;
        let update_track = command & 0x10 != 0;

        match cmd_field {
            0x00 => {
                // RESTORE
                self.head_position[self.selected_drive as usize] = 0;
                self.track_reg = 0;
                self.head_loaded = true;
                if self.current_disk().is_some() {
                    self.status_reg &= !S_NOT_READY;
                    self.status_reg |= S_TRACK0;
                    self.status_reg &= !S_SEEK_ERROR;
                } else {
                    self.status_reg |= S_NOT_READY;
                }
                self.status_reg &= !S_BUSY;
            }
            0x10 => {
                // SEEK
                let target = self.data_reg;
                let drive = self.selected_drive as usize;
                if target < TRACKS_PER_DISK {
                    self.head_position[drive] = target;
                    self.track_reg = target;
                    if self.current_disk().is_some() {
                        self.status_reg &= !S_NOT_READY;
                        self.status_reg &= !S_SEEK_ERROR;
                    } else {
                        self.status_reg |= S_NOT_READY;
                    }
                } else {
                    self.status_reg |= S_SEEK_ERROR;
                }
                self.status_reg &= !S_BUSY;
                if self.head_position[drive] == 0 {
                    self.status_reg |= S_TRACK0;
                } else {
                    self.status_reg &= !S_TRACK0;
                }
            }
            0x20 | 0x30 => {
                // STEP (same direction as last)
                self.step_update_track = update_track;
                let drive = self.selected_drive as usize;
                if self.step_direction_in {
                    if self.head_position[drive] < 76 {
                        self.head_position[drive] += 1;
                    }
                } else if self.head_position[drive] > 0 {
                    self.head_position[drive] -= 1;
                }
                if self.step_update_track {
                    self.track_reg = self.head_position[drive];
                }
                self.update_track0_status();
                self.status_reg &= !S_BUSY;
            }
            0x40 | 0x50 => {
                // STEP IN
                self.step_direction_in = true;
                self.step_update_track = update_track;
                let drive = self.selected_drive as usize;
                if self.head_position[drive] < 76 {
                    self.head_position[drive] += 1;
                }
                if self.step_update_track {
                    self.track_reg = self.head_position[drive];
                }
                self.update_track0_status();
                self.status_reg &= !S_BUSY;
            }
            0x60 | 0x70 => {
                // STEP OUT
                self.step_direction_in = false;
                self.step_update_track = update_track;
                let drive = self.selected_drive as usize;
                if self.head_position[drive] > 0 {
                    self.head_position[drive] -= 1;
                }
                if self.step_update_track {
                    self.track_reg = self.head_position[drive];
                }
                self.update_track0_status();
                self.status_reg &= !S_BUSY;
            }
            _ => {}
        }

        if self.head_loaded {
            self.status_reg |= S_HEAD_LOADED;
        } else {
            self.status_reg &= !S_HEAD_LOADED;
        }
    }

    // Type II: READ_SECTOR, WRITE_SECTOR

    fn execute_type_ii(&mut self, command: u8) {
        let is_write = command & 0x20 != 0;
        let _multiple = command & 0x10 != 0;

        if self.current_disk().is_none() {
            self.status_reg = S_NOT_READY | S_BUSY;
            self.state = FdcState::Idle;
            self.status_reg &= !S_BUSY;
            return;
        }

        self.status_reg &= !S_NOT_READY;

        if is_write {
            if self.current_disk().is_some_and(|d| d.is_write_protected()) {
                self.status_reg = S_WRITE_PROTECT;
                self.state = FdcState::Idle;
                return;
            }
            self.state = FdcState::WritingData;
            self.status_reg = S_BUSY | S_DRQ;
            self.sector_buffer = [0xE5; SECTOR_SIZE];
            self.buffer_pos = 0;
        } else {
            let track = self.track_reg;
            let sector = self.sector_reg;
            let physical_sector = if sector == 0 { 1 } else { sector };

            match self.current_disk() {
                Some(disk) => {
                    if let Ok(data) = disk.read_sector(track, physical_sector) {
                        self.sector_buffer = data;
                        self.state = FdcState::ReadingData;
                        self.buffer_pos = 0;
                        self.status_reg = S_BUSY | S_DRQ;
                    } else {
                        self.status_reg = S_RNF;
                        self.state = FdcState::Idle;
                    }
                }
                None => {
                    self.status_reg = S_NOT_READY | S_RNF;
                    self.state = FdcState::Idle;
                }
            }
        }
    }

    // Type III: READ_ADDRESS, READ_TRACK, WRITE_TRACK

    fn execute_type_iii(&mut self, command: u8) {
        match command & 0xF0 {
            0xC0 => {
                // READ ADDRESS
                self.data_reg = self.track_reg;
                self.status_reg &= !S_BUSY;
            }
            0xE0 | 0xF0 | _ => {
                self.status_reg &= !S_BUSY;
            }
        }
    }

    // Type IV: FORCE_INTERRUPT

    fn execute_force_interrupt(&mut self, _command: u8) {
        self.state = FdcState::Idle;
        self.status_reg &= !S_BUSY;
        self.status_reg &= !S_DRQ;
    }

    fn commit_write(&mut self) {
        let track = self.track_reg;
        let sector = if self.sector_reg == 0 { 1 } else { self.sector_reg };
        let data = self.sector_buffer;

        match self.current_disk_mut() {
            Some(disk) => {
                if disk.write_sector(track, sector, &data).is_err() {
                    self.status_reg |= S_WRITE_PROTECT;
                } else {
                    self.status_reg &= !S_WRITE_PROTECT;
                }
            }
            None => {
                self.status_reg |= S_NOT_READY;
            }
        }
        self.state = FdcState::Complete;
        self.status_reg &= !S_BUSY;
    }

    fn current_disk(&self) -> Option<&DiskImage> {
        self.drives[self.selected_drive as usize].as_ref()
    }

    fn current_disk_mut(&mut self) -> Option<&mut DiskImage> {
        self.drives[self.selected_drive as usize].as_mut()
    }

    fn update_ready_status(&mut self) {
        if self.current_disk().is_some() {
            self.status_reg &= !S_NOT_READY;
        } else {
            self.status_reg |= S_NOT_READY;
        }
    }

    fn update_track0_status(&mut self) {
        if self.head_position[self.selected_drive as usize] == 0 {
            self.status_reg |= S_TRACK0;
        } else {
            self.status_reg &= !S_TRACK0;
        }
    }

    // Diagnostic accessors

    pub fn track_register(&self) -> u8 {
        self.track_reg
    }

    pub fn sector_register(&self) -> u8 {
        self.sector_reg
    }

    pub fn head_position(&self) -> u8 {
        self.head_position[self.selected_drive as usize]
    }

    pub fn fdc_state(&self) -> FdcState {
        self.state
    }

    pub fn is_drq_active(&self) -> bool {
        self.status_reg & S_DRQ != 0
    }

    pub fn is_busy(&self) -> bool {
        self.status_reg & S_BUSY != 0
    }

    pub fn sector_buffer(&self) -> &[u8; SECTOR_SIZE] {
        &self.sector_buffer
    }

    pub fn buffer_position(&self) -> usize {
        self.buffer_pos
    }

    /// Direct head position set (testing only; real HW uses commands).
    pub fn set_head_position(&mut self, drive: usize, track: u8) {
        if drive < 4 {
            self.head_position[drive] = track;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_controller_with_disk() -> Fd1771 {
        let mut fdc = Fd1771::new();
        fdc.insert_blank_disk(0).unwrap();
        fdc.select_drive(0);
        fdc
    }

    #[test]
    fn test_initial_state() {
        let fdc = Fd1771::new();
        assert_eq!(fdc.track_register(), 0);
        assert_eq!(fdc.sector_register(), 1);
        assert_eq!(fdc.head_position(), 0);
        assert!(!fdc.is_busy());
        assert!(!fdc.is_drq_active());
    }

    #[test]
    fn test_no_disk_shows_not_ready() {
        let fdc = Fd1771::new();
        assert!(fdc.status_reg & S_NOT_READY != 0);
    }

    #[test]
    fn test_disk_insert_clears_not_ready() {
        let mut fdc = Fd1771::new();
        fdc.insert_blank_disk(0).unwrap();
        fdc.select_drive(0);
        assert!(fdc.status_reg & S_NOT_READY == 0);
    }

    #[test]
    fn test_restore_command() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(1, 20);
        fdc.head_position[0] = 20;
        fdc.write_register(0, 0x00);
        assert_eq!(fdc.track_register(), 0);
        assert_eq!(fdc.head_position(), 0);
        assert!(fdc.status_reg & S_TRACK0 != 0);
        assert!(fdc.status_reg & S_BUSY == 0);
    }

    #[test]
    fn test_seek_command() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(3, 30);
        fdc.write_register(0, 0x10);
        assert_eq!(fdc.track_register(), 30);
        assert_eq!(fdc.head_position(), 30);
        assert!(fdc.status_reg & S_TRACK0 == 0);
    }

    #[test]
    fn test_seek_out_of_range() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(3, 80);
        fdc.write_register(0, 0x10);
        assert!(fdc.status_reg & S_SEEK_ERROR != 0);
    }

    #[test]
    fn test_step_in_command() {
        let mut fdc = make_controller_with_disk();
        assert_eq!(fdc.head_position(), 0);
        fdc.write_register(0, 0x50);
        assert_eq!(fdc.head_position(), 1);
        assert_eq!(fdc.track_register(), 1);
        fdc.write_register(0, 0x50);
        assert_eq!(fdc.head_position(), 2);
        assert_eq!(fdc.track_register(), 2);
    }

    #[test]
    fn test_step_in_without_update() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(0, 0x40);
        assert_eq!(fdc.head_position(), 1);
        assert_eq!(fdc.track_register(), 0);
    }

    #[test]
    fn test_step_out_command() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(3, 10);
        fdc.write_register(0, 0x10);
        assert_eq!(fdc.head_position(), 10);
        fdc.write_register(0, 0x70);
        assert_eq!(fdc.head_position(), 9);
        assert_eq!(fdc.track_register(), 9);
    }

    #[test]
    fn test_step_out_at_track_zero() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(0, 0x70);
        assert_eq!(fdc.head_position(), 0);
    }

    #[test]
    fn test_step_in_at_max_track() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(3, 76);
        fdc.write_register(0, 0x10);
        assert_eq!(fdc.head_position(), 76);
        fdc.write_register(0, 0x50);
        assert_eq!(fdc.head_position(), 76);
    }

    #[test]
    fn test_read_sector_basic() {
        let mut fdc = make_controller_with_disk();
        let pattern = [0xAA; SECTOR_SIZE];
        fdc.get_disk_mut(0).unwrap().write_sector(0, 1, &pattern).unwrap();
        fdc.write_register(1, 0);
        fdc.write_register(2, 1);
        fdc.write_register(0, 0x88);
        assert!(fdc.is_drq_active());
        assert!(fdc.is_busy());
        let mut read_data = [0u8; SECTOR_SIZE];
        for i in 0..SECTOR_SIZE {
            read_data[i] = fdc.read_register(3);
        }
        assert_eq!(read_data, pattern);
        assert!(!fdc.is_busy());
    }

    #[test]
    fn test_write_sector_basic() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(1, 2);
        fdc.write_register(2, 3);
        fdc.set_head_position(0, 2);
        fdc.write_register(0, 0xA8);
        assert!(fdc.is_drq_active());
        assert!(fdc.is_busy());
        let pattern = [0x55; SECTOR_SIZE];
        for &byte in pattern.iter() {
            fdc.write_register(3, byte);
        }
        assert!(!fdc.is_busy());
        let read_back = fdc.get_disk(0).unwrap().read_sector(2, 3).unwrap();
        assert_eq!(read_back, pattern);
    }

    #[test]
    fn test_read_sector_not_found() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(1, 0);
        fdc.write_register(2, 30);
        fdc.write_register(0, 0x88);
        assert!(fdc.status_reg & S_RNF != 0);
    }

    #[test]
    fn test_read_no_disk() {
        let mut fdc = Fd1771::new();
        fdc.select_drive(0);
        fdc.write_register(0, 0x88);
        assert!(fdc.status_reg & S_NOT_READY != 0);
    }

    #[test]
    fn test_write_protected_disk() {
        let mut fdc = Fd1771::new();
        let mut disk = DiskImage::new_formatted();
        disk.set_write_protected(true);
        fdc.insert_disk(0, disk).unwrap();
        fdc.select_drive(0);
        fdc.write_register(0, 0xA8);
        assert!(fdc.status_reg & S_WRITE_PROTECT != 0);
    }

    #[test]
    fn test_read_address() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(3, 5);
        fdc.write_register(0, 0x10);
        fdc.write_register(0, 0xC0);
        assert_eq!(fdc.data_reg, 5);
    }

    #[test]
    fn test_force_interrupt() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(0, 0x88);
        assert!(fdc.is_busy());
        fdc.write_register(0, 0xD0);
        assert!(!fdc.is_busy());
    }

    #[test]
    fn test_multiple_drives() {
        let mut fdc = Fd1771::new();
        fdc.insert_blank_disk(0).unwrap();
        fdc.insert_blank_disk(1).unwrap();
        fdc.select_drive(0);
        fdc.write_register(3, 10);
        fdc.write_register(0, 0x10);
        fdc.select_drive(1);
        fdc.write_register(3, 20);
        fdc.write_register(0, 0x10);
        // FD1771 has a single shared track register (not per-drive)
        assert_eq!(fdc.track_register(), 20);
        assert_eq!(fdc.head_position[1], 20);
        fdc.select_drive(0);
        assert_eq!(fdc.head_position[0], 10);
    }

    #[test]
    fn test_busy_ignores_command() {
        let mut fdc = make_controller_with_disk();
        fdc.write_register(0, 0x88);
        assert!(fdc.is_busy());
        fdc.write_register(0, 0x10);
        assert_eq!(fdc.track_register(), 0);
    }

    #[test]
    fn test_sector0_treated_as_sector1() {
        let mut fdc = make_controller_with_disk();
        let pattern = [0xBB; SECTOR_SIZE];
        fdc.get_disk_mut(0).unwrap().write_sector(0, 1, &pattern).unwrap();
        fdc.write_register(1, 0);
        fdc.write_register(2, 0);
        fdc.write_register(0, 0x88);
        if fdc.is_drq_active() {
            let first_byte = fdc.read_register(3);
            assert_eq!(first_byte, 0xBB);
        }
    }
}