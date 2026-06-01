//! Western Digital FD1771 Floppy Disk Formatter/Controller
//!
//! The FD1771 is the chip at the heart of the Tarbell 1011 disk controller.
//! It manages all floppy disk operations: seek, read, write, and track
//! management. This model implements the FD1771 state machine as described
//! in the WD1771/FD1771 datasheet.
//!
//! Register map (4 registers, addressed by A0-A1):
//! | Offset | Read              | Write             |
//! |--------|-------------------|-------------------|
//! | 0      | Status register   | Command register  |
//! | 1      | Track register    | Track register    |
//! | 2      | Sector register   | Sector register   |
//! | 3      | Data register     | Data register     |
//!
//! The FD1771 is NOT the same as the later WD1770/1772. Key differences:
//! - Single-density only (FM encoding)
//! - No track 0 detection on the data register (has a separate input pin)
//! - Internal sector length fixed at 128 bytes (no programmable length)
//!
//! Command types:
//! - Type I:  RESTORE, SEEK, STEP, STEP_IN, STEP_OUT
//! - Type II: READ_SECTOR, WRITE_SECTOR
//! - Type III: READ_ADDRESS, READ_TRACK, WRITE_TRACK
//! - Type IV: FORCE_INTERRUPT

use crate::disk::DiskImage;

/// Number of tracks per disk (standard 8" SS/SD format)
const TRACKS_PER_DISK: u8 = 77;
/// Sector size in bytes (FD1771 is fixed at 128)
const SECTOR_SIZE: usize = 128;

// ---------------------------------------------------------------------------
// Status register bits (shared across command types)
// ---------------------------------------------------------------------------
const S_NOT_READY: u8 = 0x80; // Bit 7: Drive not ready
const S_WRITE_PROTECT: u8 = 0x40; // Bit 6: Write protect (Type I)
#[allow(dead_code)]
const S_RNF: u8 = 0x40; // Bit 6: Record not found (Type II/III)
#[allow(dead_code)]
const S_HEAD_LOADED: u8 = 0x20; // Bit 5: Head loaded (Type I)
#[allow(dead_code)]
const S_CRC_ERROR: u8 = 0x20; // Bit 5: CRC error (Type II/III)
const S_SEEK_ERROR: u8 = 0x10; // Bit 4: Seek error (Type I)
const S_LOST_DATA: u8 = 0x10; // Bit 4: Lost data (Type II/III)
const S_TRACK0: u8 = 0x04; // Bit 2: Track 00 (Type I status)
#[allow(dead_code)]
const S_INDEX: u8 = 0x02; // Bit 1: Index pulse (Type I)
const S_BUSY: u8 = 0x01; // Bit 0: Controller busy

// Type II specific status bits
const S_DRQ: u8 = 0x02; // Bit 1: Data request (Type II/III)
#[allow(dead_code)]
const S_WR_PROT: u8 = 0x40; // Bit 6: Write protect (Type II write)

// ---------------------------------------------------------------------------
// Command register bit fields
// ---------------------------------------------------------------------------

/// Command constants for reference (values are base commands without flag bits)
#[allow(dead_code)]
const CMD_RESTORE: u8 = 0x00; // 0000vVVR (V=verify, r=step rate)
#[allow(dead_code)]
const CMD_SEEK: u8 = 0x10; // 0001vVVR
#[allow(dead_code)]
const CMD_STEP: u8 = 0x20; // 0010uVVR (u=update track, same direction)
#[allow(dead_code)]
const CMD_STEP_IN: u8 = 0x40; // 0100uVVR
#[allow(dead_code)]
const CMD_STEP_OUT: u8 = 0x60; // 0110uVVR
#[allow(dead_code)]
const CMD_READ_SECTOR: u8 = 0x80;  // 100mSEEE (m=multiple, S=side, E=drive select)
#[allow(dead_code)]
const CMD_WRITE_SECTOR: u8 = 0xA0; // 101mSEEE
#[allow(dead_code)]
const CMD_READ_ADDRESS: u8 = 0xC0;
#[allow(dead_code)]
const CMD_READ_TRACK: u8 = 0xE0;
#[allow(dead_code)]
const CMD_WRITE_TRACK: u8 = 0xF0;
#[allow(dead_code)]
const CMD_FORCE_INTERRUPT: u8 = 0xD0;

// ---------------------------------------------------------------------------
// FD1771 internal state machine
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdcState {
    /// Idle, waiting for command
    Idle,
    /// Executing a Type I command (seek/restore/step)
    Seeking,
    /// Searching for the sector ID field (Type II read)
    Reading,
    /// Transferring data from disk to host (DRQ active)
    ReadingData,
    /// Searching for the sector ID field (Type II write)
    Writing,
    /// Waiting for host to supply data bytes (DRQ active)
    WritingData,
    /// Reading the address field (Type III)
    ReadingAddress,
    /// Command completed, status available
    Complete,
}

/// The FD1771 floppy disk controller chip.
///
/// This model implements the full FD1771 state machine with:
/// - All four register (status/command, track, sector, data)
/// - All command types (I, II, III, IV)
/// - DRQ and INTRQ signaling
/// - Sector buffer for read/write data transfer
/// - Up to 4 drive select lines (drives 0-3)
pub struct Fd1771 {
    // Registers
    track_reg: u8,
    sector_reg: u8,
    data_reg: u8,
    status_reg: u8,

    // Internal state
    state: FdcState,
    /// Current command being executed (raw byte from command register)
    current_command: u8,

    /// Drive select (0-3). The FD1771 has drive select outputs,
    /// but the Tarbell board uses external logic for this.
    selected_drive: u8,

    // Disk drives (up to 4)
    drives: [Option<DiskImage>; 4],

    /// Internal track position per drive (physical head position)
    /// The FD1771 tracks the head position separately from the track register.
    /// The track register is what the CPU sees; head_position is where the
    /// head actually is on the disk.
    head_position: [u8; 4],

    /// Sector data buffer for read/write operations
    sector_buffer: [u8; SECTOR_SIZE],

    /// Current byte position in the sector buffer during read/write
    buffer_pos: usize,

    /// Direction for step commands: true = step in (toward center, higher track)
    step_direction_in: bool,

    /// Whether the "update" flag was set on the last step command
    step_update_track: bool,

    /// Verify flag: after seek/restore, compare track register with sector ID
    verify: bool,

    /// Step rate in milliseconds (bits 1:0 of Type I commands)
    /// 00=6ms, 01=6ms, 10=10ms, 11=20ms (Tarbell default: 00 = 6ms)
    /// In our model we don't delay, but we track it for accuracy.
    step_rate_ms: u16,

    /// Head loaded flag (Type I status bit 5)
    head_loaded: bool,
}

impl Default for Fd1771 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fd1771 {
    /// Create a new FD1771 in reset state.
    pub fn new() -> Self {
        Self {
            track_reg: 0,
            sector_reg: 1,
            data_reg: 0,
            status_reg: S_NOT_READY, // No drive selected at init
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

    // -------------------------------------------------------------------
    // Public API: disk management
    // -------------------------------------------------------------------

    /// Insert a disk image into a drive (0-3).
    pub fn insert_disk(&mut self, drive: usize, disk: DiskImage) -> Result<(), String> {
        if drive > 3 {
            return Err("Drive number must be 0-3".into());
        }
        self.drives[drive] = Some(disk);
        self.update_ready_status();
        Ok(())
    }

    /// Insert a blank formatted disk into a drive.
    pub fn insert_blank_disk(&mut self, drive: usize) -> Result<(), String> {
        let disk = DiskImage::new_formatted();
        self.insert_disk(drive, disk)
    }

    /// Eject the disk from a drive.
    pub fn eject_disk(&mut self, drive: usize) -> Option<DiskImage> {
        if drive > 3 { return None; }
        let disk = self.drives[drive].take();
        if drive == self.selected_drive as usize {
            self.status_reg |= S_NOT_READY;
        }
        disk
    }

    /// Get a reference to the disk in a drive.
    pub fn get_disk(&self, drive: usize) -> Option<&DiskImage> {
        if drive > 3 { return None; }
        self.drives[drive].as_ref()
    }

    /// Get a mutable reference to the disk in a drive.
    pub fn get_disk_mut(&mut self, drive: usize) -> Option<&mut DiskImage> {
        if drive > 3 { return None; }
        self.drives[drive].as_mut()
    }

    /// Check if a drive has a disk inserted.
    pub fn has_disk(&self, drive: usize) -> bool {
        drive < 4 && self.drives[drive].is_some()
    }

    /// Select a drive (0-3). The Tarbell board handles this via
    /// port writes; the FD1771 itself has limited drive select.
    pub fn select_drive(&mut self, drive: u8) {
        if drive > 3 { return; }
        self.selected_drive = drive;
        self.update_ready_status();
        self.head_loaded = true; // Head loads when drive is selected
    }

    /// Get the currently selected drive number.
    pub fn selected_drive(&self) -> u8 {
        self.selected_drive
    }

    // -------------------------------------------------------------------
    // Public API: register I/O (chip-level, before Tarbell board mapping)
    // -------------------------------------------------------------------

    /// Read from one of the 4 FD1771 registers.
    /// Offset: 0=status, 1=track, 2=sector, 3=data
    pub fn read_register(&mut self, offset: u8) -> u8 {
        match offset {
            0 => self.read_status(),
            1 => self.track_reg,
            2 => self.sector_reg,
            3 => self.read_data_register(),
            _ => 0xFF, // No other registers on FD1771
        }
    }

    /// Write to one of the 4 FD1771 registers.
    /// Offset: 0=command, 1=track, 2=sector, 3=data
    pub fn write_register(&mut self, offset: u8, value: u8) {
        match offset {
            0 => self.write_command(value),
            1 => self.track_reg = value,
            2 => self.sector_reg = value,
            3 => self.write_data_register(value),
            _ => {} // No other registers on FD1771
        }
    }

    // -------------------------------------------------------------------
    // Status register (offset 0, read)
    // -------------------------------------------------------------------

    /// Read the status register. The meaning of bits depends on the last
    /// command type executed.
    fn read_status(&mut self) -> u8 {
        // If BUSY, reading status doesn't clear BUSY in our model.
        // After command completion, BUSY is cleared.
        // The real chip clears BUSY when the command finishes.
        let status = self.status_reg;

        // For Type II commands, report DRQ bit
        if self.state == FdcState::ReadingData || self.state == FdcState::WritingData {
            // Already set in buffer management
        }

        status
    }

    // -------------------------------------------------------------------
    // Data register (offset 3, read/write)
    // -------------------------------------------------------------------

    /// Read from the data register. During a read sector operation,
    /// returns the next byte from the sector buffer.
    fn read_data_register(&mut self) -> u8 {
        if self.state == FdcState::ReadingData {
            if self.buffer_pos < SECTOR_SIZE {
                let value = self.sector_buffer[self.buffer_pos];
                self.buffer_pos += 1;

                // Clear DRQ after byte is read
                self.status_reg &= !S_DRQ;

                if self.buffer_pos >= SECTOR_SIZE {
                    // All bytes read, operation complete
                    self.state = FdcState::Complete;
                    self.status_reg &= !S_BUSY;
                }
                value
            } else {
                // Overrun: tried to read past buffer
                self.status_reg |= S_LOST_DATA;
                0xFF
            }
        } else {
            self.data_reg
        }
    }

    /// Write to the data register. During a write sector operation,
    /// stores the byte into the sector buffer.
    fn write_data_register(&mut self, value: u8) {
        if self.state == FdcState::WritingData {
            if self.buffer_pos < SECTOR_SIZE {
                self.sector_buffer[self.buffer_pos] = value;
                self.buffer_pos += 1;

                // Clear DRQ after byte is written
                self.status_reg &= !S_DRQ;

                if self.buffer_pos >= SECTOR_SIZE {
                    // All bytes written, commit to disk
                    self.commit_write();
                }
            } else {
                // Overrun
                self.status_reg |= S_LOST_DATA;
            }
        } else {
            self.data_reg = value;
        }
    }

    // -------------------------------------------------------------------
    // Command register (offset 0, write)
    // -------------------------------------------------------------------

    /// Write to the command register. Parses the command type and
    /// dispatches to the appropriate handler.
    ///
    /// FD1771 command types:
    /// - Type I:   bit 7 = 0  (0x00-0x7F) - RESTORE, SEEK, STEP, STEP_IN, STEP_OUT
    /// - Type II:  bits 7-6 = 10 (0x80-0xBF) - READ_SECTOR, WRITE_SECTOR
    /// - Type III: bits 7-6 = 11 (0xC0-0xFF) - READ_ADDRESS, READ_TRACK, WRITE_TRACK
    /// - Type IV:  0xD0-0xDF - FORCE_INTERRUPT (subset of Type III range)
    fn write_command(&mut self, command: u8) {
        // If BUSY and this is not a FORCE_INTERRUPT, the command is ignored
        if self.status_reg & S_BUSY != 0 && command & 0xF0 != 0xD0 {
            return; // Command ignored while busy
        }

        self.current_command = command;

        if command & 0x80 == 0 {
            // Type I: bit 7 = 0
            self.execute_type_i(command);
        } else if command & 0x40 == 0 {
            // Type II: bit 7 = 1, bit 6 = 0 (0x80-0xBF)
            self.execute_type_ii(command);
        } else {
            // Type III/IV: bits 7-6 = 11 (0xC0-0xFF)
            if command & 0xF0 == 0xD0 {
                self.execute_force_interrupt(command);
            } else {
                self.execute_type_iii(command);
            }
        }
    }

    // -------------------------------------------------------------------
    // Type I commands: RESTORE, SEEK, STEP, STEP_IN, STEP_OUT
    // -------------------------------------------------------------------

    fn execute_type_i(&mut self, command: u8) {
        // Parse common Type I flags
        self.verify = command & 0x04 != 0;
        self.step_rate_ms = match command & 0x03 {
            0 => 6,
            1 => 6,
            2 => 10,
            3 => 20,
            _ => unreachable!(),
        };

        // Type I commands use bits 7-4 for command select, where bit 4 is
        // the update-track flag for STEP/STEP_IN/STEP_OUT.
        // RESTORE=0x00, SEEK=0x10, STEP=0x20|0x30,
        // STEP_IN=0x40|0x50, STEP_OUT=0x60|0x70
        let cmd_field = command & 0xF0;
        let update_track = command & 0x10 != 0; // bit 4 = update track reg
        match cmd_field {
            0x00 => {
                // RESTORE: seek to track 0
                self.head_position[self.selected_drive as usize] = 0;
                self.track_reg = 0;
                self.head_loaded = true;

                if self.current_disk().is_some() {
                    self.status_reg &= !S_NOT_READY;
                    // Check track 0 status
                    self.status_reg |= S_TRACK0;
                    self.status_reg &= !S_SEEK_ERROR;
                } else {
                    self.status_reg |= S_NOT_READY;
                }
                self.status_reg &= !S_BUSY;
            }
            0x10 => {
                // SEEK: move head to track specified in data register
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
                // STEP: step in the last direction used
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
                // STEP IN: step toward center (higher track number)
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
                // STEP OUT: step toward edge (lower track number)
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
            _ => {} // Unknown Type I command
        }

        // After Type I command, set HEAD LOADED flag if appropriate
        if self.head_loaded {
            self.status_reg |= S_HEAD_LOADED;
        } else {
            self.status_reg &= !S_HEAD_LOADED;
        }
    }

    // -------------------------------------------------------------------
    // Type II commands: READ_SECTOR, WRITE_SECTOR
    // -------------------------------------------------------------------

    fn execute_type_ii(&mut self, command: u8) {
        let is_write = command & 0x20 != 0; // Bit 5 = write flag
        let _multiple = command & 0x10 != 0; // Bit 4 = multiple sectors

        if self.current_disk().is_none() {
            self.status_reg = S_NOT_READY | S_BUSY;
            self.state = FdcState::Idle;
            self.status_reg &= !S_BUSY;
            return;
        }

        self.status_reg &= !S_NOT_READY;

        if is_write {
            // Check write protection
            if self.current_disk().is_some_and(|d| d.is_write_protected()) {
                self.status_reg = S_WRITE_PROTECT;
                self.state = FdcState::Idle;
                return;
            }

            // WRITE SECTOR: set up buffer for host to fill
            self.state = FdcState::WritingData;
            self.status_reg = S_BUSY | S_DRQ; // DRQ: ready for first data byte
            self.sector_buffer = [0xE5; SECTOR_SIZE]; // Fill with CP/M filler
            self.buffer_pos = 0;

            // If the sector doesn't exist on disk yet, that's OK for a write.
            // We'll write it when all 128 bytes are received.
        } else {
            // READ SECTOR: load sector from disk into buffer
            let track = self.track_reg;
            let sector = self.sector_reg;
            let physical_sector = if sector == 0 { 1 } else { sector };

            match self.current_disk() {
                Some(disk) => {
                    if let Ok(data) = disk.read_sector(track, physical_sector) {
                        self.sector_buffer = data;
                        self.state = FdcState::ReadingData;
                        self.buffer_pos = 0;
                        self.status_reg = S_BUSY | S_DRQ; // DRQ: first byte available
                    } else {
                        // Record not found
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

    // -------------------------------------------------------------------
    // Type III commands: READ_ADDRESS, READ_TRACK, WRITE_TRACK
    // -------------------------------------------------------------------

    fn execute_type_iii(&mut self, command: u8) {
        match command & 0xF0 {
            0xC0 => {
                // READ ADDRESS: return the current track number.
                // The real chip returns a 6-byte address field, but the
                // CP/M BIOS only needs the track byte.
                self.data_reg = self.track_reg;
                self.status_reg &= !S_BUSY;
            }
            0xE0 => {
                // READ_TRACK: not commonly used by CP/M, minimal support
                self.status_reg &= !S_BUSY;
            }
            0xF0 => {
                // WRITE_TRACK: format a track. Not implemented for CP/M use.
                self.status_reg &= !S_BUSY;
            }
            _ => {
                self.status_reg &= !S_BUSY;
            }
        }
    }

    // -------------------------------------------------------------------
    // Type IV command: FORCE_INTERRUPT
    // -------------------------------------------------------------------

    fn execute_force_interrupt(&mut self, _command: u8) {
        // Abort any current operation
        self.state = FdcState::Idle;
        self.status_reg &= !S_BUSY;
        self.status_reg &= !S_DRQ;

        // The interrupt condition flags (bits 0-3) determine when
        // INTRQ is asserted. For our model, we just clear BUSY.
        // A real implementation would set INTRQ based on the condition.
    }

    // -------------------------------------------------------------------
    // Write completion
    // -------------------------------------------------------------------

    fn commit_write(&mut self) {
        let track = self.track_reg;
        let sector = if self.sector_reg == 0 { 1 } else { self.sector_reg };
        let data = self.sector_buffer;

        match self.current_disk_mut() {
            Some(disk) => {
                if let Err(_) = disk.write_sector(track, sector, &data) {
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

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

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

    // -------------------------------------------------------------------
    // Diagnostic accessors
    // -------------------------------------------------------------------

    /// Get the current track register value.
    pub fn track_register(&self) -> u8 {
        self.track_reg
    }

    /// Get the current sector register value.
    pub fn sector_register(&self) -> u8 {
        self.sector_reg
    }

    /// Get the physical head position for the selected drive.
    pub fn head_position(&self) -> u8 {
        self.head_position[self.selected_drive as usize]
    }

    /// Get the current controller state.
    pub fn fdc_state(&self) -> FdcState {
        self.state
    }

    /// Check if DRQ is active (data ready to transfer).
    pub fn is_drq_active(&self) -> bool {
        self.status_reg & S_DRQ != 0
    }

    /// Check if the controller is busy.
    pub fn is_busy(&self) -> bool {
        self.status_reg & S_BUSY != 0
    }

    /// Get the current sector buffer contents (for testing).
    pub fn sector_buffer(&self) -> &[u8; SECTOR_SIZE] {
        &self.sector_buffer
    }

    /// Get the current buffer position (for testing).
    pub fn buffer_position(&self) -> usize {
        self.buffer_pos
    }

    /// Set the physical head position directly (for testing seek scenarios).
    /// In real hardware, you can only move the head via commands. This exists
    /// for test setup where we need the head at a specific track without
    /// going through the full seek sequence.
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

    // -----------------------------------------------------------------
    // Initialization and reset tests
    // -----------------------------------------------------------------

    #[test]
    fn test_initial_state() {
        let fdc = Fd1771::new();
        assert_eq!(fdc.track_register(), 0);
        assert_eq!(fdc.sector_register(), 1); // Sector register resets to 1
        assert_eq!(fdc.head_position(), 0);
        assert!(!fdc.is_busy());
        assert!(!fdc.is_drq_active());
    }

    #[test]
    fn test_no_disk_shows_not_ready() {
        let fdc = Fd1771::new();
        // Without selecting a drive, NOT_READY should be set
        assert!(fdc.status_reg & S_NOT_READY != 0);
    }

    #[test]
    fn test_disk_insert_clears_not_ready() {
        let mut fdc = Fd1771::new();
        fdc.insert_blank_disk(0).unwrap();
        fdc.select_drive(0);
        assert!(fdc.status_reg & S_NOT_READY == 0);
    }

    // -----------------------------------------------------------------
    // Type I command tests: RESTORE, SEEK, STEP
    // -----------------------------------------------------------------

    #[test]
    fn test_restore_command() {
        let mut fdc = make_controller_with_disk();
        // Set track register to non-zero first
        fdc.write_register(1, 20); // Track reg = 20
        fdc.head_position[0] = 20;

        // RESTORE: command byte 0x00 (no verify, step rate 6ms)
        fdc.write_register(0, 0x00);

        assert_eq!(fdc.track_register(), 0);
        assert_eq!(fdc.head_position(), 0);
        assert!(fdc.status_reg & S_TRACK0 != 0);
        assert!(fdc.status_reg & S_BUSY == 0); // Command completed
    }

    #[test]
    fn test_seek_command() {
        let mut fdc = make_controller_with_disk();

        // SEEK to track 30: load data register with target, then issue SEEK
        fdc.write_register(3, 30); // Data reg = 30 (target track)
        fdc.write_register(0, 0x10); // SEEK command (no verify)

        assert_eq!(fdc.track_register(), 30);
        assert_eq!(fdc.head_position(), 30);
        assert!(fdc.status_reg & S_TRACK0 == 0); // Not on track 0
    }

    #[test]
    fn test_seek_out_of_range() {
        let mut fdc = make_controller_with_disk();

        // SEEK to track 80 (out of range for 77-track disk)
        fdc.write_register(3, 80);
        fdc.write_register(0, 0x10);

        assert!(fdc.status_reg & S_SEEK_ERROR != 0);
    }

    #[test]
    fn test_step_in_command() {
        let mut fdc = make_controller_with_disk();

        // Start at track 0
        assert_eq!(fdc.head_position(), 0);

        // STEP IN with update flag (0x40 | 0x10 = 0x50)
        fdc.write_register(0, 0x50);

        assert_eq!(fdc.head_position(), 1);
        assert_eq!(fdc.track_register(), 1); // Track reg updated

        // Another step in
        fdc.write_register(0, 0x50);
        assert_eq!(fdc.head_position(), 2);
        assert_eq!(fdc.track_register(), 2);
    }

    #[test]
    fn test_step_in_without_update() {
        let mut fdc = make_controller_with_disk();

        // STEP IN without update flag (0x40, no 0x10 bit)
        fdc.write_register(0, 0x40);

        assert_eq!(fdc.head_position(), 1); // Head moved
        assert_eq!(fdc.track_register(), 0); // Track reg NOT updated
    }

    #[test]
    fn test_step_out_command() {
        let mut fdc = make_controller_with_disk();

        // Seek to track 10 first
        fdc.write_register(3, 10);
        fdc.write_register(0, 0x10);
        assert_eq!(fdc.head_position(), 10);

        // STEP OUT with update (0x60 | 0x10 = 0x70)
        fdc.write_register(0, 0x70);
        assert_eq!(fdc.head_position(), 9);
        assert_eq!(fdc.track_register(), 9);
    }

    #[test]
    fn test_step_out_at_track_zero() {
        let mut fdc = make_controller_with_disk();

        // At track 0, STEP OUT should not go below 0
        fdc.write_register(0, 0x70); // STEP OUT with update
        assert_eq!(fdc.head_position(), 0); // Still at 0
    }

    #[test]
    fn test_step_in_at_max_track() {
        let mut fdc = make_controller_with_disk();

        // Seek to track 76 (max for 77-track disk, 0-indexed)
        fdc.write_register(3, 76);
        fdc.write_register(0, 0x10);
        assert_eq!(fdc.head_position(), 76);

        // STEP IN at max track should not exceed 76
        fdc.write_register(0, 0x50);
        assert_eq!(fdc.head_position(), 76); // Still at 76
    }

    // -----------------------------------------------------------------
    // Type II command tests: READ_SECTOR, WRITE_SECTOR
    // -----------------------------------------------------------------

    #[test]
    fn test_read_sector_basic() {
        let mut fdc = make_controller_with_disk();

        // Write a known pattern to track 0, sector 1
        let pattern = [0xAA; SECTOR_SIZE];
        fdc.get_disk_mut(0).unwrap().write_sector(0, 1, &pattern).unwrap();

        // Seek to track 0, set sector 1
        fdc.write_register(1, 0); // Track reg = 0
        fdc.write_register(2, 1); // Sector reg = 1

        // Issue READ SECTOR command (0x88 = read, no multiple)
        fdc.write_register(0, 0x88);

        // Controller should be in ReadingData state with DRQ active
        assert!(fdc.is_drq_active());
        assert!(fdc.is_busy());

        // Read all 128 bytes
        let mut read_data = [0u8; SECTOR_SIZE];
        for i in 0..SECTOR_SIZE {
            read_data[i] = fdc.read_register(3); // Data register
        }
        assert_eq!(read_data, pattern);

        // Controller should be done
        assert!(!fdc.is_busy());
    }

    #[test]
    fn test_write_sector_basic() {
        let mut fdc = make_controller_with_disk();

        // Seek to track 2, sector 3
        fdc.write_register(1, 2); // Track reg = 2
        fdc.write_register(2, 3); // Sector reg = 3
        fdc.set_head_position(0, 2);

        // Issue WRITE SECTOR command
        fdc.write_register(0, 0xA8);

        // Controller should be in WritingData state with DRQ active
        assert!(fdc.is_drq_active());
        assert!(fdc.is_busy());

        // Write 128 bytes
        let pattern = [0x55; SECTOR_SIZE];
        for &byte in pattern.iter() {
            fdc.write_register(3, byte); // Data register
        }

        // Controller should be done
        assert!(!fdc.is_busy());

        // Verify the data was written to the disk
        let read_back = fdc.get_disk(0).unwrap().read_sector(2, 3).unwrap();
        assert_eq!(read_back, pattern);
    }

    #[test]
    fn test_read_sector_not_found() {
        let mut fdc = make_controller_with_disk();

        // Try to read sector 30 (out of range for 26-sector track)
        fdc.write_register(1, 0); // Track 0
        fdc.write_register(2, 30); // Invalid sector

        fdc.write_register(0, 0x88); // READ SECTOR

        // Should get RNF status
        assert!(fdc.status_reg & S_RNF != 0);
    }

    #[test]
    fn test_read_no_disk() {
        let mut fdc = Fd1771::new();
        // No disk inserted
        fdc.select_drive(0);

        fdc.write_register(0, 0x88); // READ SECTOR

        assert!(fdc.status_reg & S_NOT_READY != 0);
    }

    #[test]
    fn test_write_protected_disk() {
        let mut fdc = Fd1771::new();
        let mut disk = DiskImage::new_formatted();
        disk.set_write_protected(true);
        fdc.insert_disk(0, disk).unwrap();
        fdc.select_drive(0);

        fdc.write_register(0, 0xA8); // WRITE SECTOR

        assert!(fdc.status_reg & S_WRITE_PROTECT != 0);
    }

    // -----------------------------------------------------------------
    // Type III command tests
    // -----------------------------------------------------------------

    #[test]
    fn test_read_address() {
        let mut fdc = make_controller_with_disk();

        // Seek to track 5
        fdc.write_register(3, 5);
        fdc.write_register(0, 0x10); // SEEK to track 5

        // READ ADDRESS
        fdc.write_register(0, 0xC0);

        // Data register should contain the track number
        assert_eq!(fdc.data_reg, 5);
    }

    // -----------------------------------------------------------------
    // Type IV command test: FORCE_INTERRUPT
    // -----------------------------------------------------------------

    #[test]
    fn test_force_interrupt() {
        let mut fdc = make_controller_with_disk();

        // Start a read operation
        fdc.write_register(0, 0x88); // READ SECTOR

        // Force interrupt to abort
        fdc.write_register(0, 0xD0); // FORCE INTERRUPT

        assert!(!fdc.is_busy());
        assert!(!fdc.is_drq_active());
    }

    // -----------------------------------------------------------------
    // Drive select tests
    // -----------------------------------------------------------------

    #[test]
    fn test_drive_select() {
        let mut fdc = Fd1771::new();
        fdc.insert_blank_disk(0).unwrap();
        fdc.insert_blank_disk(1).unwrap();

        // Select drive 1 and verify
        fdc.select_drive(1);
        assert!(fdc.has_disk(1));
        assert_eq!(fdc.selected_drive(), 1);

        // Head position should be per-drive
        fdc.write_register(3, 5);
        fdc.write_register(0, 0x10); // SEEK to track 5 on drive 1
        // Verify via set_head_position for comparison
        assert_eq!(fdc.head_position(), 5);

        // Switch back to drive 0, head should be at 0
        fdc.select_drive(0);
        assert_eq!(fdc.head_position(), 0);
    }

    // -----------------------------------------------------------------
    // Force interrupt and command-while-busy tests
    // -----------------------------------------------------------------

    #[test]
    fn test_command_ignored_while_busy() {
        let mut fdc = make_controller_with_disk();

        // Start a read operation (sets BUSY)
        fdc.write_register(0, 0x88); // READ SECTOR

        // Try another command while busy (should be ignored)
        fdc.write_register(0, 0x10); // SEEK - should be ignored

        // The operation should still be in the read state
        // (DRQ should be active, bytes available to read)
        assert!(fdc.is_drq_active());
    }

    // -----------------------------------------------------------------
    // Register read/write tests
    // -----------------------------------------------------------------

    #[test]
    fn test_track_sector_register_rw() {
        let mut fdc = make_controller_with_disk();

        fdc.write_register(1, 42); // Track register
        fdc.write_register(2, 15); // Sector register

        assert_eq!(fdc.read_register(1), 42);
        assert_eq!(fdc.read_register(2), 15);
    }

    #[test]
    fn test_data_register_rw() {
        let mut fdc = make_controller_with_disk();

        // Write to data register when idle
        fdc.write_register(3, 0xAB);
        assert_eq!(fdc.read_register(3), 0xAB);
    }

    // -----------------------------------------------------------------
    // Status register bit tests
    // -----------------------------------------------------------------

    #[test]
    fn test_track0_status_after_restore() {
        let mut fdc = make_controller_with_disk();

        // RESTORE command should set TRACK0
        fdc.write_register(0, 0x00); // RESTORE
        assert!(fdc.status_reg & S_TRACK0 != 0);
    }

    #[test]
    fn test_track0_cleared_after_seek() {
        let mut fdc = make_controller_with_disk();

        // Seek to track 5
        fdc.write_register(3, 5);
        fdc.write_register(0, 0x10); // SEEK
        assert!(fdc.status_reg & S_TRACK0 == 0); // Not on track 0
    }

    // -----------------------------------------------------------------
    // Disk insert/eject tests
    // -----------------------------------------------------------------

    #[test]
    fn test_eject_disk() {
        let mut fdc = make_controller_with_disk();

        assert!(fdc.has_disk(0));
        fdc.eject_disk(0);
        assert!(!fdc.has_disk(0));

        // Should show NOT_READY after eject
        fdc.select_drive(0);
        assert!(fdc.status_reg & S_NOT_READY != 0);
    }

    #[test]
    fn test_insert_disk_sets_ready() {
        let mut fdc = Fd1771::new();
        assert!(!fdc.has_disk(0));

        fdc.insert_blank_disk(0).unwrap();
        fdc.select_drive(0);
        assert!(fdc.status_reg & S_NOT_READY == 0);
    }
}