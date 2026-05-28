//! CP/M 2.2 BIOS and boot loader for the IMSAI 8080 with Tarbell controller
//!
//! This module provides everything needed to boot and run CP/M 2.2:
//! - A boot ROM that loads system tracks from the Tarbell disk controller
//! - A full 17-entry CP/M 2.2 BIOS in 8080 machine code
//! - Disk Parameter Block and sector skew table
//!
//! Memory layout after boot:
//!
//! | Address   | Contents                                    |
//! |-----------|---------------------------------------------|
//! | 0x0000    | JMP to BOOT (cold start vector)             |
//! | 0x0003    | IOBYTE                                       |
//! | 0x0005    | JMP to BDOS                                  |
//! | 0x0100    | CCP (Command Control Program)               |
//! | ~0x0B00   | BDOS                                         |
//! | ~0x1400   | BIOS                                         |
//!
//! The BIOS jump table and routines are installed at BIOS_BASE (0x1F00)
//! after the CP/M system image is loaded from disk. The installed BIOS
//! uses the Tarbell controller (ports 0x48-0x4B) and console (ports
//! 0x00-0x01).

#![allow(dead_code)]

use crate::bus::ImsaiBus;
use crate::dpb::DiskParameterBlock;

/// Tarbell controller I/O ports
const TARBELL_CMD_STATUS: u8 = 0x48;
const TARBELL_TRACK: u8 = 0x49;
const TARBELL_SECTOR: u8 = 0x4A;
const TARBELL_DATA: u8 = 0x4B;

/// Console I/O ports
const PORT_CONSOLE_DATA: u8 = 0x00;
const PORT_CONSOLE_STATUS: u8 = 0x01;

/// FD1771 command codes
const FD_RESTORE: u8 = 0x00;
const FD_READ_SECTOR: u8 = 0x80;
const FD_WRITE_SECTOR: u8 = 0xA0;

/// FD1771 status bits
const FD_NOT_READY: u8 = 0x80;
const FD_BUSY: u8 = 0x01;
const FD_ERROR: u8 = 0x02;

/// Address where the BIOS jump table starts
const BIOS_BASE: u16 = 0x1F00;

/// Address where the DPB is stored
const DPB_ADDRESS: u16 = 0x1700;

/// Skew table address (right after DPB)
const SKEW_ADDRESS: u16 = DPB_ADDRESS + 15;

/// DMA storage address (2 bytes: low, high)
const DMA_STORAGE: u16 = DPB_ADDRESS + 20;

/// Track storage address
const TRACK_STORAGE: u16 = DPB_ADDRESS + 22;

/// Sector storage address
const SECTOR_STORAGE: u16 = DPB_ADDRESS + 23;

/// Disk drive storage address
const DISK_STORAGE: u16 = DPB_ADDRESS + 24;

/// Number of CP/M 2.2 BIOS entry points
const NUM_CPM22_BIOS_ENTRIES: usize = 17;

/// BIOS function indices
const BIOS_BOOT: usize = 0;
const BIOS_WBOOT: usize = 1;
const BIOS_CONST: usize = 2;
const BIOS_CONIN: usize = 3;
const BIOS_CONOUT: usize = 4;
const BIOS_LIST: usize = 5;
const BIOS_PUNCH: usize = 6;
const BIOS_READER: usize = 7;
const BIOS_HOME: usize = 8;
const BIOS_SELDSK: usize = 9;
const BIOS_SETTRK: usize = 10;
const BIOS_SETSEC: usize = 11;
const BIOS_SETDMA: usize = 12;
const BIOS_READ: usize = 13;
const BIOS_WRITE: usize = 14;
const BIOS_LISTST: usize = 15;
const BIOS_SECTRAN: usize = 16;

/// Helper to write a sequence of bytes to memory
fn write_bytes(bus: &mut ImsaiBus, addr: u16, bytes: &[u8]) {
    for (i, &b) in bytes.iter().enumerate() {
        bus.memory.write(addr + i as u16, b);
    }
}

/// CP/M 2.2 BIOS implementation
pub struct CpmBios;

impl CpmBios {
    /// Install the CP/M 2.2 BIOS into memory. This patches the loaded
    /// CP/M system image with an emulator-compatible BIOS.
    pub fn install(bus: &mut ImsaiBus) {
        // Warm boot vector at 0x0000: JMP to BOOT entry in BIOS jump table
        write_bytes(bus, 0x0000, &[0xC3, BIOS_BASE as u8, (BIOS_BASE >> 8) as u8]);

        // IOBYTE at 0x0003 (console only)
        bus.memory.write(0x0003, 0x00);

        // Default drive at 0x0004 (drive A)
        bus.memory.write(0x0004, 0x00);

        // BDOS entry at 0x0005: preserve if already set by loaded system
        if bus.memory.read(0x0005) != 0xC3 {
            write_bytes(bus, 0x0005, &[0xC3, 0x00, 0x00]);
        }

        // BIOS jump table: 17 entries, each 3 bytes (JMP addr)
        for i in 0..NUM_CPM22_BIOS_ENTRIES {
            let jump_addr = BIOS_BASE + (i as u16) * 3;
            // Routine address = start of routine area after jump table
            // Jump table = 17 * 3 = 51 bytes, routines start after
            let routine_base = BIOS_BASE + (NUM_CPM22_BIOS_ENTRIES as u16) * 3;
            let routine_addr = routine_base + Self::routine_offset(i);

            write_bytes(bus, jump_addr, &[0xC3, routine_addr as u8, (routine_addr >> 8) as u8]);
        }

        // Write BIOS routines
        let routine_base = BIOS_BASE + (NUM_CPM22_BIOS_ENTRIES as u16) * 3;
        for i in 0..NUM_CPM22_BIOS_ENTRIES {
            let addr = routine_base + Self::routine_offset(i);
            match i {
                BIOS_BOOT     => Self::write_boot(bus, addr),
                BIOS_WBOOT    => Self::write_wboot(bus, addr),
                BIOS_CONST    => Self::write_const(bus, addr),
                BIOS_CONIN    => Self::write_conin(bus, addr),
                BIOS_CONOUT   => Self::write_conout(bus, addr),
                BIOS_LIST     => Self::write_list(bus, addr),
                BIOS_PUNCH    => Self::write_punch(bus, addr),
                BIOS_READER   => Self::write_reader(bus, addr),
                BIOS_HOME     => Self::write_home(bus, addr),
                BIOS_SELDSK   => Self::write_seldsk(bus, addr),
                BIOS_SETTRK   => Self::write_settrk(bus, addr),
                BIOS_SETSEC   => Self::write_setsec(bus, addr),
                BIOS_SETDMA   => Self::write_setdma(bus, addr),
                BIOS_READ     => Self::write_read(bus, addr),
                BIOS_WRITE    => Self::write_write(bus, addr),
                BIOS_LISTST   => Self::write_listst(bus, addr),
                BIOS_SECTRAN  => Self::write_sectrn(bus, addr),
                _ => {}
            }
        }

        // Write the DPB and skew table
        Self::write_dpb(bus);
        Self::write_skew_table(bus);

        // Initialize scratch area
        for addr in DMA_STORAGE..=DISK_STORAGE + 1 {
            bus.memory.write(addr, 0x00);
        }
        // Default DMA address = 0x0080
        bus.memory.write(DMA_STORAGE, 0x80);
        bus.memory.write(DMA_STORAGE + 1, 0x00);

        // Default track = 0, sector = 1
        bus.memory.write(TRACK_STORAGE, 0x00);
        bus.memory.write(SECTOR_STORAGE, 0x01);
        bus.memory.write(DISK_STORAGE, 0x00);
    }

    /// Calculate the byte offset for each BIOS routine within the routine area.
    fn routine_offset(index: usize) -> u16 {
        match index {
            0  => 0,     // BOOT: 3 bytes
            1  => 16,    // WBOOT: ~50 bytes
            2  => 80,    // CONST: 13 bytes
            3  => 112,   // CONIN: 12 bytes
            4  => 160,   // CONOUT: 11 bytes
            5  => 192,   // LIST: 1 byte
            6  => 208,   // PUNCH: 11 bytes
            7  => 240,   // READER: 3 bytes
            8  => 256,   // HOME: 13 bytes
            9  => 272,   // SELDSK: 14 bytes
            10 => 304,   // SETTRK: 7 bytes
            11 => 320,   // SETSEC: 7 bytes
            12 => 336,   // SETDMA: 7 bytes
            13 => 352,   // READ: ~28 bytes
            14 => 400,   // WRITE: ~28 bytes
            15 => 448,   // LISTST: 3 bytes
            16 => 464,   // SECTRAN: 14 bytes
            _ => 0,
        }
    }

    // ---- BIOS routines (8080 machine code) ----

    /// BOOT: Cold start - jump to CCP at 0x0100, but first signal BIOS re-install.
    fn write_boot(bus: &mut ImsaiBus, addr: u16) {
        // OUT 0xFE, A (trigger BIOS re-install - ensures our BIOS is in place)
        write_bytes(bus, addr, &[0xD3, 0xFE]); // OUT 0xFE
        // JMP 0x0100 (CCP start)
        write_bytes(bus, addr + 2, &[0xC3, 0x00, 0x01]);
    }

    /// WBOOT: Warm start - reload system from disk and jump to 0x0000
    fn write_wboot(bus: &mut ImsaiBus, start: u16) {
        let mut a = start;

        // Save registers
        write_bytes(bus, a, &[0xF5]); a += 1; // PUSH PSW
        write_bytes(bus, a, &[0xC5]); a += 1; // PUSH B
        write_bytes(bus, a, &[0xD5]); a += 1; // PUSH D
        write_bytes(bus, a, &[0xE5]); a += 1; // PUSH H

        // RESTORE command to Tarbell controller (seek track 0)
        write_bytes(bus, a, &[0x3E, 0x00]); a += 2; // MVI A,0 (RESTORE)
        write_bytes(bus, a, &[0xD3, TARBELL_CMD_STATUS]); a += 2; // OUT 0x48

        // Wait for RESTORE to complete
        let wait_restore = a;
        write_bytes(bus, a, &[0xDB, TARBELL_CMD_STATUS]); a += 2; // IN 0x48
        write_bytes(bus, a, &[0xE6, FD_BUSY]); a += 2; // ANI BUSY
        write_bytes(bus, a, &[0xC2]); a += 1; // JNZ wait_restore
        write_bytes(bus, a, &[wait_restore as u8, (wait_restore >> 8) as u8]); a += 2;

        // LXI H, 0x0000 (destination address in memory)
        write_bytes(bus, a, &[0x21, 0x00, 0x00]); a += 3;

        // MVI B, 0 (track counter, OFF=3 tracks to read)
        write_bytes(bus, a, &[0x06, 0x00]); a += 2;

        // Track loop: set track on controller
        let track_start = a;
        write_bytes(bus, a, &[0x78]); a += 1; // MOV A,B
        write_bytes(bus, a, &[0xD3, TARBELL_TRACK]); a += 2; // OUT 0x49
        // MVI C, 1 (physical sector counter, 1-26)
        write_bytes(bus, a, &[0x0E, 0x01]); a += 2;

        // Sector loop
        let sector_start = a;
        // Set sector on controller
        write_bytes(bus, a, &[0x79]); a += 1; // MOV A,C
        write_bytes(bus, a, &[0xD3, TARBELL_SECTOR]); a += 2; // OUT 0x4A

        // Issue READ command
        write_bytes(bus, a, &[0x3E, FD_READ_SECTOR]); a += 2; // MVI A,0x80
        write_bytes(bus, a, &[0xD3, TARBELL_CMD_STATUS]); a += 2; // OUT 0x48

        // Wait for READ to complete (controller clears BUSY when data ready)
        let wait_read = a;
        write_bytes(bus, a, &[0xDB, TARBELL_CMD_STATUS]); a += 2; // IN 0x48
        write_bytes(bus, a, &[0xE6, FD_BUSY]); a += 2; // ANI BUSY
        write_bytes(bus, a, &[0xC2]); a += 1; // JNZ wait_read
        write_bytes(bus, a, &[wait_read as u8, (wait_read >> 8) as u8]); a += 2;

        // Read 128 bytes from data port into (HL)
        write_bytes(bus, a, &[0x16, 128]); a += 2; // MVI D,128
        let byte_loop = a;
        write_bytes(bus, a, &[0xDB, TARBELL_DATA]); a += 2; // IN 0x4B
        write_bytes(bus, a, &[0x77]); a += 1; // MOV M,A
        write_bytes(bus, a, &[0x23]); a += 1; // INX H
        write_bytes(bus, a, &[0x15]); a += 1; // DCR D
        write_bytes(bus, a, &[0xC2]); a += 1; // JNZ byte_loop
        write_bytes(bus, a, &[byte_loop as u8, (byte_loop >> 8) as u8]); a += 2;

        // Next sector: INR C; MOV A,C; CPI 27
        write_bytes(bus, a, &[0x0C]); a += 1; // INR C
        write_bytes(bus, a, &[0x79]); a += 1; // MOV A,C
        write_bytes(bus, a, &[0xFE, 27]); a += 2; // CPI 27
        write_bytes(bus, a, &[0xC2]); a += 1; // JNZ sector_start
        write_bytes(bus, a, &[sector_start as u8, (sector_start >> 8) as u8]); a += 2;

        // Next track: INR B; MOV A,B; CPI 3 (OFF=3)
        write_bytes(bus, a, &[0x04]); a += 1; // INR B
        write_bytes(bus, a, &[0x78]); a += 1; // MOV A,B
        write_bytes(bus, a, &[0xFE, 0x03]); a += 2; // CPI 3
        write_bytes(bus, a, &[0xC2]); a += 1; // JNZ track_start
        write_bytes(bus, a, &[track_start as u8, (track_start >> 8) as u8]); a += 2;

        // RESTORE (seek track 0) before returning
        write_bytes(bus, a, &[0x3E, 0x00]); a += 2; // MVI A,0
        write_bytes(bus, a, &[0xD3, TARBELL_CMD_STATUS]); a += 2; // OUT 0x48

        // *** Re-install BIOS after disk reload ***
        // The system tracks we just loaded contain a CMI5619 BIOS
        // that doesn't work with our emulator. Signal the emulator to
        // re-install our emulator-compatible BIOS by writing to port 0xFE.
        write_bytes(bus, a, &[0xD3, 0xFE]); a += 2; // OUT 0xFE (BIOS re-install)

        // Re-patch warm boot vector to our WBOOT
        // (The loaded system may have overwritten 0x0000)
        write_bytes(bus, a, &[0x3E, 0xC3]); a += 2; // MVI A,0xC3 (JMP opcode)
        write_bytes(bus, a, &[0x32, 0x00, 0x00]); a += 3; // STA 0x0000
        write_bytes(bus, a, &[0x21]); a += 1; // LXI H,WBOOT_addr
        write_bytes(bus, a, &[(start.wrapping_add(3)) as u8]); a += 1;
        write_bytes(bus, a, &[((start.wrapping_add(3)) >> 8) as u8]); a += 1;
        write_bytes(bus, a, &[0x22, 0x01, 0x00]); a += 3; // SHLD 0x0001

        // Pop registers
        write_bytes(bus, a, &[0xE1]); a += 1; // POP H
        write_bytes(bus, a, &[0xD1]); a += 1; // POP D
        write_bytes(bus, a, &[0xC1]); a += 1; // POP B
        write_bytes(bus, a, &[0xF1]); a += 1; // POP PSW

        // JMP 0x0100 (CCP start)
        write_bytes(bus, a, &[0xC3, 0x00, 0x01]); a += 3;
    }

    /// CONST: Check console status. Return 0xFF if key ready, 0x00 if not.
    fn write_const(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[
            0xDB, PORT_CONSOLE_STATUS, // IN PORT_CONSOLE_STATUS
            0xE6, 0x01,                 // ANI 0x01
            0xC2, (addr + 10) as u8, ((addr + 10) >> 8) as u8, // JNZ ready
            0xAF,                       // XRA A (A=0, not ready)
            0xC9,                       // RET
            0x3E, 0xFF,                 // MVI A,0xFF (ready)
            0xC9,                       // RET
        ]);
    }

    /// CONIN: Read console character with wait loop. Strip parity.
    fn write_conin(bus: &mut ImsaiBus, addr: u16) {
        // Wait for character: loop reading status until key ready
        write_bytes(bus, addr, &[
            0xDB, PORT_CONSOLE_STATUS, // IN PORT_CONSOLE_STATUS
            0xE6, 0x01,                 // ANI 0x01 (key ready bit)
            0xCA, addr as u8, (addr >> 8) as u8, // JZ wait
            0xDB, PORT_CONSOLE_DATA,     // IN PORT_CONSOLE_DATA
            0xE6, 0x7F,                 // ANI 0x7F (strip parity)
            0xC9,                       // RET
        ]);
    }

    /// CONOUT: Write character in register C to console, wait for ready.
    fn write_conout(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[
            0xDB, PORT_CONSOLE_STATUS, // IN PORT_CONSOLE_STATUS
            0xE6, 0x02,                 // ANI 0x02 (display ready bit)
            0xCA, addr as u8, (addr >> 8) as u8, // JZ wait
            0x79,                       // MOV A,C
            0xD3, PORT_CONSOLE_DATA,     // OUT PORT_CONSOLE_DATA
            0xC9,                       // RET
        ]);
    }

    /// LIST: No printer, just RET.
    fn write_list(bus: &mut ImsaiBus, addr: u16) {
        bus.memory.write(addr, 0xC9); // RET
    }

    /// PUNCH: Same as CONOUT.
    fn write_punch(bus: &mut ImsaiBus, addr: u16) {
        Self::write_conout(bus, addr);
    }

    /// READER: Return CTRL-Z (0x1A) for EOF.
    fn write_reader(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[0x3E, 0x1A, 0xC9]); // MVI A,0x1A; RET
    }

    /// HOME: Seek to track 0.
    fn write_home(bus: &mut ImsaiBus, addr: u16) {
        let wait = addr + 8;
        write_bytes(bus, addr, &[
            0x3E, FD_RESTORE,           // MVI A,FD_RESTORE
            0xD3, TARBELL_CMD_STATUS,    // OUT CMD
            0xDB, TARBELL_CMD_STATUS,    // IN CMD (wait loop)
            0xE6, FD_BUSY,               // ANI BUSY
            0xC2, wait as u8, (wait >> 8) as u8, // JNZ wait
            0xAF,                       // XRA A (success)
            0xC9,                       // RET
        ]);
    }

    /// SELDSK: Select disk. C = drive number. Return HL = DPB addr or 0.
    fn write_seldsk(bus: &mut ImsaiBus, addr: u16) {
        let _valid = addr + 13;
        let invalid = addr + 16;
        write_bytes(bus, addr, &[
            0x79,                       // MOV A,C
            0x32, DISK_STORAGE as u8, (DISK_STORAGE >> 8) as u8, // STA DISK_STORAGE
            0xFE, 0x04,                 // CPI 4 (max drives)
            0xD2, invalid as u8, (invalid >> 8) as u8, // JNC invalid
            0x21, DPB_ADDRESS as u8, (DPB_ADDRESS >> 8) as u8, // LXI H,DPB_ADDRESS
            0xC9,                       // RET (valid)
            0x21, 0x00, 0x00,           // LXI H,0
            0xC9,                       // RET (invalid)
        ]);
    }

    /// SETTRK: Set track. C = track number.
    fn write_settrk(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[
            0x79,                       // MOV A,C
            0x32, TRACK_STORAGE as u8, (TRACK_STORAGE >> 8) as u8, // STA TRACK_STORAGE
            0xD3, TARBELL_TRACK,         // OUT TRACK
            0xC9,                       // RET
        ]);
    }

    /// SETSEC: Set sector. C = sector number.
    fn write_setsec(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[
            0x79,                       // MOV A,C
            0x32, SECTOR_STORAGE as u8, (SECTOR_STORAGE >> 8) as u8, // STA SECTOR_STORAGE
            0xD3, TARBELL_SECTOR,        // OUT SECTOR
            0xC9,                       // RET
        ]);
    }

    /// SETDMA: Set DMA address. B = high, C = low.
    fn write_setdma(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[
            0x21, DMA_STORAGE as u8, (DMA_STORAGE >> 8) as u8, // LXI H,DMA_STORAGE
            0x71,                       // MOV M,C (store low byte)
            0x23,                       // INX H
            0x70,                       // MOV M,B (store high byte)
            0xC9,                       // RET
        ]);
    }

    /// READ: Read sector into DMA address. Return A=0 for success, A=1 for error.
    fn write_read(bus: &mut ImsaiBus, addr: u16) {
        // Issue READ command
        write_bytes(bus, addr, &[
            0x3E, FD_READ_SECTOR,       // MVI A,FD_READ_SECTOR
            0xD3, TARBELL_CMD_STATUS,    // OUT CMD
        ]);
        // Wait for completion
        let wait = addr + 7;
        write_bytes(bus, addr + 4, &[
            0xDB, TARBELL_CMD_STATUS,    // IN CMD
            0xE6, FD_BUSY,               // ANI BUSY
            0xC2, wait as u8, (wait >> 8) as u8, // JNZ wait
        ]);
        // Check error
        let _ok_addr = addr + 25;
        let err_addr = addr + 28;
        write_bytes(bus, addr + 10, &[
            0xDB, TARBELL_CMD_STATUS,    // IN CMD
            0xE6, FD_ERROR,              // ANI ERROR
            0xC2, err_addr as u8, (err_addr >> 8) as u8, // JNZ error
            // Load DMA address
            0x2A, DMA_STORAGE as u8, (DMA_STORAGE >> 8) as u8, // LHLD DMA_STORAGE
            // Read 128 bytes
            0x16, 128,                   // MVI D,128
        ]);
        let byte_loop = addr + 21;
        write_bytes(bus, addr + 18, &[
            0xDB, TARBELL_DATA,          // IN DATA
            0x77,                       // MOV M,A
            0x23,                       // INX H
            0x15,                       // DCR D
            0xC2, byte_loop as u8, (byte_loop >> 8) as u8, // JNZ byte_loop
            // Success
            0xAF,                       // XRA A (A=0)
            0xC9,                       // RET
            // Error
            0x3E, 0x01,                 // MVI A,1
            0xC9,                       // RET
        ]);
    }

    /// WRITE: Write sector from DMA address. Return A=0=ok, A=1=error, A=2=wp.
    fn write_write(bus: &mut ImsaiBus, addr: u16) {
        // Issue WRITE command
        write_bytes(bus, addr, &[
            0x3E, FD_WRITE_SECTOR,      // MVI A,FD_WRITE_SECTOR
            0xD3, TARBELL_CMD_STATUS,    // OUT CMD
            // Load DMA address
            0x2A, DMA_STORAGE as u8, (DMA_STORAGE >> 8) as u8, // LHLD DMA_STORAGE
            // Write 128 bytes
            0x16, 128,                   // MVI D,128
        ]);
        let byte_loop = addr + 12;
        write_bytes(bus, addr + 10, &[
            0x7E,                       // MOV A,M
            0xD3, TARBELL_DATA,          // OUT DATA
            0x23,                       // INX H
            0x15,                       // DCR D
            0xC2, byte_loop as u8, (byte_loop >> 8) as u8, // JNZ byte_loop
        ]);
        // Wait for write completion
        let wait = addr + 22;
        write_bytes(bus, addr + 18, &[
            0xDB, TARBELL_CMD_STATUS,    // IN CMD
            0xE6, FD_BUSY,               // ANI BUSY
            0xC2, wait as u8, (wait >> 8) as u8, // JNZ wait
        ]);
        // Check error
        let err_addr = addr + 34;
        write_bytes(bus, addr + 24, &[
            0xDB, TARBELL_CMD_STATUS,    // IN CMD
            0xE6, FD_ERROR,              // ANI ERROR
            0xC2, err_addr as u8, (err_addr >> 8) as u8, // JNZ error
            0xAF,                       // XRA A (success)
            0xC9,                       // RET
            0x3E, 0x01,                 // MVI A,1 (error)
            0xC9,                       // RET
        ]);
    }

    /// LISTST: Always ready (0xFF).
    fn write_listst(bus: &mut ImsaiBus, addr: u16) {
        write_bytes(bus, addr, &[0x3E, 0xFF, 0xC9]); // MVI A,0xFF; RET
    }

    /// SECTRAN: Sector translate. BC=logical, DE=table. Return HL=physical.
    fn write_sectrn(bus: &mut ImsaiBus, addr: u16) {
        let no_translate = addr + 13;
        write_bytes(bus, addr, &[
            0x69,                       // MOV L,C
            0x60,                       // MOV H,B
            0x7A,                       // MOV A,D
            0xB3,                       // ORA E
            0xCA, no_translate as u8, (no_translate >> 8) as u8, // JZ no_translate
            0x09,                       // DAD B (HL = DE + BC)
            0x7E,                       // MOV A,M
            0x6F,                       // MOV L,A
            0x26, 0x00,                 // MVI H,0
            0xC9,                       // RET
            0xC9,                       // RET (no translation, HL=BC)
        ]);
    }

    /// Write the DPB into memory at DPB_ADDRESS
    fn write_dpb(bus: &mut ImsaiBus) {
        let dpb = DiskParameterBlock::tarbell_standard();
        let bytes = dpb.to_bytes();
        for (i, &byte) in bytes.iter().enumerate() {
            bus.memory.write(DPB_ADDRESS + i as u16, byte);
        }
    }

    /// Write the sector skew table after the DPB
    fn write_skew_table(bus: &mut ImsaiBus) {
        let skew_table: [u8; 26] = [
            1, 7, 13, 19, 25, 5, 11, 17, 23, 3, 9, 15, 21, 2, 8, 14, 20, 26, 6, 12, 18, 24,
            4, 10, 16, 22,
        ];
        for (i, &byte) in skew_table.iter().enumerate() {
            bus.memory.write(SKEW_ADDRESS + i as u16, byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ImsaiBus;

    #[test]
    fn test_cpm22_bios_install_jump_table() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        // Warm boot vector at 0x0000: JMP
        assert_eq!(bus.memory.read(0x0000), 0xC3);

        // IOBYTE at 0x0003
        assert_eq!(bus.memory.read(0x0003), 0x00);

        // BDOS entry at 0x0005: JMP
        assert_eq!(bus.memory.read(0x0005), 0xC3);

        // All 17 BIOS entries should be JMP instructions
        for i in 0..17 {
            let addr = BIOS_BASE + (i as u16) * 3;
            assert_eq!(
                bus.memory.read(addr),
                0xC3,
                "BIOS entry {} at {:#06x} should be JMP",
                i,
                addr
            );
        }
    }

    #[test]
    fn test_cpm22_bios_dpb_in_memory() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        // SPT at DPB_ADDRESS should be 26 (0x1A)
        assert_eq!(bus.memory.read(DPB_ADDRESS), 0x1A);
        assert_eq!(bus.memory.read(DPB_ADDRESS + 1), 0x00);

        // BSH should be 3
        assert_eq!(bus.memory.read(DPB_ADDRESS + 2), 3);

        // OFF should be 3
        assert_eq!(bus.memory.read(DPB_ADDRESS + 13), 0x03);
        assert_eq!(bus.memory.read(DPB_ADDRESS + 14), 0x00);

        // CKS should be 32 (0x20)
        assert_eq!(bus.memory.read(DPB_ADDRESS + 11), 0x20);
        assert_eq!(bus.memory.read(DPB_ADDRESS + 12), 0x00);
    }

    #[test]
    fn test_cpm22_bios_dma_default() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        // Default DMA should be 0x0080
        assert_eq!(bus.memory.read(DMA_STORAGE), 0x80);
        assert_eq!(bus.memory.read(DMA_STORAGE + 1), 0x00);
    }

    #[test]
    fn test_boot_jumps_to_ccp() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        let routine_base = BIOS_BASE + (NUM_CPM22_BIOS_ENTRIES as u16) * 3;
        let boot_addr = routine_base + CpmBios::routine_offset(BIOS_BOOT);

        // BOOT routine starts with OUT 0xFE (BIOS re-install trigger)
        assert_eq!(bus.memory.read(boot_addr), 0xD3); // OUT
        assert_eq!(bus.memory.read(boot_addr + 1), 0xFE); // port 0xFE
        // Then JMP 0x0100
        assert_eq!(bus.memory.read(boot_addr + 2), 0xC3); // JMP
        assert_eq!(bus.memory.read(boot_addr + 3), 0x00);
        assert_eq!(bus.memory.read(boot_addr + 4), 0x01);
    }
}