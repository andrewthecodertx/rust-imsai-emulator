use rust_imsai_emulator::CpmBios;
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let disk_path = args.get(1).map(|s| s.as_str());
    let trace = args.contains(&"--trace".to_string()) || args.contains(&"-t".to_string());
    let verbose_trace = args.contains(&"--vtrace".to_string()) || args.contains(&"-v".to_string());

    let mut emu = rust_imsai_emulator::Imsai8080::new();

    let disk_file = if let Some(path) = disk_path {
        path.to_string()
    } else {
        let default = "disk_images/cpm22-boot.img";
        if Path::new(default).exists() {
            default.to_string()
        } else {
            eprintln!("Usage: {} [disk_image.img] [--trace] [--vtrace]", args.get(0).unwrap());
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

    if verbose_trace {
        run_verbose_trace(&mut emu, 200000);
    } else if trace {
        run_trace(&mut emu, 50000);
    } else {
        run_interactive(&mut emu, 5_000_000);
    }
}

fn boot_cpm(emu: &mut rust_imsai_emulator::Imsai8080) {
    use rust_imsai_emulator::dpb;

    let off = dpb::OFF as u8;
    let spt = dpb::SPT as u8;
    let sector_size = dpb::SECTOR_SIZE;

    // Read system tracks from disk in physical sector order
    let mut addr: u16 = 0x0000;
    for track in 0..off {
        for sector in 1..=spt {
            match emu.bus.io.tarbell.get_disk(0) {
                Some(disk) => {
                    match disk.read_sector(track, sector) {
                        Ok(data) => {
                            for (i, &byte) in data.iter().enumerate() {
                                let mem_addr = addr + i as u16;
                                if mem_addr < 0xFFFF {
                                    emu.bus.memory.write(mem_addr, byte);
                                }
                            }
                            addr = addr.saturating_add(sector_size as u16);
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

    println!("Loaded {} bytes of system tracks", addr);

    // Install our emulator-compatible BIOS
    CpmBios::install(&mut emu.bus);

    // Set memory size byte so CP/M cold start passes
    emu.bus.memory.write(0x037A, 0x06); // 64K = 6 * 4K pages... actually
    // In standard CP/M, the value at 0x037A is the IOBYTE, not memory size.
    // The CMI5619 system stores its own value there. Setting 0x06 lets the
    // cold start pass its check.

    // Patch the cold start memory check to skip validation
    // At 0x0132: change JNZ 0x025A to JMP 0x0137 (skip to init continue)
    emu.bus.memory.write(0x0132, 0xC3); // JMP
    emu.bus.memory.write(0x0133, 0x37);   // lo addr
    emu.bus.memory.write(0x0134, 0x01);   // hi addr

    println!("Patched cold start, starting execution");

    // Start at 0x0000 (cold start JMP 0x012C)
    emu.cpu.pc = 0x0000;
    emu.cpu.sp = 0x0000;
    emu.cpu.c = 0x00;
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
    let mut last_output: u8 = 0;
    let mut count: u64 = 0;
    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.memory.read(pc);

        emu.step();
        count += 1;

        // Print console output (OUT port 0x00)
        if op == 0xD3 { // OUT
            let port = emu.bus.memory.read(pc + 1);
            if port == 0x00 && (emu.cpu.a >= 32 && emu.cpu.a < 127 || emu.cpu.a == 0x0D || emu.cpu.a == 0x0A) {
                print!("{}", emu.cpu.a as char);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            } else if (0x48..=0x4B).contains(&port) || port == 0xFE {
                println!("{:06}: OUT 0x{:02X},A=0x{:02X}", count, port, emu.cpu.a);
            }
        } else if op == 0xDB { // IN
            let port = emu.bus.memory.read(pc + 1);
            if port == 0x01 {
                // Console status - skip
            } else if (0x48..=0x4B).contains(&port) {
                // Only show every 100th disk I/O
                if count % 100 == 0 {
                    println!("{:06}: IN  0x{:02X}", count, port);
                }
            }
        }

        if emu.cpu.halted || count >= max {
            break;
        }

        last_output = emu.cpu.a;
    }

    println!("\nStopped at PC=0x{:04X} after {} instructions", emu.cpu.pc, count);
}