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