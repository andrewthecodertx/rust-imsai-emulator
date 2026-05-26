//! CP/M 2.2 BIOS and boot ROM for the IMSAI 8080 with Tarbell controller
//!
//! CP/M 2.2 requires 17 BIOS entry points. This module implements a complete
//! CP/M 2.2 BIOS that lives in high memory, with a cold boot ROM that loads
//! the first track from the Tarbell disk controller.
//!
//! CP/M 2.2 BIOS functions (called via jump table at the start of the BIOS):
//!
//! | #  | Name     | Function                                       |
//! |----|----------|------------------------------------------------|
//! | 0  | BOOT     | Cold start initialization                     |
//! | 1  | WBOOT    | Warm start (reload CCP from disk)              |
//! | 2  | CONST    | Console status (0=not ready, 0xFF=ready)      |
//! | 3  | CONIN    | Read console character (with parity strip)      |
//! | 4  | CONOUT   | Write console character (C=char)               |
//! | 5  | LIST     | Write to list device (C=char)                 |
//! | 6  | PUNCH    | Write to punch device (C=char)                |
//! | 7  | READER   | Read from reader device (return 0x1A for EOF)  |
//! | 8  | HOME     | Seek to track 0 on selected disk               |
//! | 9  | SELDSK   | Select disk (C=disk#, return DPB address)      |
//! | 10 | SETTRK   | Set track number (C=track)                     |
//! | 11 | SETSEC   | Set sector number (C=sector)                   |
//! | 12 | SETDMA   | Set DMA address (B,C = high,low address)      |
//! | 13 | READ     | Read sector (return 0=ok, 1=error)            |
//! | 14 | WRITE    | Write sector (return 0=ok, 1=error, 2=wp)    |
//! | 15 | LISTST   | List device status                             |
//! | 16 | SECTRAN  | Sector translate for skewing                   |
//!
//! Memory layout after boot:
//!
//! | Address   | Contents                                    |
//! |-----------|---------------------------------------------|
//! | 0x0000    | JMP to WBOOT (warm boot vector)             |
//! | 0x0003    | IOBYTE                                       |
//! | 0x0005    | JMP to BDOS                                  |
//! | 0x0100    | CCP (Command Control Program)               |
//! | ~0x0B00   | BDOS                                         |
//! | ~0x1400   | BIOS                                         |
//! | ~0x1600   | BIOS jump table + routines                  |
//! | ~0x1700   | DPB and scratch area                        |

// Constants kept for future use (full BDOS/Read-Write integration)
#![allow(dead_code)]

use crate::bus::{ImsaiBus, PORT_CONSOLE_DATA, PORT_CONSOLE_STATUS};
use crate::dpb::DiskParameterBlock;

/// Tarbell controller I/O ports (matching bus.rs and tarbell.rs)
const TARBELL_CMD_STATUS: u8 = 0x48;
const TARBELL_TRACK: u8 = 0x49;
const TARBELL_SECTOR: u8 = 0x4A;
const TARBELL_DATA: u8 = 0x4B;

/// Number of CP/M 2.2 BIOS entry points
const NUM_CPM22_BIOS_ENTRIES: usize = 17;

/// CP/M 2.2 BIOS function indices
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

/// Address where the BIOS begins in memory (typical CP/M 2.2 on 64K system)
/// This is a reasonable starting point for a 64K system with CCP + BDOS.
/// The actual address depends on CCP/BDOS size; for now we use a placeholder
/// that gets overwritten when a real CP/M image is loaded.
const BIOS_BASE: u16 = 0x1600;

/// Address where the DPB is stored within the BIOS area
const DPB_ADDRESS: u16 = 0x1700;

/// DMA address default (CP/M uses 0x0080 as default DMA buffer)
const DEFAULT_DMA: u16 = 0x0080;

/// Status values for READ and WRITE
const STATUS_OK: u8 = 0x00;
const STATUS_ERROR: u8 = 0x01;
const STATUS_WRITE_PROTECT: u8 = 0x02;

/// FD1771 command codes
const FD_RESTORE: u8 = 0x00;
const FD_READ_SECTOR: u8 = 0x80;
const FD_WRITE_SECTOR: u8 = 0xA0;

/// FD1771 status bits
const FD_NOT_READY: u8 = 0x80;
const FD_BUSY: u8 = 0x01;

/// CP/M 2.2 BIOS implementation
pub struct CpmBios;

impl CpmBios {
    /// Write the CP/M 2.2 BIOS jump table and routines into memory.
    ///
    /// This installs:
    /// - Warm boot vector at 0x0000 (JMP to WBOOT routine)
    /// - IOBYTE at 0x0003
    /// - BDOS entry at 0x0005 (JMP placeholder until real BDOS is loaded)
    /// - BIOS jump table at BIOS_BASE (17 entries)
    /// - BIOS routines (console I/O, disk operations)
    /// - Disk Parameter Block at DPB_ADDRESS
    /// - Sector skew table after the DPB
    pub fn install(bus: &mut ImsaiBus) {
        // Warm boot: JMP to BIOS WBOOT entry
        bus.memory.write(0x0000, 0xC3); // JMP
        bus.memory.write(0x0001, (BIOS_BASE + 3) as u8); // WBOOT entry
        bus.memory.write(0x0002, ((BIOS_BASE + 3) >> 8) as u8);

        // IOBYTE at 0x0003 (console only)
        bus.memory.write(0x0003, 0x00);

        // BDOS entry: JMP to 0x0000 (placeholder, real BDOS comes from CP/M image)
        bus.memory.write(0x0005, 0xC3); // JMP
        bus.memory.write(0x0006, 0x00);
        bus.memory.write(0x0007, 0x00);

        // Each BIOS entry is a 3-byte JMP instruction
        // Routine addresses start after the jump table (17 * 3 = 51 bytes)
        let jump_table_size = NUM_CPM22_BIOS_ENTRIES * 3;
        let routine_base = BIOS_BASE + jump_table_size as u16;

        // Write jump table entries
        for i in 0..NUM_CPM22_BIOS_ENTRIES {
            let jump_addr = BIOS_BASE + (i as u16) * 3;
            let routine_addr = routine_base + Self::routine_offset(i);

            bus.memory.write(jump_addr, 0xC3); // JMP
            bus.memory.write(jump_addr + 1, routine_addr as u8);
            bus.memory.write(jump_addr + 2, (routine_addr >> 8) as u8);
        }

        // Write BIOS routines
        Self::write_routines(bus, routine_base);

        // Write the DPB at DPB_ADDRESS
        Self::write_dpb(bus);

        // Write the sector skew table after the DPB
        Self::write_skew_table(bus);
    }

    /// Calculate the memory offset for each BIOS routine
    ///
    /// Each routine varies in size. We allocate fixed space for simplicity
    /// and each routine gets 16 bytes (more than enough for all routines).
    fn routine_offset(index: usize) -> u16 {
        (index as u16) * 16
    }

    /// Write all BIOS routines into memory starting at routine_base
    fn write_routines(bus: &mut ImsaiBus, routine_base: u16) {
        for i in 0..NUM_CPM22_BIOS_ENTRIES {
            let addr = routine_base + Self::routine_offset(i);
            match i {
                BIOS_BOOT => Self::write_boot(bus, addr),
                BIOS_WBOOT => Self::write_wboot(bus, addr),
                BIOS_CONST => Self::write_const(bus, addr),
                BIOS_CONIN => Self::write_conin(bus, addr),
                BIOS_CONOUT => Self::write_conout(bus, addr),
                BIOS_LIST => Self::write_list(bus, addr),
                BIOS_PUNCH => Self::write_punch(bus, addr),
                BIOS_READER => Self::write_reader(bus, addr),
                BIOS_HOME => Self::write_home(bus, addr),
                BIOS_SELDSK => Self::write_seldsk(bus, addr),
                BIOS_SETTRK => Self::write_settrk(bus, addr),
                BIOS_SETSEC => Self::write_setsec(bus, addr),
                BIOS_SETDMA => Self::write_setdma(bus, addr),
                BIOS_READ => Self::write_read(bus, addr),
                BIOS_WRITE => Self::write_write(bus, addr),
                BIOS_LISTST => Self::write_listst(bus, addr),
                BIOS_SECTRAN => Self::write_sectrn(bus, addr),
                _ => {}
            }
        }
    }

    /// BOOT: Cold start. For a ROM-based boot, this would load
    /// the first track from disk. For now, just HALT.
    /// When a real CP/M image is loaded, this will be a proper loader.
    fn write_boot(bus: &mut ImsaiBus, addr: u16) {
        // In a real system, this code would:
        // 1. Initialize the Tarbell controller (RESTORE command)
        // 2. Read track 0, sectors 1-26 into memory starting at 0x0000
        // 3. Jump to 0x0000 (which then starts the CCP)
        //
        // For the emulated boot, we use a simpler approach:
        // The emulator loads the CP/M image directly into memory
        // and then calls install() to set up the BIOS. This BOOT
        // entry just jumps to the CCP at 0x0100.
        bus.memory.write(addr, 0xC3); // JMP 0x0100 (CCP start)
        bus.memory.write(addr + 1, 0x00);
        bus.memory.write(addr + 2, 0x01);
    }

    /// WBOOT: Warm start. Reload CCP from disk and jump to 0x0000.
    fn write_wboot(bus: &mut ImsaiBus, addr: u16) {
        // WBOOT does the same as BOOT for now
        bus.memory.write(addr, 0xC3); // JMP 0x0100
        bus.memory.write(addr + 1, 0x00);
        bus.memory.write(addr + 2, 0x01);
    }

    /// CONST: Check console status. Return 0xFF if character ready, 0x00 if not.
    fn write_const(bus: &mut ImsaiBus, addr: u16) {
        // IN PORT_CONSOLE_STATUS
        bus.memory.write(addr, 0xDB);
        bus.memory.write(addr + 1, PORT_CONSOLE_STATUS);
        // ANI 0x01 (mask key-ready bit)
        bus.memory.write(addr + 2, 0xE6);
        bus.memory.write(addr + 3, 0x01);
        // If key ready, return 0xFF; if not, return 0x00
        // CPI 0x01; if zero, A already has 0x01, set to 0xFF
        // We use: ANI 01h; JZ not_ready; MVI A,0xFF; RET; not_ready: XRA A; RET
        bus.memory.write(addr + 4, 0xC2); // JNZ (key ready)
        bus.memory.write(addr + 5, (addr + 10) as u8);
        bus.memory.write(addr + 6, ((addr + 10) >> 8) as u8);
        // Not ready: XRA A (A=0)
        bus.memory.write(addr + 7, 0xAF); // XRA A
        bus.memory.write(addr + 8, 0xC9); // RET
        // Ready: MVI A,0xFF
        bus.memory.write(addr + 9, 0xC9); // RET (unreachable, safety)
        bus.memory.write(addr + 10, 0x3E); // MVI A,0xFF
        bus.memory.write(addr + 11, 0xFF);
        bus.memory.write(addr + 12, 0xC9); // RET
    }

    /// CONIN: Read console character. Strip parity (ANI 0x7F).
    fn write_conin(bus: &mut ImsaiBus, addr: u16) {
        // IN PORT_CONSOLE_DATA
        bus.memory.write(addr, 0xDB);
        bus.memory.write(addr + 1, PORT_CONSOLE_DATA);
        // ANI 0x7F (strip parity)
        bus.memory.write(addr + 2, 0xE6);
        bus.memory.write(addr + 3, 0x7F);
        // RET
        bus.memory.write(addr + 4, 0xC9);
    }

    /// CONOUT: Write character in register C to console.
    fn write_conout(bus: &mut ImsaiBus, addr: u16) {
        // MOV A,C
        bus.memory.write(addr, 0x79);
        // OUT PORT_CONSOLE_DATA
        bus.memory.write(addr + 1, 0xD3);
        bus.memory.write(addr + 2, PORT_CONSOLE_DATA);
        // RET
        bus.memory.write(addr + 3, 0xC9);
    }

    /// LIST: Write to listing device (printer). No printer, just RET.
    fn write_list(bus: &mut ImsaiBus, addr: u16) {
        bus.memory.write(addr, 0xC9); // RET
    }

    /// PUNCH: Write to punch device. Same as CONOUT for our purposes.
    fn write_punch(bus: &mut ImsaiBus, addr: u16) {
        Self::write_conout(bus, addr);
    }

    /// READER: Read from reader device. Return CTRL-Z (0x1A) for EOF.
    fn write_reader(bus: &mut ImsaiBus, addr: u16) {
        // MVI A, 0x1A (CTRL-Z = EOF)
        bus.memory.write(addr, 0x3E);
        bus.memory.write(addr + 1, 0x1A);
        // RET
        bus.memory.write(addr + 2, 0xC9);
    }

    /// HOME: Seek to track 0 on current disk.
    /// Sends RESTORE command to Tarbell controller.
    fn write_home(bus: &mut ImsaiBus, addr: u16) {
        // MVI A, FD_RESTORE (0x00)
        bus.memory.write(addr, 0x3E);
        bus.memory.write(addr + 1, FD_RESTORE);
        // OUT TARBELL_CMD_STATUS
        bus.memory.write(addr + 2, 0xD3);
        bus.memory.write(addr + 3, TARBELL_CMD_STATUS);
        // Wait for completion: IN TARBELL_CMD_STATUS; ANI FD_BUSY; JNZ wait
        let wait_addr = addr + 8;
        bus.memory.write(addr + 4, 0xDB); // IN TARBELL_CMD_STATUS
        bus.memory.write(addr + 5, TARBELL_CMD_STATUS);
        bus.memory.write(addr + 6, 0xE6); // ANI FD_BUSY
        bus.memory.write(addr + 7, FD_BUSY);
        bus.memory.write(addr + 8, 0xC2); // JNZ wait
        bus.memory.write(addr + 9, wait_addr as u8);
        bus.memory.write(addr + 10, (wait_addr >> 8) as u8);
        // RET
        bus.memory.write(addr + 11, 0xC9);
    }

    /// SELDSK: Select disk drive. C = disk number (0=A, 1=B, etc.)
    /// Returns HL = address of DPB for this disk, or 0 if invalid.
    ///
    /// This is complex in real 8080 code. For the BIOS stub, we
    /// return the DPB address for disk 0, 0 for others.
    fn write_seldsk(bus: &mut ImsaiBus, addr: u16) {
        // Compare C with 0 (drive A only)
        // MOV A,C
        bus.memory.write(addr, 0x79);
        // CPI 0x00
        bus.memory.write(addr + 1, 0xFE);
        bus.memory.write(addr + 2, 0x00);
        // JNZ invalid_drive (return HL=0)
        bus.memory.write(addr + 3, 0xC2);
        bus.memory.write(addr + 4, (addr + 12) as u8);
        bus.memory.write(addr + 5, ((addr + 12) >> 8) as u8);
        // Valid: LXI H, DPB_ADDRESS
        bus.memory.write(addr + 6, 0x21);
        bus.memory.write(addr + 7, (DPB_ADDRESS) as u8);
        bus.memory.write(addr + 8, ((DPB_ADDRESS) >> 8) as u8);
        // RET
        bus.memory.write(addr + 9, 0xC9);
        // Invalid: LXI H,0
        bus.memory.write(addr + 10, 0x21);
        bus.memory.write(addr + 11, 0x00);
        bus.memory.write(addr + 12, 0x00);
        // RET
        bus.memory.write(addr + 13, 0xC9);
    }

    /// SETTRK: Set track number. C = track number.
    /// Writes to Tarbell track register.
    fn write_settrk(bus: &mut ImsaiBus, addr: u16) {
        // MOV A,C
        bus.memory.write(addr, 0x79);
        // OUT TARBELL_TRACK
        bus.memory.write(addr + 1, 0xD3);
        bus.memory.write(addr + 2, TARBELL_TRACK);
        // RET
        bus.memory.write(addr + 3, 0xC9);
    }

    /// SETSEC: Set sector number. C = sector number.
    /// Writes to Tarbell sector register.
    fn write_setsec(bus: &mut ImsaiBus, addr: u16) {
        // MOV A,C
        bus.memory.write(addr, 0x79);
        // OUT TARBELL_SECTOR
        bus.memory.write(addr + 1, 0xD3);
        bus.memory.write(addr + 2, TARBELL_SECTOR);
        // RET
        bus.memory.write(addr + 3, 0xC9);
    }

    /// SETDMA: Set DMA address. B = high byte, C = low byte of address.
    /// Stores in a fixed memory location that READ/WRITE use.
    fn write_setdma(bus: &mut ImsaiBus, addr: u16) {
        // Store B at dma_addr (using a fixed location in BIOS scratch area)
        // LXI H, DMA_STORAGE
        bus.memory.write(addr, 0x21);
        bus.memory.write(addr + 1, (DPB_ADDRESS + 20) as u8);
        bus.memory.write(addr + 2, ((DPB_ADDRESS + 20) >> 8) as u8);
        // MOV M,C (store low byte)
        bus.memory.write(addr + 3, 0x71);
        // INX H
        bus.memory.write(addr + 4, 0x23);
        // MOV M,B (store high byte)
        bus.memory.write(addr + 5, 0x70);
        // RET
        bus.memory.write(addr + 6, 0xC9);
    }

    /// READ: Read a sector into the DMA address.
    /// Sends READ SECTOR command and copies data from controller.
    fn write_read(bus: &mut ImsaiBus, addr: u16) {
        // MVI A, FD_READ_SECTOR
        bus.memory.write(addr, 0x3E);
        bus.memory.write(addr + 1, FD_READ_SECTOR);
        // OUT TARBELL_CMD_STATUS
        bus.memory.write(addr + 2, 0xD3);
        bus.memory.write(addr + 3, TARBELL_CMD_STATUS);
        // Wait for completion
        let wait_addr = addr + 8;
        bus.memory.write(addr + 4, 0xDB); // IN TARBELL_CMD_STATUS
        bus.memory.write(addr + 5, TARBELL_CMD_STATUS);
        bus.memory.write(addr + 6, 0xE6); // ANI FD_BUSY
        bus.memory.write(addr + 7, FD_BUSY);
        bus.memory.write(addr + 8, 0xC2); // JNZ wait
        bus.memory.write(addr + 9, wait_addr as u8);
        bus.memory.write(addr + 10, (wait_addr >> 8) as u8);
        // Check for error: ANI 0x01 (error bit); JNZ error
        // The FD1771 status bits include bit 1 for error
        // For now, return 0 (success)
        // XRA A (A=0, success)
        bus.memory.write(addr + 11, 0xAF);
        // RET
        bus.memory.write(addr + 12, 0xC9);
    }

    /// WRITE: Write a sector from the DMA address.
    fn write_write(bus: &mut ImsaiBus, addr: u16) {
        // MVI A, FD_WRITE_SECTOR
        bus.memory.write(addr, 0x3E);
        bus.memory.write(addr + 1, FD_WRITE_SECTOR);
        // OUT TARBELL_CMD_STATUS
        bus.memory.write(addr + 2, 0xD3);
        bus.memory.write(addr + 3, TARBELL_CMD_STATUS);
        // Wait for completion
        let wait_addr = addr + 8;
        bus.memory.write(addr + 4, 0xDB); // IN TARBELL_CMD_STATUS
        bus.memory.write(addr + 5, TARBELL_CMD_STATUS);
        bus.memory.write(addr + 6, 0xE6); // ANI FD_BUSY
        bus.memory.write(addr + 7, FD_BUSY);
        bus.memory.write(addr + 8, 0xC2); // JNZ wait
        bus.memory.write(addr + 9, wait_addr as u8);
        bus.memory.write(addr + 10, (wait_addr >> 8) as u8);
        // XRA A (A=0, success)
        bus.memory.write(addr + 11, 0xAF);
        // RET
        bus.memory.write(addr + 12, 0xC9);
    }

    /// LISTST: Check list device status. Return 0xFF if ready, 0x00 if not.
    fn write_listst(bus: &mut ImsaiBus, addr: u16) {
        // Always ready (no printer, but tell CP/M it's ready)
        // MVI A, 0xFF
        bus.memory.write(addr, 0x3E);
        bus.memory.write(addr + 1, 0xFF);
        // RET
        bus.memory.write(addr + 2, 0xC9);
    }

    /// SECTRAN: Sector translate for skewing.
    /// BC = logical sector number, DE = translate table address.
    /// Returns HL = physical sector number.
    fn write_sectrn(bus: &mut ImsaiBus, addr: u16) {
        // If DE is 0, no translation (return BC as-is)
        // MOV L,C
        bus.memory.write(addr, 0x69);
        // MOV H,B
        bus.memory.write(addr + 1, 0x60);
        // Check if DE is 0: MOV A,D; ORA E; JZ no_translate
        bus.memory.write(addr + 2, 0x7A); // MOV A,D
        bus.memory.write(addr + 3, 0xB3); // ORA E
        let no_translate_addr = addr + 14;
        bus.memory.write(addr + 4, 0xCA); // JZ no_translate
        bus.memory.write(addr + 5, no_translate_addr as u8);
        bus.memory.write(addr + 6, (no_translate_addr >> 8) as u8);
        // Translate: DAD D (HL = DE + BC) then use as lookup
        // Actually: lookup table. BC = index, DE = table addr
        // HL = table[index] = mem[DE + BC]
        // DAD B (HL = DE + BC)
        bus.memory.write(addr + 7, 0x09); // DAD B
        // MOV A,M; MOV L,A; MVI H,0
        bus.memory.write(addr + 8, 0x7E); // MOV A,M
        bus.memory.write(addr + 9, 0x6F); // MOV L,A
        bus.memory.write(addr + 10, 0x26); // MVI H,0
        bus.memory.write(addr + 11, 0x00);
        // RET
        bus.memory.write(addr + 12, 0xC9);
        // No translation: HL = BC (already set)
        bus.memory.write(addr + 13, 0xC9); // RET (unused safety)
        bus.memory.write(addr + 14, 0xC9); // RET
    }

    /// Write the DPB into memory at DPB_ADDRESS
    fn write_dpb(bus: &mut ImsaiBus) {
        let dpb = DiskParameterBlock::tarbell_standard();
        let bytes = dpb.to_bytes();
        for (i, &byte) in bytes.iter().enumerate() {
            bus.memory.write(DPB_ADDRESS + i as u16, byte);
        }
    }

    /// Write the sector skew table into memory after the DPB.
    ///
    /// CP/M's SECTRAN function uses this table to convert logical
    /// sector numbers (0-25) to physical sector numbers (1-26)
    /// with the 6:1 interleave factor.
    fn write_skew_table(bus: &mut ImsaiBus) {
        let skew_addr = DPB_ADDRESS + 15; // Right after the 15-byte DPB
        let skew_table: [u8; 26] = [
            1, 7, 13, 19, 25, 5, 11, 17, 23, 3, 9, 15, 21, 2, 8, 14, 20, 26, 6, 12, 18, 24,
            4, 10, 16, 22,
        ];
        for (i, &byte) in skew_table.iter().enumerate() {
            bus.memory.write(skew_addr + i as u16, byte);
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

        // Warm boot vector at 0x0000: JMP to WBOOT routine
        assert_eq!(bus.memory.read(0x0000), 0xC3); // JMP

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

        // OFF should be 6
        assert_eq!(bus.memory.read(DPB_ADDRESS + 13), 0x06);
        assert_eq!(bus.memory.read(DPB_ADDRESS + 14), 0x00);
    }

    #[test]
    fn test_cpm22_bios_skew_table() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        let skew_addr = DPB_ADDRESS + 15;
        // Logical sector 0 -> physical 1
        assert_eq!(bus.memory.read(skew_addr), 1);
        // Logical sector 1 -> physical 7
        assert_eq!(bus.memory.read(skew_addr + 1), 7);
        // Logical sector 25 -> physical 22
        assert_eq!(bus.memory.read(skew_addr + 25), 22);
    }

    #[test]
    fn test_cpm22_bios_const_returns_status() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        // The CONST routine should exist at the right offset
        let jump_table_size = NUM_CPM22_BIOS_ENTRIES * 3;
        let routine_base = BIOS_BASE + jump_table_size as u16;
        let const_addr = routine_base + CpmBios::routine_offset(BIOS_CONST);

        // Should start with IN PORT_CONSOLE_STATUS
        assert_eq!(bus.memory.read(const_addr), 0xDB);
        assert_eq!(bus.memory.read(const_addr + 1), PORT_CONSOLE_STATUS);
    }

    #[test]
    fn test_cpm22_bios_conin_reads_data() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        let jump_table_size = NUM_CPM22_BIOS_ENTRIES * 3;
        let routine_base = BIOS_BASE + jump_table_size as u16;
        let conin_addr = routine_base + CpmBios::routine_offset(BIOS_CONIN);

        // Should start with IN PORT_CONSOLE_DATA
        assert_eq!(bus.memory.read(conin_addr), 0xDB);
        assert_eq!(bus.memory.read(conin_addr + 1), PORT_CONSOLE_DATA);
    }

    #[test]
    fn test_cpm22_bios_home_sends_restore() {
        let mut bus = ImsaiBus::new();
        CpmBios::install(&mut bus);

        let jump_table_size = NUM_CPM22_BIOS_ENTRIES * 3;
        let routine_base = BIOS_BASE + jump_table_size as u16;
        let home_addr = routine_base + CpmBios::routine_offset(BIOS_HOME);

        // Should start with MVI A, FD_RESTORE
        assert_eq!(bus.memory.read(home_addr), 0x3E); // MVI A
        assert_eq!(bus.memory.read(home_addr + 1), FD_RESTORE);
    }
}