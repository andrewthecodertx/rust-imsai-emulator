use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let trace = args.contains(&"--trace".to_string()) || args.contains(&"-t".to_string());
    let verbose_trace = args.contains(&"--vtrace".to_string()) || args.contains(&"-v".to_string());
    let diag = args.contains(&"--diag".to_string()) || args.contains(&"-d".to_string());
    let step_trace = args.contains(&"--step".to_string()) || args.contains(&"-s".to_string());
    let pc_trace = args.contains(&"--pctrace".to_string()) || args.contains(&"-p".to_string());
    let disk_path = args.iter().skip(1).find(|a| !a.starts_with('-')).map(|s| s.as_str());

    let mut emu = rust_imsai_emulator::Imsai8080::new();

    let disk_file = if let Some(path) = disk_path {
        path.to_string()
    } else {
        let default = "disk_images/cpm22-boot.img";
        if Path::new(default).exists() {
            default.to_string()
        } else {
            eprintln!("Usage: {} [disk_image.img] [--trace] [--vtrace] [--diag] [--step] [--pctrace]", args.get(0).unwrap());
            return;
        }
    };

    match emu.bus.io.tarbell.insert_disk(0, &disk_file) {
        Ok(()) => println!("Loaded disk: {}", disk_file),
        Err(e) => {
            eprintln!("Error loading disk '{}': {}", disk_file, e);
            return;
        }
    }

    boot_cpm(&mut emu);

    if step_trace {
        run_step_trace(&mut emu, 500);
    } else if diag {
        run_diag(&mut emu, 50000);
    } else if pc_trace {
        run_pc_trace(&mut emu, 5_000_000);
    } else if verbose_trace {
        run_verbose_trace(&mut emu, 200000);
    } else if trace {
        run_trace(&mut emu, 50000);
    } else {
        run_interactive(&mut emu, 5_000_000);
    }
}

/// CP/M 2.2 64K system memory layout
///
/// | Address   | Contents                                    |
/// |-----------|---------------------------------------------|
/// | 0x0000    | JMP WBOOT (0xFA03)                          |
/// | 0x0003    | IOBYTE                                       |
/// | 0x0004    | Current drive                                |
/// | 0x0005    | JMP BDOS (0xEC03)                            |
/// | 0x0100    | TPA start                                    |
/// | 0xE400    | CCP (Command Control Program)                |
/// | 0xEC00    | BDOS                                         |
/// | 0xFA00    | BIOS (jump table + routines)                 |
const CPMB: u16 = 0xE400;

fn boot_cpm(emu: &mut rust_imsai_emulator::Imsai8080) {
    use rust_imsai_emulator::dpb;

    let off = dpb::OFF as u8; // 3 reserved tracks
    let spt = dpb::SPT as u8; // 26 sectors per track

    // Read all system tracks from disk into a buffer.
    // The system tracks contain the CP/M boot sector, CCP, BDOS, and
    // the original CMI5619 BIOS. We load them, then use CpmBios to
    // relocate the CCP and BDOS into their proper memory locations.
    let sector_size = dpb::SECTOR_SIZE as usize;
    let system_size = off as usize * spt as usize * sector_size;
    let mut buf = vec![0u8; system_size];
    let mut offset = 0usize;

    for track in 0..off {
        for sector in 1..=spt {
            match emu.bus.io.tarbell.get_disk(0) {
                Some(disk) => {
                    match disk.read_sector(track, sector) {
                        Ok(data) => {
                            let end = std::cmp::min(offset + data.len(), system_size);
                            buf[offset..end].copy_from_slice(&data[..end - offset]);
                            offset = end;
                        }
                        Err(e) => {
                            eprintln!("Error reading track {} sector {}: {}", track, sector, e);
                            return;
                        }
                    }
                }
                None => {
                    eprintln!("No disk in drive 0");
                    return;
                }
            }
        }
    }

    println!("Read {} bytes from system tracks", offset);

    // Use CpmBios::load_and_relocate() to properly load the DRI relocating
    // image format into memory with correct bias computation per segment.
    rust_imsai_emulator::CpmBios::load_and_relocate(&mut emu.bus, &buf);

    // Install our custom BIOS at 0xFA00. This replaces the CMI5619 BIOS
    // with our emulator-compatible version that uses the correct I/O ports
    // (0x00-0x01 for console, 0x48-0x4B for Tarbell controller).
    // This also sets up the system vectors at 0x0000 and 0x0005.
    rust_imsai_emulator::Bios::install_jump_table(&mut emu.bus);

    // Debug: verify key memory locations
    println!("Boot vectors:");
    println!("  0x0000: {:02X} {:02X} {:02X}  (should be C3 03 FA = JMP 0xFA03)",
        emu.bus.memory.read(0x0000), emu.bus.memory.read(0x0001), emu.bus.memory.read(0x0002));
    println!("  0x0005: {:02X} {:02X} {:02X}  (should be C3 03 EC = JMP 0xEC03)",
        emu.bus.memory.read(0x0005), emu.bus.memory.read(0x0006), emu.bus.memory.read(0x0007));
    println!("  CPMB:   {:02X} {:02X} {:02X} {:02X}  (CCP first bytes)",
        emu.bus.memory.read(CPMB), emu.bus.memory.read(CPMB + 1),
        emu.bus.memory.read(CPMB + 2), emu.bus.memory.read(CPMB + 3));
    println!("  CPMB+0x100: {:02X} {:02X} {:02X} {:02X}",
        emu.bus.memory.read(CPMB + 0x100), emu.bus.memory.read(CPMB + 0x101),
        emu.bus.memory.read(CPMB + 0x102), emu.bus.memory.read(CPMB + 0x103));
    println!("  0xE500: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
        emu.bus.memory.read(0xE500), emu.bus.memory.read(0xE501),
        emu.bus.memory.read(0xE502), emu.bus.memory.read(0xE503),
        emu.bus.memory.read(0xE504), emu.bus.memory.read(0xE505),
        emu.bus.memory.read(0xE506), emu.bus.memory.read(0xE507));
    println!("  BDOS:   {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
        emu.bus.memory.read(0xEC00), emu.bus.memory.read(0xEC01),
        emu.bus.memory.read(0xEC02), emu.bus.memory.read(0xEC03),
        emu.bus.memory.read(0xEC04), emu.bus.memory.read(0xEC05));
    println!("  BIOS:   {:02X} {:02X} {:02X}  (first JMP entry)",
        emu.bus.memory.read(0xFA00), emu.bus.memory.read(0xFA01),
        emu.bus.memory.read(0xFA02));

    // Start execution at the WBOOT vector (0x0000)
    emu.cpu.pc = 0x0000;
    emu.cpu.sp = 0x0000;
}

/// Step-by-step instruction trace
fn run_step_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    println!("=== STEP TRACE ({} instructions) ===", max);
    let mut count: u64 = 0;

    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.memory.read(pc);

        let desc = match op {
            0xC3 => {
                let lo = emu.bus.memory.read(pc + 1);
                let hi = emu.bus.memory.read(pc + 2);
                format!("JMP 0x{:02X}{:02X}", hi, lo)
            }
            0xCD => {
                let lo = emu.bus.memory.read(pc + 1);
                let hi = emu.bus.memory.read(pc + 2);
                format!("CALL 0x{:02X}{:02X}", hi, lo)
            }
            0xC9 => "RET".to_string(),
            0xD3 => {
                let port = emu.bus.memory.read(pc + 1);
                format!("OUT 0x{:02X},A=0x{:02X}", port, emu.cpu.a)
            }
            0xDB => {
                let port = emu.bus.memory.read(pc + 1);
                format!("IN A,0x{:02X}", port)
            }
            0x31 => {
                let lo = emu.bus.memory.read(pc + 1);
                let hi = emu.bus.memory.read(pc + 2);
                format!("LXI SP,0x{:02X}{:02X}", hi, lo)
            }
            0x00 => "NOP".to_string(),
            _ => format!("0x{:02X}", op),
        };

        println!("{:6}: PC=0x{:04X} {:20} A=0x{:02X} C=0x{:02X} SP=0x{:04X}",
            count, pc, desc, emu.cpu.a, emu.cpu.c, emu.cpu.sp);

        emu.step();
        count += 1;

        if emu.cpu.halted || count >= max {
            break;
        }
    }

    println!("\nStopped at PC=0x{:04X} after {} instructions", emu.cpu.pc, count);
}

fn run_diag(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    println!("=== DIAGNOSTIC TRACE ({} instructions) ===", max);
    let mut count: u64 = 0;
    let mut io_log: Vec<(u64, u8, u8, bool)> = Vec::new();
    let mut pc_snapshots: Vec<(u64, u16)> = Vec::new();

    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.memory.read(pc);
        emu.step();
        count += 1;

        if op == 0xD3 {
            let port = emu.bus.memory.read(pc + 1);
            io_log.push((count, port, emu.cpu.a, true));
        } else if op == 0xDB {
            let port = emu.bus.memory.read(pc + 1);
            io_log.push((count, port, emu.cpu.a, false));
        }

        if count % 10000 == 0 {
            pc_snapshots.push((count, emu.cpu.pc));
        }

        if emu.cpu.halted || count >= max {
            break;
        }
    }

    println!("\n=== I/O LOG (first 50) ===");
    for (i, (cnt, port, val, is_out)) in io_log.iter().take(50).enumerate() {
        let dir = if *is_out { "OUT" } else { "IN " };
        println!("{:5}: {:08} {:3} 0x{:02X} A=0x{:02X}", i, cnt, dir, port, val);
    }
    if io_log.len() > 50 {
        println!("  ... {} total I/O operations", io_log.len());
    }

    println!("\n=== PC SNAPSHOTS ===");
    for (cnt, pc) in &pc_snapshots {
        println!("  {:08}: PC=0x{:04X}", cnt, pc);
    }

    println!("\n=== FINAL STATE ===");
    println!("PC=0x{:04X} SP=0x{:04X} A=0x{:02X}", emu.cpu.pc, emu.cpu.sp, emu.cpu.a);
    println!("0x0000: {:02X} {:02X} {:02X}", emu.bus.memory.read(0), emu.bus.memory.read(1), emu.bus.memory.read(2));
    println!("0x0005: {:02X} {:02X} {:02X}", emu.bus.memory.read(5), emu.bus.memory.read(6), emu.bus.memory.read(7));

    let display = emu.bus.io.video.get_display_string();
    if !display.trim().is_empty() && display.trim().chars().any(|c| c != ' ') {
        println!("\nDisplay:\n{}", display);
    } else {
        println!("\n(no display output)");
    }
}

fn run_interactive(emu: &mut rust_imsai_emulator::Imsai8080, max_instructions: u64) {
    println!("IMSAI 8080 - CP/M 2.2 ({} instructions)", max_instructions);

    let mut count: u64 = 0;
    loop {
        for _ in 0..1000 {
            emu.step();
            count += 1;
        }

        if emu.cpu.halted || count >= max_instructions {
            break;
        }
    }

    println!("\nStopped at PC=0x{:04X} after {} instructions", emu.cpu.pc, count);
    let display = emu.bus.io.video.get_display_string();
    if !display.trim().is_empty() && display.trim().chars().any(|c| c != ' ') {
        println!("\nDisplay:\n{}", display);
    } else {
        println!("\n(no display output)");
    }
}

fn run_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    println!("Tracing {} instructions from PC=0x{:04X}...", max, emu.cpu.pc);
    let mut count: u64 = 0;
    loop {
        emu.step();
        count += 1;
        if emu.cpu.halted || count >= max {
            break;
        }
    }
    println!("Stopped at PC=0x{:04X} after {} instructions", emu.cpu.pc, count);
    let display = emu.bus.io.video.get_display_string();
    println!("\nDisplay:\n{}", display);
}

/// PC trace: runs at full speed but keeps a ring buffer of the last N
/// fully-decoded instructions. Also captures all I/O operations.
/// Dumps the buffer and I/O log at the end so you can see exactly
/// what the CPU was doing right before it stopped or got stuck.
fn run_pc_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    const RING_SIZE: usize = 2048;
    println!("=== PC TRACE ({} instructions, ring={}) ===", max, RING_SIZE);

    // Ring buffer entries: (count, pc, op_bytes, A, B, C, D, E, H, L, SP, flags)
    let mut ring: Vec<(u64, u16, [u8; 4], u8, u8, u8, u8, u8, u8, u8, u16, u8)> =
        Vec::with_capacity(RING_SIZE);
    let mut ring_idx: usize = 0;
    let mut ring_full = false;

    // I/O log: (count, port, value, is_out)
    let mut io_log: Vec<(u64, u8, u8, bool)> = Vec::new();

    // CALL 5 tracker
    let mut call5_count: u64 = 0;
    let mut last_call5_func: u8 = 0;

    let mut count: u64 = 0;
    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.memory.read(pc);

        // Capture registers BEFORE step
        let a = emu.cpu.a;
        let b = emu.cpu.b;
        let c = emu.cpu.c;
        let d = emu.cpu.d;
        let e = emu.cpu.e;
        let h = emu.cpu.h;
        let l = emu.cpu.l;
        let sp = emu.cpu.sp;
        let flags = emu.cpu.flags.as_psw();

        // Grab up to 4 bytes for the instruction
        let mut op_bytes = [0u8; 4];
        for j in 0..4u16 {
            op_bytes[j as usize] = emu.bus.memory.read(pc.wrapping_add(j));
        }

        emu.step();
        count += 1;

        // Detect CALL 5 (BDOS entry)
        if op == 0xCD {
            let lo = emu.bus.memory.read(pc + 1);
            let hi = emu.bus.memory.read(pc + 2);
            let target = lo as u16 | (hi as u16) << 8;
            if target == 0x0005 {
                call5_count += 1;
                last_call5_func = c; // BDOS function number is in C
            }
        }

        // Detect I/O
        if op == 0xD3 {
            let port = emu.bus.memory.read(pc + 1);
            io_log.push((count, port, a, true));
        } else if op == 0xDB {
            let port = emu.bus.memory.read(pc + 1);
            io_log.push((count, port, a, false));
        }

        // Write to ring buffer
        if ring.len() < RING_SIZE {
            ring.push((count, pc, op_bytes, a, b, c, d, e, h, l, sp, flags));
        } else {
            ring[ring_idx] = (count, pc, op_bytes, a, b, c, d, e, h, l, sp, flags);
            ring_idx = (ring_idx + 1) % RING_SIZE;
            ring_full = true;
        }

        if emu.cpu.halted || count >= max {
            break;
        }
    }

    // Dump ring buffer in chronological order
    println!("\n=== LAST {} INSTRUCTIONS ===", ring.len().min(RING_SIZE));
    let start = if ring_full { ring_idx } else { 0 };
    let len = ring.len().min(RING_SIZE);
    for i in 0..len {
        let idx = (start + i) % len;
        let (cnt, pc, bytes, a, b, c, d, e, h, l, sp, flags) = ring[idx];
        let desc = disassemble_8080(pc, bytes);
        println!("{:8}: PC={:04X} {:30} A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:04X} F={:02X}",
            cnt, pc, desc, a, b, c, d, e, h, l, sp, flags);
    }

    // Dump I/O log
    println!("\n=== I/O LOG (all {} operations) ===", io_log.len());
    for (i, (cnt, port, val, is_out)) in io_log.iter().enumerate() {
        let dir = if *is_out { "OUT" } else { "IN " };
        let port_name = match port {
            0x00 => "CON_DATA",
            0x01 => "CON_STAT",
            0x48 => "TARB_STAT",
            0x49 => "TARB_TRK",
            0x4A => "TARB_SEC",
            0x4B => "TARB_DATA",
            0xFE => "DEBUG",
            _ => "",
        };
        if port_name.is_empty() {
            println!("{:5}: {:08} {:3} 0x{:02X}       A=0x{:02X}", i, cnt, dir, port, val);
        } else {
            println!("{:5}: {:08} {:3} 0x{:02X} {:10} A=0x{:02X}", i, cnt, dir, port, port_name, val);
        }
    }
    if io_log.is_empty() {
        println!("  (none — no IN/OUT instructions executed)");
    }

    // Dump CALL 5 summary
    println!("\n=== CALL 5 (BDOS) SUMMARY ===");
    println!("  Total CALL 5 calls: {}", call5_count);
    if call5_count > 0 {
        println!("  Last function number in C: 0x{:02X}", last_call5_func);
    }

    // Final state
    println!("\n=== FINAL STATE ===");
    println!("PC=0x{:04X} SP=0x{:04x} A=0x{:02X} BC=0x{:02X}{:02X} DE=0x{:02X}{:02X} HL=0x{:02X}{:02X}",
        emu.cpu.pc, emu.cpu.sp, emu.cpu.a,
        emu.cpu.b, emu.cpu.c, emu.cpu.d, emu.cpu.e, emu.cpu.h, emu.cpu.l);
    println!("0x0000: {:02X} {:02X} {:02X}   (WBOOT vector)",
        emu.bus.memory.read(0), emu.bus.memory.read(1), emu.bus.memory.read(2));
    println!("0x0005: {:02X} {:02X} {:02X}   (BDOS vector)",
        emu.bus.memory.read(5), emu.bus.memory.read(6), emu.bus.memory.read(7));
}

/// Minimal 8080 disassembler for trace output.
/// Takes PC (for display) and up to 4 bytes of the instruction.
fn disassemble_8080(_pc: u16, bytes: [u8; 4]) -> String {
    let op = bytes[0];
    let lo = bytes[1];
    let hi = bytes[2];
    let addr = lo as u16 | (hi as u16) << 8;

    // 8080 instruction decode
    match op {
        // NOP
        0x00 => "NOP".into(),
        // LXI
        0x01 => format!("LXI BC,0x{:04X}", addr),
        0x11 => format!("LXI DE,0x{:04X}", addr),
        0x21 => format!("LXI HL,0x{:04X}", addr),
        0x31 => format!("LXI SP,0x{:04X}", addr),
        // JMP/JNZ/JZ/JNC/JC
        0xC3 => format!("JMP 0x{:04X}", addr),
        0xC2 => format!("JNZ 0x{:04X}", addr),
        0xCA => format!("JZ 0x{:04X}", addr),
        0xD2 => format!("JNC 0x{:04X}", addr),
        0xDA => format!("JC 0x{:04X}", addr),
        0xE2 => format!("JPO 0x{:04X}", addr),
        0xEA => format!("JPE 0x{:04X}", addr),
        0xF2 => format!("JP 0x{:04X}", addr),
        0xFA => format!("JM 0x{:04X}", addr),
        // CALL/RET
        0xCD => format!("CALL 0x{:04X}", addr),
        0xC4 => format!("CNZ 0x{:04X}", addr),
        0xCC => format!("CZ 0x{:04X}", addr),
        0xD4 => format!("CNC 0x{:04X}", addr),
        0xDC => format!("CC 0x{:04X}", addr),
        0xC9 => "RET".into(),
        0xC0 => "RNZ".into(),
        0xC8 => "RZ".into(),
        0xD0 => "RNC".into(),
        0xD8 => "RC".into(),
        // MVI
        0x3E => format!("MVI A,0x{:02X}", lo),
        0x06 => format!("MVI B,0x{:02X}", lo),
        0x0E => format!("MVI C,0x{:02X}", lo),
        0x16 => format!("MVI D,0x{:02X}", lo),
        0x1E => format!("MVI E,0x{:02X}", lo),
        0x26 => format!("MVI H,0x{:02X}", lo),
        0x2E => format!("MVI L,0x{:02X}", lo),
        0x36 => format!("MVI M,0x{:02X}", lo),
        // IN/OUT
        0xDB => format!("IN 0x{:02X}", lo),
        0xD3 => format!("OUT 0x{:02X}", lo),
        // MOV
        0x7F => "MOV A,A".into(),
        0x78 => "MOV A,B".into(),
        0x79 => "MOV A,C".into(),
        0x7A => "MOV A,D".into(),
        0x7B => "MOV A,E".into(),
        0x7C => "MOV A,H".into(),
        0x7D => "MOV A,L".into(),
        0x7E => "MOV A,M".into(),
        0x47 => "MOV B,A".into(),
        0x40 => "MOV B,B".into(),
        0x41 => "MOV B,C".into(),
        0x42 => "MOV B,D".into(),
        0x43 => "MOV B,E".into(),
        0x44 => "MOV B,H".into(),
        0x45 => "MOV B,L".into(),
        0x46 => "MOV B,M".into(),
        0x4F => "MOV C,A".into(),
        0x48 => "MOV C,B".into(),
        0x49 => "MOV C,C".into(),
        0x4A => "MOV C,D".into(),
        0x4B => "MOV C,E".into(),
        0x4C => "MOV C,H".into(),
        0x4D => "MOV C,L".into(),
        0x4E => "MOV C,M".into(),
        0x57 => "MOV D,A".into(),
        0x5F => "MOV E,A".into(),
        0x67 => "MOV H,A".into(),
        0x6F => "MOV L,A".into(),
        0x77 => "MOV M,A".into(),
        0x70 => "MOV M,B".into(),
        0x71 => "MOV M,C".into(),
        // Arithmetic/Logic
        0x80..=0x8F => {
            let names = ["ADD","ADC","SUB","SBB","ANA","XRA","ORA","CMP"];
            let reg = op & 0x07;
            let op_name = names[((op >> 3) & 7) as usize];
            let reg_name = ["B","C","D","E","H","L","M","A"][reg as usize];
            format!("{} {}", op_name, reg_name)
        }
        // Increment/Decrement
        0x04 => "INR B".into(),
        0x0C => "INR C".into(),
        0x14 => "INR D".into(),
        0x1C => "INR E".into(),
        0x24 => "INR H".into(),
        0x2C => "INR L".into(),
        0x34 => "INR M".into(),
        0x3C => "INR A".into(),
        0x05 => "DCR B".into(),
        0x0D => "DCR C".into(),
        0x15 => "DCR D".into(),
        0x1D => "DCR E".into(),
        0x25 => "DCR H".into(),
        0x2D => "DCR L".into(),
        0x35 => "DCR M".into(),
        0x3D => "DCR A".into(),
        // INX/DCX
        0x03 => "INX BC".into(),
        0x13 => "INX DE".into(),
        0x23 => "INX HL".into(),
        0x33 => "INX SP".into(),
        0x0B => "DCX BC".into(),
        0x1B => "DCX DE".into(),
        0x2B => "DCX HL".into(),
        0x3B => "DCX SP".into(),
        // DAD
        0x09 => "DAD BC".into(),
        0x19 => "DAD DE".into(),
        0x29 => "DAD HL".into(),
        0x39 => "DAD SP".into(),
        // Stack
        0xC5 => "PUSH BC".into(),
        0xD5 => "PUSH DE".into(),
        0xE5 => "PUSH HL".into(),
        0xF5 => "PUSH PSW".into(),
        0xC1 => "POP BC".into(),
        0xD1 => "POP DE".into(),
        0xE1 => "POP HL".into(),
        0xF1 => "POP PSW".into(),
        // Memory
        0x32 => format!("STA 0x{:04X}", addr),
        0x3A => format!("LDA 0x{:04X}", addr),
        0x22 => format!("SHLD 0x{:04X}", addr),
        0x2A => format!("LHLD 0x{:04X}", addr),
        0xEB => "XCHG".into(),
        0xE3 => "XTHL".into(),
        0xF9 => "SPHL".into(),
        // Immediate
        0xC6 => format!("ADI 0x{:02X}", lo),
        0xCE => format!("ACI 0x{:02X}", lo),
        0xD6 => format!("SUI 0x{:02X}", lo),
        0xDE => format!("SBI 0x{:02X}", lo),
        0xE6 => format!("ANI 0x{:02X}", lo),
        0xEE => format!("XRI 0x{:02X}", lo),
        0xF6 => format!("ORI 0x{:02X}", lo),
        0xFE => format!("CPI 0x{:02X}", lo),
        // Control
        0x76 => "HLT".into(),
        0xF3 => "DI".into(),
        0xFB => "EI".into(),
        0xE9 => "PCHL".into(),
        // RST
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
            format!("RST {}", (op >> 3) & 7)
        }
        // MOV r,r (remaining common ones)
        _ => {
            // Try MOV decode: 0b01dddsss
            if op >> 6 == 0b01 {
                let dst = (op >> 3) & 7;
                let src = op & 7;
                let reg_names = ["B","C","D","E","H","L","M","A"];
                format!("MOV {},{}", reg_names[dst as usize], reg_names[src as usize])
            } else {
                format!("0x{:02X}", op)
            }
        }
    }
}

fn run_verbose_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    let mut count: u64 = 0;
    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.memory.read(pc);

        emu.step();
        count += 1;

        if op == 0xD3 {
            let port = emu.bus.memory.read(pc + 1);
            if port == 0x00 && (emu.cpu.a >= 32 && emu.cpu.a < 127 || emu.cpu.a == 0x0D || emu.cpu.a == 0x0A) {
                print!("{}", emu.cpu.a as char);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            } else if (0x48..=0x4B).contains(&port) || (0xF8..=0xFD).contains(&port) {
                if count % 500 == 0 {
                    println!("{:06}: OUT 0x{:02X},A=0x{:02X}", count, port, emu.cpu.a);
                }
            }
        } else if op == 0xDB {
            let port = emu.bus.memory.read(pc + 1);
            if port == 0x01 || port == 0xF9 {
                // Console/disk status polling - too noisy
            } else if (0x48..=0x4B).contains(&port) || (0xF8..=0xFD).contains(&port) {
                if count % 500 == 0 {
                    println!("{:06}: IN  0x{:02X}", count, port);
                }
            }
        }

        if emu.cpu.halted || count >= max {
            break;
        }
    }

    println!("\nStopped at PC=0x{:04X} after {} instructions", emu.cpu.pc, count);
}