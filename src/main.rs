use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let trace = args.contains(&"--trace".to_string()) || args.contains(&"-t".to_string());
    let verbose_trace = args.contains(&"--vtrace".to_string()) || args.contains(&"-v".to_string());
    let diag = args.contains(&"--diag".to_string()) || args.contains(&"-d".to_string());
    let step_trace = args.contains(&"--step".to_string()) || args.contains(&"-s".to_string());
    let disk_path = args.iter().skip(1).find(|a| !a.starts_with('-')).map(|s| s.as_str());

    let mut emu = rust_imsai_emulator::Imsai8080::new();

    let disk_file = if let Some(path) = disk_path {
        path.to_string()
    } else {
        let default = "disk_images/cpm22-boot.img";
        if Path::new(default).exists() {
            default.to_string()
        } else {
            eprintln!("Usage: {} [disk_image.img] [--trace] [--vtrace] [--diag] [--step]", args.get(0).unwrap());
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
    } else if verbose_trace {
        run_verbose_trace(&mut emu, 200000);
    } else if trace {
        run_trace(&mut emu, 50000);
    } else {
        run_interactive(&mut emu, 5_000_000);
    }
}

/// CP/M 2.2 64K system memory layout
const CPMB: u16 = 0xE400; // CCP base address for 64K

fn boot_cpm(emu: &mut rust_imsai_emulator::Imsai8080) {
    use rust_imsai_emulator::dpb;

    let off = dpb::OFF as u8; // 3 reserved tracks
    let spt = dpb::SPT as u8; // 26 sectors per track

    // Read all system tracks from disk into memory at 0x0000 first.
    // We need the raw data to find the system image boundaries.
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

    // Copy the CCP+BDOS+BIOS from the system image to CPMB (0xE400).
    // The system image starts at file offset 0x0100 (CCP) which should be
    // placed at CPMB. The CMI5619 boot loader reads starting at sector 2
    // (offset 0x80 = 128) into CPMB, but standard DRI reads from offset
    // 0x0100 (sector 3). Since CPM.CPM is 9728 bytes = 76 sectors,
    // and the CMI5619 boot reads from sector 2, the data starts at
    // file offset 0x80 (128 bytes into the image).
    //
    // The 2DBOOT64.ASM says: LXI H,CPMB; MVI D,NSECTS; MVI C,2 (start sector)
    // So sector 2 (1-indexed) = the second 128-byte sector on track 0.
    // In our raw image, that's byte offset 128 (0x80).
    //
    // Copy from offset 0x80 in system tracks to CPMB in memory.
    let sys_start = 0x80usize; // sector 2 on track 0
    let sys_len = system_size - sys_start;
    for i in 0..sys_len {
        let mem_addr = CPMB.wrapping_add(i as u16);
        if mem_addr == 0 {
            break;
        }
        emu.bus.memory.write(mem_addr, buf[sys_start + i]);
    }

    println!("Loaded {} bytes at CPMB 0x{:04X}", sys_len, CPMB);

    // Debug: print memory around 0x0000 and 0x0010 before install
    println!("Before install:");
    for addr in [0x0000, 0x0003, 0x0005, 0x0010, 0x0013, 0x0100] {
        let lo = emu.bus.memory.read(addr);
        let hi = emu.bus.memory.read(addr + 1);
        let next = if addr < 0x0100 { emu.bus.memory.read(addr + 2) } else { 0 };
        println!("  0x{:04X}: {:02X} {:02X} {:02X}", addr, lo, hi, next);
    }

    // Install our custom BIOS. This overwrites the CMI5619 BIOS
    // that was loaded from the system tracks with our emulator-compatible
    // version that uses the correct I/O ports.
    rust_imsai_emulator::Bios::install_jump_table(&mut emu.bus);

    // Debug: print memory around 0x0000 and 0x0010 after install
    println!("After Bios::install_jump_table():");
    for addr in [0x0000, 0x0003, 0x0005, 0x0010, 0x0013, 0x0100, 0x0104, 0x0108] {
        let lo = emu.bus.memory.read(addr);
        let hi = emu.bus.memory.read(addr + 1);
        let next = if addr < 0x0108 { emu.bus.memory.read(addr + 2) } else { 0 };
        println!("  0x{:04X}: {:02X} {:02X} {:02X}", addr, lo, hi, next);
    }

    // CCP_ADDR = 0xE400

    // The CMI5619 BIOS BOOT entry (BOOTE at CPMB+0x1600 = 0xFA00) does:
    // 1. Set up vectors at 0x0000 (JMP WBOOT) and 0x0005 (JMP BDOS)
    // 2. Read current drive from 0x0004
    // 3. JMP to CCP at CPMB
    //
    // But our CpmBios::install() just set these vectors. The CMI5619 BOOTE
    // would overwrite them. Since we've installed our BIOS at 0xFA00,
    // and our BOOT routine just does OUT 0xFE then JMP CCP, we simply
    // start at the BOOT entry point which jumps to CCP.
    //
    // However, the CMI5619 BOOTE also sets up important state that the CCP
    // needs (like the BDOS address in the system image). The CMI5619 BOOTE
    // code at file offset 0x2024 writes 0x0005 = JMP 0x0806 where 0x0806
    // is relative to CPMB. After relocation to CPMB=0xE400, BDOS entry =
    // 0xE400 + 0x0806 = 0xEC06.
    //
    // We need to set these vectors manually since we're skipping the
    // CMI5619 BOOTE initialization.

    // Set up warm boot vector: 0x0000 = JMP 0xFA00 (our BIOS WBOOT)
    // Already done by CpmBios::install()

    // Set up BDOS entry: 0x0005 = JMP BDOS_addr
    // The BDOS in the system image is at CPMB + 0x0800 (file offset 0x0900-0x80=0x0880).
    // The BDOS function entry (for CALL 5) is at BDOS start + 3.
    // The system image's BDOS at offset 0x0880 has: JMP 0x035C then JMP 0x0358
    // where 0x035C/0x0358 are BDOS-internal addresses (ORG 0).
    // These need to be relocated: add CPMB + 0x0800 - 0x0100 = 0xEB00 to them.
    // Actually, the BDOS was assembled with ORG 0, so the 0x035C means
    // "offset 0x035C from BDOS base". BDOS base in memory = CPMB + 0x0800 = 0xEC00.
    // So 0x035C memory address = 0xEC00 + 0x035C = 0xEF5C.
    //
    // We need to patch the BDOS JMPs to relocated addresses.
    let bdos_base: u16 = CPMB + 0x0800; // 0xEC00
    // Patch BDOS+0 (JMP target): was 0x035C, becomes bdos_base + 0x035C
    let bdos_init = bdos_base.wrapping_add(0x035C);
    emu.bus.memory.write(bdos_base + 1, bdos_init as u8);
    emu.bus.memory.write(bdos_base + 2, (bdos_init >> 8) as u8);
    // Patch BDOS+3 (JMP target): was 0x0358, becomes bdos_base + 0x0358
    let bdos_entry = bdos_base.wrapping_add(0x0358);
    emu.bus.memory.write(bdos_base + 4, bdos_entry as u8);
    emu.bus.memory.write(bdos_base + 5, (bdos_entry >> 8) as u8);

    // Set 0x0005 = JMP to BDOS function entry (bdos_base + 3)
    let bdos_func_entry = bdos_base + 3; // 0xEC03
    emu.bus.memory.write(0x0005, 0xC3);
    emu.bus.memory.write(0x0006, bdos_func_entry as u8);
    emu.bus.memory.write(0x0007, (bdos_func_entry >> 8) as u8);

    // Now relocate ALL internal addresses within the CCP and BDOS.
    // CCP was ORG 0x0100 in the original system, loaded at file offset 0x80+0x80 = 0x100.
    // Wait - the system data starts at offset 0x80 in our buffer (sector 2).
    // File offset 0x0100 in CPM.CPM = buffer offset 0x0100 - 0x80 = 0x80? No...
    // 
    // Actually: CPM.CPM is 9728 bytes (the system tracks data).
    // Our buffer has ALL system tracks (9984 bytes = 3 tracks × 26 × 128).
    // The CPM.CPM data occupies the first 9728 bytes of the system tracks.
    // We copied from buffer offset 0x80 (= sector 2) to memory at CPMB.
    // So: buffer[0x80] -> memory[0xE400]
    //     buffer[0x80 + i] -> memory[0xE400 + i]
    //
    // The CCP at CPM.CPM offset 0x0100 corresponds to buffer offset 0x0100,
    // which is memory offset 0x0100 - 0x80 = 0x0080 from CPMB,
    // i.e., memory address 0xE480.
    // Wait, that can't be right. Let me reconsider.
    // 
    // The CMI5619 boot (2DBOOT64) reads from sector 2 onward.
    // Track 0: sectors 1-26 (128 bytes each = 3328 bytes)
    // Sector 1 = bytes 0-127 (boot sector, not loaded into CPMB)
    // Sector 2 = bytes 128-255 (loaded at CPMB)
    // ...
    // Sector 26 = bytes 3250-3377 (loaded at CPMB + 319*1 = CPMB + 0x13F)
    //
    // Actually, the boot loads sectors 2-26 of track 0 = 25 sectors = 3200 bytes
    // starting at CPMB. Then all of track 1 (26 sectors) and track 2 (26 sectors).
    // Total: 25 + 26 + 26 = 77 sectors = 9856 bytes.
    //
    // In our buffer (which contains ALL 78 sectors):
    // buffer[0x80..0x2700] = sectors 2-78 = system data
    // This is loaded at memory[CPMB..CPMB + 0x2680]
    //
    // In the CPM.CPM raw image:
    // offset 0x0000 = boot sector (already at buffer[0..0x80])  
    // offset 0x0080 = start of system data loaded at CPMB
    // offset 0x0100 = CCP (loaded at CPMB + 0x0080 = 0xE480)?
    // No, that doesn't match CPM.CPM structure.
    //
    // The standard DRI CPM.CPM layout:
    // 0x0000-0x007F: boot sector (sector 1 on track 0)
    // 0x0080-0x00FF: sector 2 on track 0  
    // 0x0100-0x017F: sector 3 on track 0 (CCP starts here)
    // But the CMI5619 boot loads from sector 2, so:
    // buffer[0x80] -> CPMB, buffer[0x100] -> CPMB + 0x80
    // CPM.CPM offset 0x0100 (CCP) -> memory CPMB + 0x80

    // Hmm, this means the CCP is at CPMB + 0x80 = 0xE480, not at CPMB.
    // But that contradicts the 2ABIOS64.ASM which says "JMP CPMB" (0xE400).
    // The CMI5619 boot loads from sector 2 (skipping sector 1), so the
    // first byte loaded goes to CPMB. The BOOT sector has the loader code
    // and the DRI JMP/Copyright/etc. The ACTUAL CCP starts at the offset
    // where it appears in the system data after sector 1.
    //
    // In CPM.CPM: sector 3 (offset 0x100) has 3D C2 00 02 which is DRI CCP code.
    // Wait, that's what we saw earlier. The CCP in CPM.CPM is at offset 0x0100.
    // But the CMI5619 boot loads starting from sector 2 (offset 0x80).
    // So sector 2 data goes to CPMB + 0, sector 3 goes to CPMB + 0x80, etc.
    // The CCP at sector 3 (file offset 0x100) ends up at CPMB + 0x80 = 0xE480.
    //
    // BUT the standard CP/M BDOS entry at 0x0005 is set to JMP BDOS where
    // BDOS is at CPMB + some offset. The CMI5619 BIOS code sets it to
    // JMP CPMB+0x0806. CPMB+0x806 would be 0xE400+0x0806 = 0xEC06 which
    // is the BDOS entry.
    //
    // So the system data loaded from sector 2 onward at CPMB has:
    // offset 0 from CPMB: sector 2 data (0x80-0xFF in CPM.CPM = copyright string etc.)
    // offset 0x80 from CPMB: sector 3 (0x100 in CPM.CPM = CCP start)
    // offset 0x800 from CPMB: approximately sector 18+ (0x800+0x80=0x880 -> BDOS at 0x900?)
    //
    // Actually this is getting very confusing. Let me just check what
    // data is at CPMB in memory after our copy.
    println!("Memory at CPMB (0xE400): {:02X} {:02X} {:02X} {:02X}",
        emu.bus.memory.read(CPMB), emu.bus.memory.read(CPMB + 1),
        emu.bus.memory.read(CPMB + 2), emu.bus.memory.read(CPMB + 3));
    println!("Memory at CPMB+0x80 (0xE480): {:02X} {:02X} {:02X} {:02X}",
        emu.bus.memory.read(CPMB + 0x80), emu.bus.memory.read(CPMB + 0x81),
        emu.bus.memory.read(CPMB + 0x82), emu.bus.memory.read(CPMB + 0x83));
    println!("BDOS at 0xEC00: {:02X} {:02X} {:02X}",
        emu.bus.memory.read(0xEC00), emu.bus.memory.read(0xEC01), emu.bus.memory.read(0xEC02));
    println!("BDOS at 0xEC06: {:02X} {:02X}",
        emu.bus.memory.read(0xEC06), emu.bus.memory.read(0xEC07));

    // Start at our WBOOT entry (0x0000) which does JMP to CCP (0xE400)
    // emu.cpu.pc = 0xFA00;
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