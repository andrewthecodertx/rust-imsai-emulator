//! CP/M 2.2 BIOS and boot loader for the IMSAI 8080 with Tarbell controller
//!
//! This module provides everything needed to boot and run CP/M 2.2:
//! - System track loader with DRI relocating-image support
//! - A full 17-entry CP/M 2.2 BIOS in 8080 machine code
//! - Disk Parameter Block and sector skew table
//!
//! Memory layout for 64K system after boot:
//!
//! | Address   | Contents                                    |
//! |-----------|---------------------------------------------|
//! | 0x0000    | JMP to WBOOT (warm start vector)             |
//! | 0x0003    | IOBYTE                                       |
//! | 0x0004    | Current drive                                |
//! | 0x0005    | JMP to BDOS                                  |
//! | 0x0100    | TPA start (Transient Program Area)           |
//! | 0xE400    | CCP (Command Control Program)                |
//! | 0xEC06    | BDOS                                         |
//! | 0xFA00    | BIOS (jump table + routines)                 |
//!
//! The DRI CP/M 2.2 system image is a relocating format. The CCP and BDOS
//! are stored at file offsets 0x0100 and 0x0906 respectively, with internal
//! addresses relative to the minimum 20K system. For a 64K system, we add
//! BIAS = (64-20)*1024 = 0xB000 to all addresses during load.
//!
//! The BIOS jump table and routines are installed at BIOS_BASE (0xFA00)
//! after the CP/M system image is loaded from disk and relocated. The
//! installed BIOS uses the Tarbell controller (ports 0x48-0x4B) and
//! console (ports 0x00-0x01).

#![allow(dead_code)]

use crate::bus::ImsaiBus;

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

/// Memory size in KBytes
const MSIZE: u16 = 64;

/// BIAS = (MSIZE - 20) * 1024. The DRI relocating image stores CCP+BDOS
/// as if for a 20K system. We add BIAS to all addresses to relocate for 64K.
const BIAS: u16 = (MSIZE - 20) * 1024; // 0xB000

/// CPMB = BIAS + 0x3400. Start of the CP/M system in memory.
const CPMB: u16 = BIAS + 0x3400; // 0xE400

/// CCP starts at CPMB
const CCP_ADDR: u16 = CPMB; // 0xE400

/// BDOS entry point. In the z80pack 64K CP/M 2.2 image,
/// BDOS is at 0xEC00 and the function dispatcher is at 0xEC06.
const BDOS_ADDR: u16 = 0xEC06;

/// Address where the BIOS jump table starts.
/// In the relocating image, BIOS is at CPMB + 0x1600.
const BIOS_BASE: u16 = CPMB + 0x1600; // 0xFA00

/// Address where the DPB is stored (placed well before BIOS to avoid overlap)
/// DPB = 15 bytes, Skew table = 26 bytes, scratch = 7 bytes = 48 bytes total
/// Place at BIOS_BASE - 48 = 0xF9D0
const DPB_ADDRESS: u16 = BIOS_BASE - 48; // 0xF9D0

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
    /// Load the CP/M system image from a byte buffer and relocate it for 64K.
    ///
    /// The DRI CP/M 2.2 system image uses a relocating format where:
    /// - File offset 0x0000-0x00FF: Boot sector + cold start code
    /// - File offset 0x0100: CCP (assembled with ORG 0x0100)
    /// - File offset 0x0900: BDOS (assembled with ORG 0x0000)
    ///
    /// The CCP and BDOS have DIFFERENT relocation bases:
    /// - CCP addresses are relative to 0x0100 (their ORG address)
    /// - BDOS addresses are relative to 0x0000 (BDOS assembled with ORG 0)
    ///
    /// For a 64K system:
    /// - CCP is loaded at 0xE400 → CCP bias = 0xE400 - 0x0100 = 0xE300
    /// - BDOS is loaded at 0xEC00 → BDOS base = 0xEC00
    ///
    /// The DRI cold start scans for JMP/CALL opcodes and relocates addresses.
    /// We do the same, but apply the correct bias per segment.
    pub fn load_and_relocate(bus: &mut ImsaiBus, system_data: &[u8]) {
        let _system_size = system_data.len();

        // Load the system tracks as raw sectors into memory at CPMB (0xE400),
        // starting from sector 2 (offset 0x80 in the raw image).
        // This matches how the CMI5619 boot loader works: it reads sectors
        // 2-78 sequentially into CPMB without any relocation.
        //
        // After loading, we apply the DRI relocation to the CCP and BDOS
        // segments based on where they actually land in memory.
        const SECTOR_2_OFFSET: usize = 0x80; // skip boot sector
        let load_len = system_data.len().saturating_sub(SECTOR_2_OFFSET);

        for i in 0..load_len {
            let mem_addr = CPMB.wrapping_add(i as u16);
            if mem_addr >= BIOS_BASE {
                break; // Don't overwrite our BIOS area at 0xFA00
            }
            bus.memory.write(mem_addr, system_data[SECTOR_2_OFFSET + i]);
        }

        println!("Loaded {} bytes raw at CPMB 0x{:04X}", load_len, CPMB);

        // Now apply DRI relocation. The CPM.CPM relocating image has:
        // - CCP at file offset 0x0100, assembled with ORG 0x0100
        // - BDOS at file offset 0x0900, assembled with ORG 0x0000
        //
        // After our raw load from offset 0x80 to CPMB:
        // - File offset 0x0100 maps to memory CPMB + (0x100 - 0x80) = 0xE480
        // - File offset 0x0900 maps to memory CPMB + (0x900 - 0x80) = 0xEC80
        //
        // So CCP is at 0xE480 and BDOS is at 0xEC80 in memory.
        // CCP_BIAS = 0xE480 - 0x0100 = 0xE380
        // BDOS_BIAS = 0xEC80 (since BDOS was ORG 0)
        //
        // But wait — the standard CP/M 2.2 layout expects CCP at 0xE400
        // and BDOS at 0xEC00. Our CCP at 0xE480 is 0x80 off from standard.
        // The CMI5619 uses a different layout where the first 128 bytes
        // (sector 2 = DRI copyright/cold start data) precede the CCP.
        //
        // The key insight: the DRI relocating image was DESIGNED for the
        // cold start code to determine memory size and load the CCP+BDOS
        // at addresses that correspond to that memory size. The sector 2
        // data (copyright, etc.) is NOT part of the CCP — it's cold start
        // data that gets overwritten after the system is configured.
        //
        // For a 64K system, the standard layout is:
        //   CCP  at 0xE400 (CPMB)
        //   BDOS at 0xEC00 (CPMB + 0x800)
        //   BIOS at 0xFA00 (CPMB + 0x1600)
        //
        // So the correct approach is:
        // 1. Load CCP from file offset 0x0100 to memory 0xE400
        // 2. Relocate CCP with BIAS = 0xE400 - 0x0100 = 0xE300
        // 3. Load BDOS from file offset 0x0900 to memory 0xEC00
        // 4. Relocate BDOS with BIAS = 0xEC00
        //
        // This is what the original code tried to do, but we need to be
        // careful about NOT relocating data that isn't CCP/BDOS code.
        // The CMI5619 BIOS at file offsets 0x700+ should be skipped.

        // CCP: file offset 0x0100, load at 0xE400, BIAS = 0xE300
        const CCP_FILE_START: usize = 0x0100;
        const CCP_FILE_END: usize = 0x0900; // BDOS follows at 0x900
        const CCP_BIAS: u16 = CCP_ADDR - 0x0100; // 0xE300

        // Copy CCP to memory at 0xE400
        if CCP_FILE_END <= system_data.len() {
            for i in 0..(CCP_FILE_END - CCP_FILE_START) {
                let mem_addr = CCP_ADDR + i as u16;
                if mem_addr >= BIOS_BASE {
                    break;
                }
                bus.memory.write(mem_addr, system_data[CCP_FILE_START + i]);
            }

            // Relocate CCP addresses
            Self::relocate_segment(
                bus,
                CCP_ADDR,
                CCP_BIAS,
                0x0000, // min relocatable
                0x4000, // max relocatable
                CCP_ADDR,
                (CCP_FILE_END - CCP_FILE_START) as u16,
            );
        }

        // BDOS: file offset 0x0900, load at 0xEC00, BIAS = 0xEC00
        const BDOS_MEM_BASE: u16 = CPMB + 0x0800; // 0xEC00
        const BDOS_FILE_START: usize = 0x0900;
        const BDOS_BIAS: u16 = BDOS_MEM_BASE; // since ORG was 0

        let bdos_len = system_data.len().saturating_sub(BDOS_FILE_START);
        for i in 0..bdos_len {
            let mem_addr = BDOS_MEM_BASE.wrapping_add(i as u16);
            if mem_addr >= BIOS_BASE {
                break;
            }
            bus.memory.write(mem_addr, system_data[BDOS_FILE_START + i]);
        }

        // Relocate BDOS addresses
        Self::relocate_segment(
            bus,
            BDOS_MEM_BASE,
            BDOS_BIAS,
            0x0000,
            0x4000,
            BDOS_MEM_BASE,
            bdos_len as u16,
        );

        println!("Applied DRI relocation: CCP BIAS=0x{:04X}, BDOS BIAS=0x{:04X}", CCP_BIAS, BDOS_BIAS);
    }

    /// 16-bit address operand that falls within the `[min_addr, max_addr)` range.
    fn relocate_segment(
        bus: &mut ImsaiBus,
        _base: u16,
        bias: u16,
        min_addr: u16,
        max_addr: u16,
        mem_start: u16,
        mem_len: u16,
    ) {
        let mut i: u16 = 0;
        while i < mem_len {
            let op = bus.memory.read(mem_start.wrapping_add(i));
            match op {
                // 3-byte instructions with 16-bit address operand
                0xC3 | 0xC2 | 0xCA | 0xD2 | 0xDA | 0xE2 | 0xEA | 0xF2 | 0xFA | // JMPs
                0xCD | 0xC4 | 0xCC | 0xD4 | 0xDC | 0xE4 | 0xEC | 0xF4 | 0xFC | // CALLs
                0x21 | 0x11 | 0x01 | 0x31 | // LXI
                0x32 | 0x3A | 0x22 | 0x2A   // STA/LDA/SHLD/LHLD
                => {
                    if i + 2 < mem_len {
                        let addr_loc = mem_start.wrapping_add(i).wrapping_add(1);
                        let lo = bus.memory.read(addr_loc) as u16;
                        let hi = bus.memory.read(addr_loc + 1) as u16;
                        let target = lo | (hi << 8);
                        if target >= min_addr && target < max_addr {
                            let new_target = target.wrapping_add(bias);
                            bus.memory.write(addr_loc, new_target as u8);
                            bus.memory.write(addr_loc + 1, (new_target >> 8) as u8);
                        }
                    }
                    i += 3;
                }
                // 2-byte instructions (skip 2nd byte)
                0x3E | 0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | // MVI
                0xC6 | 0xD6 | 0xE6 | 0xF6 | 0xEE | 0xFE | 0xCE | 0xDE | // immediate ALU
                0xDB | 0xD3 // IN/OUT
                => {
                    i += 2;
                }
                // 1-byte instructions
                _ => {
                    i += 1;
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
    fn test_relocate_segment() {
        let mut bus = ImsaiBus::new();
        // Write a JMP at 0xE400 with target 0x0100 (CCP ORG address)
        bus.memory.write(0xE400, 0xC3); // JMP
        bus.memory.write(0xE401, 0x00); // lo
        bus.memory.write(0xE402, 0x01); // hi = 0x0100

        // Relocate with CCP BIAS = 0xE300, range 0x0000-0x4000
        CpmBios::relocate_segment(&mut bus, 0xE400, 0xE300, 0x0000, 0x4000, 0xE400, 3);

        // 0x0100 + 0xE300 = 0xE400
        assert_eq!(bus.memory.read(0xE401), 0x00);
        assert_eq!(bus.memory.read(0xE402), 0xE4);
    }

    #[test]
    fn test_relocate_segment_preserves_out_of_range() {
        let mut bus = ImsaiBus::new();
        // Write a JMP at 0xE400 with target 0xFF00 (outside relocatable range)
        bus.memory.write(0xE400, 0xC3); // JMP
        bus.memory.write(0xE401, 0x00); // lo
        bus.memory.write(0xE402, 0xFF); // hi = 0xFF00

        // Should NOT relocate addresses outside [0x0000, 0x4000)
        CpmBios::relocate_segment(&mut bus, 0xE400, 0xE300, 0x0000, 0x4000, 0xE400, 3);

        assert_eq!(bus.memory.read(0xE401), 0x00);
        assert_eq!(bus.memory.read(0xE402), 0xFF);
    }

    #[test]
    fn test_load_and_relocate_empty() {
        let mut bus = ImsaiBus::new();
        // Too-small system image should bail out gracefully
        let tiny_data = vec![0u8; 128];
        CpmBios::load_and_relocate(&mut bus, &tiny_data);
        // Memory should remain unchanged at CCP addr
        assert_eq!(bus.memory.read(CCP_ADDR), 0x00);
    }

    #[test]
    fn test_memory_layout_constants() {
        // Verify our constants match the standard CP/M 2.2 64K layout
        assert_eq!(MSIZE, 64);
        assert_eq!(BIAS, 0xB000);
        assert_eq!(CPMB, 0xE400);
        assert_eq!(CCP_ADDR, 0xE400);
        assert_eq!(BDOS_ADDR, 0xEC06);
        assert_eq!(BIOS_BASE, 0xFA00);
        // TPA range: 0x0100 to 0xE3FF = 58,368 bytes
        assert_eq!(CCP_ADDR - 0x0100, 0xE300);
    }

    #[test]
    fn test_dpb_constants() {
        // Verify DPB address placement doesn't overlap with BIOS_BASE
        assert!(DPB_ADDRESS < BIOS_BASE);
        assert!(SKEW_ADDRESS > DPB_ADDRESS);
        assert!(DMA_STORAGE > SKEW_ADDRESS);
        // DPB + skew + scratch should fit before BIOS
        assert!((BIOS_BASE - DPB_ADDRESS) >= 48, "Not enough room for DPB data before BIOS");
    }
}