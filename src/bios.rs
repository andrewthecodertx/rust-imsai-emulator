//! CP/M 2.2 BIOS implementation for the IMSAI 8080 emulator
//!
//! Installs a complete CP/M 2.2 BIOS at 0xFA00 in memory, including:
//! - System vectors at 0x0000 (WBOOT) and 0x0005 (BDOS)
//! - 17-entry BIOS jump table at 0xFA00
//! - Full BIOS routines using Tarbell controller ports 0x48-0x4B
//!   and console ports 0x00-0x01
//!
//! Memory layout after install:
//!
//! | Address   | Contents                                    |
//! |-----------|---------------------------------------------|
//! | 0x0000    | JMP WBOOT (0xFA03)                          |
//! | 0x0003    | IOBYTE                                       |
//! | 0x0004    | Current drive                                |
//! | 0x0005    | JMP BDOS (0xEC03)                            |
//! | 0xFA00    | BIOS jump table (17 × 3-byte JMP entries)   |
//! | 0xFA33+   | BIOS routines                                |

use crate::bus::ImsaiBus;

/// BIOS base address for 64K CP/M 2.2 system
const BIOS_BASE: u16 = 0xFA00;

/// Number of CP/M 2.2 BIOS entries
const NUM_ENTRIES: usize = 17;

/// BDOS entry point for CALL 5 (BDOS+3 = function dispatcher)
const BDOS_ENTRY: u16 = 0xEC03;

/// CCP base address for 64K system
const CCP_ADDR: u16 = 0xE400;

/// Console I/O ports
const CON_DATA: u8 = 0x00;
const CON_STAT: u8 = 0x01;

/// Tarbell controller I/O ports
const TARB_STAT: u8 = 0x48;
const TARB_TRK: u8 = 0x49;
const TARB_SEC: u8 = 0x4A;
const TARB_DATA: u8 = 0x4B;

/// DPB/skew/scratch addresses (just below BIOS)
const DPB_ADDR: u16 = 0xF9D0;
const SKEW_ADDR: u16 = DPB_ADDR + 15;
const CUR_TRACK: u16 = 0xF9E8;
const CUR_SECTOR: u16 = 0xF9E9;
const CUR_DMA: u16 = 0xF9EA;
const CUR_DISK: u16 = 0xF9EC;

/// Simple code builder that tracks the current absolute address.
struct CodeBuilder {
    code: Vec<u8>,
    base: u16,
}

impl CodeBuilder {
    fn new(base: u16) -> Self {
        Self { code: Vec::with_capacity(512), base }
    }

    /// Current absolute address where the next byte will be placed
    fn here(&self) -> u16 {
        self.base + self.code.len() as u16
    }

    fn emit(&mut self, b: u8) {
        self.code.push(b);
    }

    fn emit_u16(&mut self, v: u16) {
        self.code.push(v as u8);
        self.code.push((v >> 8) as u8);
    }

    fn emit_jmp(&mut self, target: u16) {
        self.emit(0xC3); self.emit_u16(target);
    }

    fn emit_jz(&mut self, target: u16) {
        self.emit(0xCA); self.emit_u16(target);
    }

    fn emit_jnz(&mut self, target: u16) {
        self.emit(0xC2); self.emit_u16(target);
    }

    fn emit_jnc(&mut self, target: u16) {
        self.emit(0xD2); self.emit_u16(target);
    }

    fn emit_mvi_a(&mut self, val: u8) {
        self.emit(0x3E); self.emit(val);
    }

    fn emit_mvi_b(&mut self, val: u8) {
        self.emit(0x06); self.emit(val);
    }

    fn emit_out(&mut self, port: u8) {
        self.emit(0xD3); self.emit(port);
    }

    fn emit_in(&mut self, port: u8) {
        self.emit(0xDB); self.emit(port);
    }

    fn emit_ani(&mut self, val: u8) {
        self.emit(0xE6); self.emit(val);
    }

    fn emit_cpi(&mut self, val: u8) {
        self.emit(0xFE); self.emit(val);
    }

    fn emit_sta(&mut self, addr: u16) {
        self.emit(0x32); self.emit_u16(addr);
    }

    fn emit_lhld(&mut self, addr: u16) {
        self.emit(0x2A); self.emit_u16(addr);
    }

    fn emit_shld(&mut self, addr: u16) {
        self.emit(0x22); self.emit_u16(addr);
    }

    fn emit_lxi_h(&mut self, val: u16) {
        self.emit(0x21); self.emit_u16(val);
    }

    fn emit_lxi_sp(&mut self, val: u16) {
        self.emit(0x31); self.emit_u16(val);
    }

    fn emit_ret(&mut self) { self.emit(0xC9); }
}

pub struct Bios;

impl Bios {
    pub fn install_jump_table(bus: &mut ImsaiBus) {
        // ── System vectors ──
        write_jmp(bus, 0x0000, BIOS_BASE + 3); // JMP WBOOT (jump table entry 1)
        bus.memory.write(0x0003, 0x00); // IOBYTE
        bus.memory.write(0x0004, 0x00); // Current drive = A
        write_jmp(bus, 0x0005, BDOS_ENTRY);

        // ── Build BIOS routines ──
        let routine_base = BIOS_BASE + (NUM_ENTRIES as u16) * 3;
        let mut b = CodeBuilder::new(routine_base);
        let mut entry_addrs: [u16; NUM_ENTRIES] = [0; NUM_ENTRIES];

        // Entry 0: BOOT — debug marker, fall through to WBOOT
        entry_addrs[0] = b.here();
        b.emit_out(0xFE);

        // Entry 1: WBOOT — set A=0 (warm boot), reset SP, initialize zero page, jump to CCP
        // CP/M expects A=0 for warm boot (A≠0 means cold boot)
        entry_addrs[1] = b.here();
        b.emit_mvi_a(0x00); // A = 0 (warm boot indicator)
        b.emit_lxi_sp(0x0000);
        // Initialize key zero-page locations
        // 0x0003: IOBYTE = 0 (console)
        b.emit_mvi_a(0x00); // actually, MVI destroys A... we need A=0 still
        // STA 0x0003 is safe since A is already 0
        // But we need to use LXI H + SHLD for 16-bit stores
        // Actually: A=0, and we need to store to 0x0003 and 0x0004
        // Simplest: STA 0x0003; STA 0x0004
        b.emit(0x32); b.emit_u16(0x0003); // STA 0x0003 (IOBYTE = 0)
        b.emit(0x32); b.emit_u16(0x0004); // STA 0x0004 (current drive = 0)
        // Restore A=0 in case STA affected it (it doesn't on 8080, but be safe)
        b.emit_mvi_a(0x00);
        b.emit_jmp(CCP_ADDR);

        // Entry 2: CONST — check console status
        entry_addrs[2] = b.here();
        let const_false = b.here() + 10; // IN(2) ANI(2) JZ(3) MVI_0xFF(2) RET(1) = 10
        b.emit_in(CON_STAT);
        b.emit_ani(0x01);
        b.emit_jz(const_false);
        b.emit_mvi_a(0xFF);
        b.emit_ret();
        // const_false:
        debug_assert_eq!(b.here(), const_false);
        b.emit_mvi_a(0x00);
        b.emit_ret();

        // Entry 3: CONIN — blocking console read
        entry_addrs[3] = b.here();
        let conin_loop = b.here();
        b.emit_in(CON_STAT);
        b.emit_ani(0x01);
        b.emit_jz(conin_loop);
        b.emit_in(CON_DATA);
        b.emit_ani(0x7F);
        b.emit_ret();

        // Entry 4: CONOUT — write char in C to console
        entry_addrs[4] = b.here();
        b.emit(0x79); // MOV A,C
        b.emit_out(CON_DATA);
        b.emit_ret();

        // Entry 5: LIST — write to list device (no-op)
        entry_addrs[5] = b.here();
        b.emit_ret();

        // Entry 6: PUNCH — write to punch (same as console)
        entry_addrs[6] = b.here();
        b.emit(0x79); // MOV A,C
        b.emit_out(CON_DATA);
        b.emit_ret();

        // Entry 7: READER — return EOF (0x1A)
        entry_addrs[7] = b.here();
        b.emit_mvi_a(0x1A);
        b.emit_ret();

        // Entry 8: HOME — seek to track 0
        entry_addrs[8] = b.here();
        b.emit_mvi_a(0x00); // RESTORE command
        b.emit_out(TARB_STAT);
        let home_wait = b.here();
        b.emit_in(TARB_STAT);
        b.emit_ani(0x01); // BUSY
        b.emit_jnz(home_wait);
        b.emit_mvi_a(0x00);
        b.emit_sta(CUR_TRACK);
        b.emit_ret();

        // Entry 9: SELDSK — select disk, return HL=DPB or 0
        entry_addrs[9] = b.here();
        b.emit(0x79); // MOV A,C
        b.emit_cpi(0x04);
        // seldsk_err is after: JNC(3) + MOV+STA+LXI+RET(1+3+3+1=8)
        let seldsk_err = b.here() + 3 + 1 + 3 + 3 + 1;
        b.emit_jnc(seldsk_err);
        b.emit(0x79); // MOV A,C
        b.emit_sta(CUR_DISK);
        b.emit_lxi_h(DPB_ADDR);
        b.emit_ret();
        // seldsk_err:
        debug_assert_eq!(b.here(), seldsk_err);
        b.emit_lxi_h(0x0000);
        b.emit_ret();

        // Entry 10: SETTRK — set track (C = track number)
        entry_addrs[10] = b.here();
        b.emit(0x79); // MOV A,C
        b.emit_out(TARB_TRK);
        b.emit_sta(CUR_TRACK);
        b.emit_ret();

        // Entry 11: SETSEC — set sector (C = sector number)
        entry_addrs[11] = b.here();
        b.emit(0x79); // MOV A,C
        b.emit_out(TARB_SEC);
        b.emit_sta(CUR_SECTOR);
        b.emit_ret();

        // Entry 12: SETDMA — set DMA address (BC = address)
        entry_addrs[12] = b.here();
        b.emit(0x69); // MOV L,C
        b.emit(0x60); // MOV H,B
        b.emit_shld(CUR_DMA);
        b.emit_ret();

        // Entry 13: READ — read sector into DMA buffer
        entry_addrs[13] = b.here();
        b.emit_mvi_a(0x80); // READ command
        b.emit_out(TARB_STAT);
        let read_loop = b.here();
        b.emit_in(TARB_STAT);
        b.emit_ani(0x02); // DRQ
        // Forward reference: skip error path (ANI+JNZ+MVI+RET = 2+3+2+1=8 bytes)
        // Plus the JNZ itself (3 bytes)
        let read_got_drq = b.here() + 3 + 2 + 3 + 2 + 1;
        b.emit_jnz(read_got_drq);
        // No DRQ: check if still busy
        b.emit_ani(0x01); // BUSY
        b.emit_jnz(read_loop);
        // Error: not busy, no DRQ
        b.emit_mvi_a(0x01);
        b.emit_ret();
        // read_got_drq:
        debug_assert_eq!(b.here(), read_got_drq);
        b.emit_lhld(CUR_DMA);
        b.emit_mvi_b(128);
        let read_byte_loop = b.here();
        b.emit_in(TARB_DATA);
        b.emit(0x77); // MOV M,A
        b.emit(0x23); // INX H
        b.emit(0x05); // DCR B
        b.emit_jnz(read_byte_loop);
        let read_wait = b.here();
        b.emit_in(TARB_STAT);
        b.emit_ani(0x01); // BUSY
        b.emit_jnz(read_wait);
        b.emit_mvi_a(0x00); // success
        b.emit_ret();

        // Entry 14: WRITE — write sector from DMA buffer
        entry_addrs[14] = b.here();
        b.emit_mvi_a(0xA0); // WRITE command
        b.emit_out(TARB_STAT);
        let write_loop = b.here();
        b.emit_in(TARB_STAT);
        b.emit_ani(0x02); // DRQ
        let write_got_drq = b.here() + 3 + 2 + 3 + 2 + 1;
        b.emit_jnz(write_got_drq);
        b.emit_ani(0x01); // BUSY
        b.emit_jnz(write_loop);
        b.emit_mvi_a(0x01); // error
        b.emit_ret();
        // write_got_drq:
        debug_assert_eq!(b.here(), write_got_drq);
        b.emit_lhld(CUR_DMA);
        b.emit_mvi_b(128);
        let write_byte_loop = b.here();
        b.emit(0x7E); // MOV A,M
        b.emit_out(TARB_DATA);
        b.emit(0x23); // INX H
        b.emit(0x05); // DCR B
        b.emit_jnz(write_byte_loop);
        let write_wait = b.here();
        b.emit_in(TARB_STAT);
        b.emit_ani(0x01); // BUSY
        b.emit_jnz(write_wait);
        b.emit_mvi_a(0x00); // success
        b.emit_ret();

        // Entry 15: LISTST — always ready (0xFF)
        entry_addrs[15] = b.here();
        b.emit_mvi_a(0xFF);
        b.emit_ret();

        // Entry 16: SECTRAN — sector translation with skew table
        // BC = logical sector, DE = table address (0 = no translation)
        // If DE != 0: HL = table[BC], else HL = BC
        entry_addrs[16] = b.here();
        b.emit(0x7B); // MOV A,E
        b.emit(0xB2); // ORA D
        let sectran_no_xlate = b.here() + 3 + 1 + 1 + 1 + 1 + 2 + 1; // JZ(3) + XCHG+DAD+MOV+MOV+MVI+RET = 7
        b.emit_jz(sectran_no_xlate);
        b.emit(0xEB); // XCHG (HL = skew table addr)
        b.emit(0x09); // DAD B (HL = table + sector)
        b.emit(0x7E); // MOV A,M
        b.emit(0x6F); // MOV L,A
        b.emit_mvi_a(0x00); // H = 0 (reusing MVI A,0 then MOV H,A below)
        // Actually we need MVI H,0 not MVI A,0. The MVI A,0 clobbers A.
        // We just set L = physical sector. Set H = 0.
        // Back up: we need MOV L,A then MOV H,0? No, we already did MOV L,A.
        // We need H=0. MVI H,0 is 0x26 0x00 (2 bytes).
        // But I already emitted MVI A,0x00 (which is 0x3E 0x00, 2 bytes).
        // That's wrong - it should be MVI H,0x00 (0x26 0x00).
        // I need to fix this. Let me patch it.
        // Actually, I can't easily patch since the bytes are already pushed.
        // The MVI A,0 was supposed to be MVI H,0.
        // But wait, the code has: MOV L,A (L has physical sector), then we need H=0.
        // After MOV A,M; MOV L,A - A has the table value, L has it too.
        // We want H=0. The correct instruction is MOV H,A... no, A has the sector value.
        // We want H=0. So: LXI H,... no.
        // Simplest: after MOV L,A, we need MVI H,0.
        // I emitted emit_mvi_a(0x00) which is MVI A,0 (0x3E, 0x00).
        // That should have been 0x26, 0x00 (MVI H,0).
        // I'll need to fix this after building. Let me track it.
        // The builder already has the wrong bytes. I need to patch.
        b.emit_ret();
        // sectran_no_xlate:
        debug_assert_eq!(b.here(), sectran_no_xlate);
        b.emit(0x69); // MOV L,C
        b.emit(0x60); // MOV H,B
        b.emit_ret();

        // ── Fix SECTRAN: replace MVI A,0x00 with MVI H,0x00 ──
        // The wrong bytes (MVI A,0x00 = 0x3E,0x00) need to become (MVI H,0x00 = 0x26,0x00)
        // Find the MVI A,0xFF sequence in SECTRAN
        // We emitted: MOV E,D(0x7B) ORA D(0xB2) JZ(3 bytes) XCHG(0xEB) DAD B(0x09) MOV A,M(0x7E) MOV L,A(0x6F) MVI A,0x00(0x3E,0x00) RET(0xC9)
        // We need to find and patch 0x3E to 0x26 at the right position.
        // Actually, let me just fix it properly by patching the code vec directly.

        // ── Write code to memory ──
        for (i, &byte) in b.code.iter().enumerate() {
            bus.memory.write(routine_base + i as u16, byte);
        }

        // Patch SECTRAN: find the "MVI A,0x00" that should be "MVI H,0x00"
        // The SECTRAN section starts at entry_addrs[16] - routine_base in the code vec.
        // Let me find the sequence: 0x6F (MOV L,A) followed by 0x3E 0x00 near SECTRAN
        let sectran_start = (entry_addrs[16] - routine_base) as usize;
        for i in sectran_start..b.code.len() - 2 {
            if b.code[i] == 0x6F && b.code[i + 1] == 0x3E && b.code[i + 2] == 0x00 {
                bus.memory.write(routine_base + i as u16 + 1, 0x26); // MVI H,0x00
                break;
            }
        }

        // ── Write BIOS jump table ──
        for i in 0..NUM_ENTRIES {
            write_jmp(bus, BIOS_BASE + (i as u16) * 3, entry_addrs[i]);
        }

        // ── Install DPB ──
        let dpb = crate::dpb::DiskParameterBlock::tarbell_standard();
        let dpb_bytes = dpb.to_bytes();
        for (i, &byte) in dpb_bytes.iter().enumerate() {
            bus.memory.write(DPB_ADDR + i as u16, byte);
        }

        // ── Install skew table ──
        let skew: [u8; 26] = [
            1, 7, 13, 19, 25, 5, 11, 17, 23, 3, 9, 15, 21,
            2, 8, 14, 20, 26, 4, 10, 16, 22, 6, 12, 18, 24
        ];
        for (i, &byte) in skew.iter().enumerate() {
            bus.memory.write(SKEW_ADDR + i as u16, byte);
        }

        // ── Initialize scratch variables ──
        bus.memory.write(CUR_TRACK, 0);
        bus.memory.write(CUR_SECTOR, 1);
        bus.memory.write(CUR_DISK, 0);
        write_u16(bus, CUR_DMA, 0x0080);
    }
}

fn write_jmp(bus: &mut ImsaiBus, addr: u16, target: u16) {
    bus.memory.write(addr, 0xC3);
    write_u16(bus, addr + 1, target);
}

fn write_u16(bus: &mut ImsaiBus, addr: u16, val: u16) {
    bus.memory.write(addr, val as u8);
    bus.memory.write(addr + 1, (val >> 8) as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ImsaiBus;

    #[test]
    fn test_system_vectors() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // 0x0000: JMP WBOOT
        assert_eq!(bus.memory.read(0x0000), 0xC3);
        let wboot_target = bus.memory.read(0x0001) as u16 | (bus.memory.read(0x0002) as u16) << 8;
        // WBOOT is entry 1 in jump table, so jump from 0x0000 goes to WBOOT routine
        assert!(wboot_target >= 0xFA00, "WBOOT target should be in BIOS area, got 0x{:04X}", wboot_target);

        // 0x0005: JMP BDOS
        assert_eq!(bus.memory.read(0x0005), 0xC3);
        let bdos_target = bus.memory.read(0x0006) as u16 | (bus.memory.read(0x0007) as u16) << 8;
        assert_eq!(bdos_target, BDOS_ENTRY, "BDOS vector should point to 0x{:04X}, got 0x{:04X}", BDOS_ENTRY, bdos_target);

        // IOBYTE and current drive
        assert_eq!(bus.memory.read(0x0003), 0x00);
        assert_eq!(bus.memory.read(0x0004), 0x00);
    }

    #[test]
    fn test_bios_jump_table() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // All 17 entries should be JMP instructions targeting BIOS routine area
        for i in 0..NUM_ENTRIES {
            let addr = BIOS_BASE + (i as u16) * 3;
            assert_eq!(bus.memory.read(addr), 0xC3,
                "BIOS entry {} at 0x{:04X} should be JMP, got 0x{:02X}", i, addr, bus.memory.read(addr));
            let target = bus.memory.read(addr + 1) as u16
                | (bus.memory.read(addr + 2) as u16) << 8;
            assert!(target >= BIOS_BASE && target < 0xFF00,
                "BIOS entry {} target 0x{:04X} out of range", i, target);
        }
    }

    #[test]
    fn test_wboot_starts_with_mvi_a() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // Entry 1 (WBOOT) routine should start with MVI A,0x00
        let wboot_jmp = BIOS_BASE + 3; // jump table entry 1
        let wboot_target = bus.memory.read(wboot_jmp + 1) as u16
            | (bus.memory.read(wboot_jmp + 2) as u16) << 8;
        assert_eq!(bus.memory.read(wboot_target), 0x3E, // MVI A
            "WBOOT should start with MVI A,0x00, got 0x{:02X}", bus.memory.read(wboot_target));
        assert_eq!(bus.memory.read(wboot_target + 1), 0x00);
    }

    #[test]
    fn test_dpb_installed() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        assert_eq!(bus.memory.read(DPB_ADDR), 0x1A); // SPT low = 26
        assert_eq!(bus.memory.read(DPB_ADDR + 1), 0x00); // SPT high
    }

    #[test]
    fn test_conout_routine() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // CONOUT (entry 4): MOV A,C; OUT 0x00; RET
        let conout_jmp = BIOS_BASE + 4 * 3;
        let conout_target = bus.memory.read(conout_jmp + 1) as u16
            | (bus.memory.read(conout_jmp + 2) as u16) << 8;
        assert_eq!(bus.memory.read(conout_target), 0x79); // MOV A,C
        assert_eq!(bus.memory.read(conout_target + 1), 0xD3); // OUT
        assert_eq!(bus.memory.read(conout_target + 2), CON_DATA);
        assert_eq!(bus.memory.read(conout_target + 3), 0xC9); // RET
    }

    #[test]
    fn test_sectran_routine() {
        let mut bus = ImsaiBus::new();
        Bios::install_jump_table(&mut bus);

        // SECTRAN (entry 16) should start with MOV A,E (0x7B)
        let sectran_jmp = BIOS_BASE + 16 * 3;
        let sectran_target = bus.memory.read(sectran_jmp + 1) as u16
            | (bus.memory.read(sectran_jmp + 2) as u16) << 8;
        assert_eq!(bus.memory.read(sectran_target), 0x7B); // MOV A,E

        // The "MVI H,0" patch should have been applied (0x26, not 0x3E)
        // Find MOV L,A (0x6F) in SECTRAN routine
        let found_mvi_h = (0..30).any(|i| {
            bus.memory.read(sectran_target + i as u16) == 0x26
        });
        assert!(found_mvi_h, "SECTRAN should contain MVI H,0 (0x26)");
    }
}