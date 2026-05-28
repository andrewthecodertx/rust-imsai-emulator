//! BIOS implementation for the IMSAI 8080 emulator
//!
//! CP/M BIOS calls translated to I/O port operations on the bus.
//! CP/M uses a jump table at 0x0000 with entries pointing to BIOS routines.
//! The standard CP/M 1.4 BIOS functions:
//!
//! | Function | Register | Description |
//! |----------|----------|-------------|
//! | CONST    | C=1      | Check console status |
//! | CONIN    | C=3      | Read console character |
//! | CONOUT   | C=5      | Write console character |
//! | LIST     | C=7      | Write to listing device |
//! | READER   | C=9      | Read from reader device |
//! | LISTST   | C=11     | Check list device status |

use crate::bus::{ImsaiBus, PORT_CONSOLE_DATA, PORT_CONSOLE_STATUS};

/// Tarbell controller I/O ports
const TARBELL_CMD_STATUS: u8 = 0x48;
const TARBELL_TRACK: u8 = 0x49;
const TARBELL_SECTOR: u8 = 0x4A;
const TARBELL_DATA: u8 = 0x4B;

/// BIOS entry points in the CP/M jump table
const NUM_BIOS_ENTRIES: usize = 17;

/// BIOS implementation for CP/M
pub struct Bios;

impl Bios {
    /// Write CP/M BIOS jump table and warm boot loader into memory.
    ///
    /// The jump table at 0x0000 contains:
    ///   0x0000: JMP to warm boot (BIOS entry 0)
    ///   0x0003: IOBYTE
    ///   0x0005: JMP to BDOS entry
    ///   0x0006-0x002F: BIOS function jump table
    ///
    /// Each BIOS entry point (0x10, 0x14, 0x18, 0x1C, 0x20, 0x24) contains:
    ///   IN/OUT instruction for the corresponding I/O port
    ///   RET
    pub fn install_jump_table(bus: &mut ImsaiBus) {
        // Warm boot: WBOOT does nothing and jumps to CCP (0xE400)
        const CCP_ADDR: u16 = 0xE400;
        bus.memory.write(0x0000, 0xC3); // JMP
        bus.memory.write(0x0001, CCP_ADDR as u8);
        bus.memory.write(0x0002, (CCP_ADDR >> 8) as u8);

        // IOBYTE at 0x0003 (console only)
        bus.memory.write(0x0003, 0x00);

        // BDOS entry: JMP to warm boot (no BDOS yet, just restart)
        bus.memory.write(0x0005, 0xC3); // JMP
        bus.memory.write(0x0006, 0x00);
        bus.memory.write(0x0007, 0x00);

        // BIOS jump table entries starting at 0x0010
        let bios_base: u16 = 0x0010;
        let entry_size: u16 = 3; // JMP addr (3 bytes per entry)
        let routine_base: u16 = 0x0100;

        // Each routine is an IN/OUT + RET sequence at routine_base + i*4
        for i in 0..NUM_BIOS_ENTRIES {
            let jump_addr = bios_base + (i as u16) * entry_size;
            let routine_addr = routine_base + (i as u16) * 4;

            // Jump table entry: JMP routine_addr
            bus.memory.write(jump_addr, 0xC3); // JMP
            bus.memory.write(jump_addr + 1, routine_addr as u8);
            bus.memory.write(jump_addr + 2, (routine_addr >> 8) as u8);

            // Routine at routine_addr
            match i {
                0 => {
                    // CONST: IN PORT_CONSOLE_STATUS, ANI 0x01, RET
                    bus.memory.write(routine_addr, 0xDB); // IN
                    bus.memory.write(routine_addr + 1, PORT_CONSOLE_STATUS);
                    bus.memory.write(routine_addr + 2, 0xE6); // ANI
                    bus.memory.write(routine_addr + 3, 0x01);
                }
                1 => {
                    // CONIN: IN PORT_CONSOLE_DATA, ANI 0x7F, RET
                    bus.memory.write(routine_addr, 0xDB); // IN
                    bus.memory.write(routine_addr + 1, PORT_CONSOLE_DATA);
                    bus.memory.write(routine_addr + 2, 0xE6); // ANI
                    bus.memory.write(routine_addr + 3, 0x7F);
                }
                2 => {
                    // CONOUT: MOV A,C; OUT PORT_CONSOLE_DATA; RET
                    bus.memory.write(routine_addr, 0x79); // MOV A,C
                    bus.memory.write(routine_addr + 1, 0xD3); // OUT
                    bus.memory.write(routine_addr + 2, PORT_CONSOLE_DATA);
                    bus.memory.write(routine_addr + 3, 0xC9); // RET (replaces ANI slot)
                }
                3 => {
                    // LIST: just RET (no printer)
                    bus.memory.write(routine_addr, 0xC9); // RET
                }
                4 => {
                    // READER: return EOF (CTRL-Z = 0x1A)
                    bus.memory.write(routine_addr, 0x3E); // MVI A,0x1A
                    bus.memory.write(routine_addr + 1, 0x1A);
                    bus.memory.write(routine_addr + 2, 0xC9); // RET
                }
                5 => {
                    // LISTST: always ready (return 0xFF)
                    bus.memory.write(routine_addr, 0x3E); // MVI A,0xFF
                    bus.memory.write(routine_addr + 1, 0xFF);
                    bus.memory.write(routine_addr + 2, 0xC9); // RET
                }
                _ => {
                    // Stub routines for entries 6-16 (SELDSK, SETTRK, etc.)
                    // For disk operations, return failure (CPI 0x01, RET) to indicate no disk
                    bus.memory.write(routine_addr, 0xC9); // RET only (simple stub)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ImsaiBus;

    #[test]
    fn test_bios_jump_table_installed() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // Warm boot vector at 0x0000: JMP CCP_ADDR (0xE400)
        assert_eq!(bus.memory.read(0x0000), 0xC3);
        assert_eq!(bus.memory.read(0x0001), 0x00);
        assert_eq!(bus.memory.read(0x0002), 0xE4);

        // BDOS entry at 0x0005: JMP 0x0000 (placeholder until system loaded)
        assert_eq!(bus.memory.read(0x0005), 0xC3);

        // IOBYTE at 0x0003
        assert_eq!(bus.memory.read(0x0003), 0x00);

        // BIOS entry 0 (BOOT): JMP 0x0100
        assert_eq!(bus.memory.read(0x0010), 0xC3);
        assert_eq!(bus.memory.read(0x0011), 0x00);
        assert_eq!(bus.memory.read(0x0012), 0x01);

        // BOOT routine: IN console status (CONST stub since all 17 entries start at BOOT)
        assert_eq!(bus.memory.read(0x0100), 0xDB);
        assert_eq!(bus.memory.read(0x0101), PORT_CONSOLE_STATUS);
    }

    #[test]
    fn test_bios_conout_routine() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // CONOUT routine at 0x0108: MOV A,C, OUT 0x00
        assert_eq!(bus.memory.read(0x0108), 0x79); // MOV A,C
        assert_eq!(bus.memory.read(0x0109), 0xD3); // OUT
        assert_eq!(bus.memory.read(0x010A), PORT_CONSOLE_DATA);
    }

    #[test]
    fn test_bios_reader_returns_eof() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // READER routine at 0x0110: MVI A,0x1A
        assert_eq!(bus.memory.read(0x0110), 0x3E);
        assert_eq!(bus.memory.read(0x0111), 0x1A);
    }

    #[test]
    fn test_bios_listst_returns_ready() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // LISTST routine at 0x0114: MVI A,0xFF
        assert_eq!(bus.memory.read(0x0114), 0x3E);
        assert_eq!(bus.memory.read(0x0115), 0xFF);
    }
}