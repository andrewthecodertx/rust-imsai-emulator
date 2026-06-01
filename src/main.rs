use std::env;
use std::io::{self, Write};
use std::time::Instant;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode},
    ExecutableCommand,
};

/// CCP base address for 64K CP/M 2.2 system
const CPMB: u16 = 0xE400;

fn main() {
    let args: Vec<String> = env::args().collect();
    let trace = args.contains(&"--trace".to_string()) || args.contains(&"-t".to_string());
    let verbose_trace = args.contains(&"--vtrace".to_string()) || args.contains(&"-v".to_string());
    let diag = args.contains(&"--diag".to_string()) || args.contains(&"-d".to_string());
    let step_trace = args.contains(&"--step".to_string()) || args.contains(&"-s".to_string());
    let pc_trace = args.contains(&"--pctrace".to_string()) || args.contains(&"-p".to_string());
    let batch_mode = args.contains(&"--batch".to_string()) || args.contains(&"-b".to_string());
    // --cmd "DIR\r" pre-loads keyboard input for scripted testing
    let cmd_text = args.iter().position(|a| a == "--cmd")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone());
    let disk_path = if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd") {
        args.iter().skip(1)
            .enumerate()
            .filter(|(i, a)| *i != cmd_idx && *i != cmd_idx + 1 && !a.starts_with('-'))
            .map(|(_, a)| a.as_str())
            .next()
    } else {
        args.iter().skip(1).find(|a| !a.starts_with('-')).map(|s| s.as_str())
    };

    let mut emu = rust_imsai_emulator::Imsai8080::new();

    let disk_file = if let Some(path) = disk_path {
        path.to_string()
    } else {
        eprintln!("Usage: {} <disk_image.img> [options]", args.get(0).unwrap());
        eprintln!("Options: --batch  --trace  --vtrace  --diag  --step  --pctrace  --script  --cmd \"text\"");
        return;
    };

    match emu.bus.io.tarbell.insert_disk(0, &disk_file) {
        Ok(()) => println!("Loaded disk: {}", disk_file),
        Err(e) => {
            eprintln!("Error loading disk '{}': {}", disk_file, e);
            return;
        }
    }

    // Boot CP/M: load system tracks and install our BIOS
    boot_cpm(&mut emu);

    // Pre-load keyboard with --cmd text (convert escape sequences)
    if let Some(ref cmd) = cmd_text {
        let input = cmd.replace("\\r", "\r").replace("\\n", "\n");
        emu.bus.io.keyboard.type_text(&input);
    }

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
    } else if args.contains(&"--script".to_string()) {
        run_scripted(&mut emu, cmd_text.as_deref(), 100_000_000);
    } else if batch_mode {
        run_interactive(&mut emu, 50_000_000);
    } else {
        run_terminal(&mut emu);
    }
}
fn boot_cpm(emu: &mut rust_imsai_emulator::Imsai8080) {
    // CP/M 2.2 64K boot: load system tracks from disk image.
    //
    // The disk image must contain CCP+BDOS assembled for the 64K layout
    // (CCP=0xE400, BDOS=0xEC06) on its first 2-3 tracks. We load the
    // system tracks into memory, then install our own BIOS at 0xFA00.
    const BIOS_BASE: u16 = 0xFA00;

    let mut mem_addr: u16 = CPMB; // 0xE400
    let mut sectors_loaded: u16 = 0;

    // Load system tracks (tracks 0 through OFF-1) into memory at 0xE400.
    // Skip track 0 sector 1 (boot sector placeholder).
    for track in 0..2u8 {
        for sector in 1..=26u8 {
            if track == 0 && sector == 1 {
                continue; // skip boot sector
            }

            match emu.bus.io.tarbell.get_disk(0) {
                Some(disk) => {
                    match disk.read_sector(track, sector) {
                        Ok(data) => {
                            if mem_addr >= BIOS_BASE {
                                continue; // don't overwrite BIOS area
                            }
                            let end = mem_addr as usize + data.len();
                            if end > BIOS_BASE as usize {
                                let avail = BIOS_BASE - mem_addr;
                                for j in 0..avail as usize {
                                    emu.bus.memory.write(mem_addr + j as u16, data[j]);
                                }
                                mem_addr = BIOS_BASE;
                                sectors_loaded += 1;
                                continue;
                            }
                            for j in 0..data.len() {
                                emu.bus.memory.write(mem_addr + j as u16, data[j]);
                            }
                            mem_addr += data.len() as u16;
                            sectors_loaded += 1;
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

    let bytes_loaded = mem_addr - CPMB;
    println!("Loaded {} sectors ({} bytes) into 0x{:04X}-0x{:04X}",
        sectors_loaded, bytes_loaded, CPMB, CPMB + bytes_loaded);

    // Install our custom BIOS at 0xFA00.
    rust_imsai_emulator::Bios::install_jump_table(&mut emu.bus);

    // Start at CCP cold-start entry at 0xE400.
    emu.cpu.pc = CPMB;
    emu.cpu.sp = 0x0000;
}
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

/// Interactive terminal mode: raw terminal, real-time keyboard input, live console output.
/// This is the primary user-facing mode for running CP/M interactively.
fn run_terminal(emu: &mut rust_imsai_emulator::Imsai8080) {
    // Try to enable raw mode; fall back to batch mode if no TTY
    if enable_raw_mode().is_err() {
        eprintln!("No TTY available, falling back to batch mode (use --batch to force)");
        run_interactive(emu, 50_000_000);
        return;
    }

    // Disable auto-rendering of video display (we handle output directly)
    emu.bus.io.video.auto_render = false;

    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen).expect("Failed to enter alternate screen");
    stdout.flush().ok();

    // Print welcome banner
    print!("\r\nIMSAI 8080 - CP/M 2.2 Terminal\r\nPress Ctrl+] to exit\r\n---\r\n");
    stdout.flush().ok();

    let batch_size: u64 = 5000;
    let idle_sleep = std::time::Duration::from_millis(5);
    let poll_timeout = std::time::Duration::from_millis(0);
    let mut instruction_count: u64 = 0;
    let mut idle_count: u64 = 0;

    let start_time = Instant::now();

    loop {
        // Run a batch of CPU instructions
        for _ in 0..batch_size {
            let pc = emu.cpu.pc;
            let op = emu.bus.memory.read(pc);

            emu.step();
            instruction_count += 1;

            // Intercept OUT 0x00/0x7B (console data) to print directly to terminal
            if op == 0xD3 {
                let port = emu.bus.memory.read(pc + 1);
                if port == 0x00 || port == 0x7B {
                    let ch = emu.cpu.a;
                    if ch == 0x0D {
                        print!("\r");
                    } else if ch == 0x0A {
                        print!("\n");
                    } else if ch == 0x08 {
                        // Backspace: move left, clear, move left
                        print!("\x08 \x08");
                    } else if ch >= 0x20 && ch < 0x7F {
                        print!("{}", ch as char);
                    }
                    // Control chars below 0x20 (except CR/LF/BS) are silently ignored
                    stdout.flush().ok();
                }
            }

            if emu.cpu.halted {
                break;
            }
        }

        // After the batch, check for keyboard input (non-blocking)
        let mut got_key = false;
        while event::poll(poll_timeout).unwrap_or(false) {
            if let Ok(ev) = event::read() {
                match ev {
                    Event::Key(key_event) => {
                        match key_event {
                            KeyEvent { code: KeyCode::Esc, .. } => {
                                // Escape key: send ESC (0x1B) to CP/M
                                emu.bus.io.keyboard.type_text("\x1B");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Char(']'), modifiers: KeyModifiers::CONTROL, .. } => {
                                print!("\r\n\r\n--- Ctrl+] pressed, exiting ---\r\n");
                                stdout.flush().ok();
                                stdout.execute(LeaveAlternateScreen).ok();
                                disable_raw_mode().ok();
                                let elapsed = start_time.elapsed();
                                print_instructions_summary(instruction_count, elapsed);
                                return;
                            }
                            KeyEvent { code: KeyCode::Char(ch), modifiers: KeyModifiers::CONTROL, .. } => {
                                // Ctrl+key: send as control character (A=0x01, B=0x02, etc.)
                                let ctrl_ch = (ch as u8) & 0x1F;
                                if ctrl_ch != 0 {
                                    let buf = [ctrl_ch];
                                    emu.bus.io.keyboard.type_text(&String::from_utf8_lossy(&buf));
                                    got_key = true;
                                }
                            }
                            KeyEvent { code: KeyCode::Char(ch), .. } => {
                                // Regular character: convert to uppercase for CP/M
                                let byte = if ch == '\n' || ch == '\r' {
                                    0x0D_u8 // CR for CP/M
                                } else {
                                    ch.to_ascii_uppercase() as u8
                                };
                                let buf = [byte];
                                emu.bus.io.keyboard.type_text(&String::from_utf8_lossy(&buf));
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Enter, .. } => {
                                emu.bus.io.keyboard.type_text("\r");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Backspace, .. } => {
                                emu.bus.io.keyboard.type_text("\x7F");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Delete, .. } => {
                                emu.bus.io.keyboard.type_text("\x7F");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Tab, .. } => {
                                emu.bus.io.keyboard.type_text("\t");
                                got_key = true;
                            }
                            _ => {} // Ignore other key events
                        }
                    }
                    Event::Resize(_, _) => {
                        // Terminal resize - CP/M doesn't care
                    }
                    _ => {} // Ignore mouse and other events
                }
            }
        }

        if emu.cpu.halted {
            print!("\r\n\r\n--- CPU HALTED ---\r\n");
            break;
        }

        // If no key was pressed and the keyboard buffer is empty,
        // we're likely in the CONIN spin loop. Sleep briefly to
        // avoid burning 100% CPU.
        if !got_key && !emu.bus.io.keyboard.is_char_ready() {
            idle_count += 1;
            if idle_count > 5 {
                // After 5 idle batches, start sleeping to reduce CPU usage.
                // This means we'll still check for input every 5ms, which
                // gives ~200Hz polling rate - more than enough for typing.
                std::thread::sleep(idle_sleep);
            }
        } else {
            idle_count = 0;
        }
    }

    // Cleanup terminal
    print!("\r\n");
    stdout.execute(LeaveAlternateScreen).ok();
    disable_raw_mode().ok();

    let elapsed = start_time.elapsed();
    print_instructions_summary(instruction_count, elapsed);
}

fn print_instructions_summary(count: u64, elapsed: std::time::Duration) {
    let secs = elapsed.as_secs_f64();
    let ips = if secs > 0.0 { count as f64 / secs } else { 0.0 };
    eprintln!("Executed {} instructions in {:.2}s ({:.0} ips)",
        count, secs, ips);
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

    // Memory dump for debugging
    println!("\n=== MEMORY DUMP ===");
    dump_memory(&emu, 0x0000, 8, "Vectors");
    dump_memory(&emu, 0x0100, 16, "TPA");
    dump_memory(&emu, 0xF9F0, 48, "BDOS data/DPH area");
    dump_memory(&emu, 0xFB20, 48, "Our DPH+DIRBUF");
    dump_memory(&emu, 0xFBB0, 48, "CSV+ALV");
    dump_memory(&emu, 0xFA00, 8, "BIOS jump table");
}

fn dump_memory(emu: &rust_imsai_emulator::Imsai8080, start: u16, len: usize, label: &str) {
    print!("\n0x{:04X}: {} = ", start, label);
    for i in 0..len {
        if i > 0 && i % 16 == 0 {
            print!("\n        ");
        }
        print!("{:02X} ", emu.bus.memory.read(start + i as u16));
    }
    println!();
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
fn run_pc_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    const RING_SIZE: usize = 8192;
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

    // Early trace: capture first 200 instructions
    const EARLY_TRACE_SIZE: usize = 200;
    let mut early_trace: Vec<(u64, u16, [u8; 4], u8, u8, u8, u8, u8, u8, u8, u16, u8)> = Vec::new();

    // Region transition log: record when PC crosses a major boundary
    // (e.g., from BIOS area to CCP, or from CCP to TPA)
    let mut last_region: u8 = 0; // 0=zero-page, 1=TPA, 2=CCP, 3=BDOS, 4=BIOS, 5=other
    let mut transitions: Vec<(u64, u16, u8, u8)> = Vec::new(); // (count, pc, from, to)

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

        // Track region transitions
        let region = if pc < 0x0100 { 0u8 } // zero page
            else if pc < 0xE400 { 1 }       // TPA
            else if pc < 0xEC00 { 2 }       // CCP
            else if pc < 0xFA00 { 3 }      // BDOS
            else if pc < 0xFE00 { 4 }      // BIOS
            else { 5 };                    // other/unused
        if region != last_region {
            transitions.push((count, pc, last_region, region));
            last_region = region;
        }

        // Write to early trace
        if early_trace.len() < EARLY_TRACE_SIZE {
            early_trace.push((count, pc, op_bytes, a, b, c, d, e, h, l, sp, flags));
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

    // Dump early trace
    println!("\n=== FIRST {} INSTRUCTIONS ===", early_trace.len());
    for (cnt, pc, bytes, a, b, c, d, e, h, l, sp, flags) in &early_trace {
        let desc = disassemble_8080(*pc, *bytes);
        println!("{:8}: PC={:04X} {:30} A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:04X} F={:02X}",
            cnt, pc, desc, a, b, c, d, e, h, l, sp, flags);
    }

    // Dump region transitions
    let region_names = ["ZEROPAGE", "TPA     ", "CCP     ", "BDOS    ", "BIOS    ", "OTHER   "];
    println!("\n=== REGION TRANSITIONS ===");
    for (cnt, pc, from, to) in &transitions {
        println!("  {:8}: PC=0x{:04X} {} -> {}",
            cnt, pc, region_names[*from as usize], region_names[*to as usize]);
    }

    // Dump ring buffer (tail) — only non-NOP
    println!("\n=== LAST {} INSTRUCTIONS (non-NOP only) ===", ring.len().min(RING_SIZE));
    let start = if ring_full { ring_idx } else { 0 };
    let len = ring.len().min(RING_SIZE);
    let mut nop_count: u64 = 0;
    for i in 0..len {
        let idx = (start + i) % len;
        let (cnt, pc, bytes, a, b, c, d, e, h, l, sp, flags) = ring[idx];
        if bytes[0] == 0x00 {
            nop_count += 1;
            continue;
        }
        if nop_count > 0 {
            println!("  ... {} NOP instructions ...", nop_count);
            nop_count = 0;
        }
        let desc = disassemble_8080(pc, bytes);
        println!("{:8}: PC={:04X} {:30} A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:04X} F={:02X}",
            cnt, pc, desc, a, b, c, d, e, h, l, sp, flags);
    }
    if nop_count > 0 {
        println!("  ... {} NOP instructions ...", nop_count);
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

/// Hybrid test: runs at full speed with periodic display flush and I/O logging.
/// Goal: quickly determine if *any* console or disk I/O is happening.
///
/// Also serves as a "console output test" — if no OUT to video ports,
/// the problem is not in BIOS but in the program not running.
#[allow(dead_code)]
fn run_hybrid_test(emu: &mut rust_imsai_emulator::Imsai8080, max_instructions: u64) {
    println!("=== HYBRID TEST ({} instructions) ===", max_instructions);
    println!("Running at full speed with periodic flushes...");

    let mut count: u64 = 0;
    let mut video_chars: Vec<char> = Vec::new();
    let mut io_events: Vec<(u64, char, u8, u8)> = Vec::new(); // (count, dir, port, value)
    let mut last_console_out: u64 = 0;
    let mut last_disk_out: u64 = 0;
    let flush_interval: u64 = 10000;

    // Track whether we've *ever* seen output to console or disk
    let mut ever_saw_console_out: bool = false;
    let mut ever_saw_disk_out: bool = false;

    loop {
        // Step batches of 1000 and flush video buffer periodically
        for _ in 0..flush_interval {
            let pc = emu.cpu.pc;
            let op = emu.bus.memory.read(pc);

            emu.step();
            count += 1;

            // Monitor I/O
            if op == 0xD3 {
                let port = emu.bus.memory.read(pc + 1);
                let val = emu.cpu.a;
                if port == 0x00 || port == 0x01 {
                    io_events.push((count, 'O', port, val));
                    last_console_out = count;
                    if port == 0x00 && val >= 32 && val < 127 {
                        video_chars.push(val as char);
                        ever_saw_console_out = true;
                    }
                } else if (0x48..=0x4B).contains(&port) || (0xF8..0xFD).contains(&port) {
                    io_events.push((count, 'O', port, val));
                    last_disk_out = count;
                    ever_saw_disk_out = true;
                }
            } else if op == 0xDB {
                let port = emu.bus.memory.read(pc + 1);
                if (0x48..=0x4B).contains(&port) || (0xF8..0xFD).contains(&port) {
                    io_events.push((count, 'I', port, emu.cpu.a));
                }
            }

            if emu.cpu.halted || count >= max_instructions {
                break;
            }
        }

        // Flush display every flush_interval steps (or if HLT)
        let display = emu.bus.io.video.get_display_string();
        for c in display.chars().filter(|c| *c != ' ') {
            video_chars.push(c);
            ever_saw_console_out = true;
        }

        // Print I/O activity summary
        if last_console_out > 0 && count - last_console_out <= flush_interval {
            let recent_video: String = video_chars.iter().skip(video_chars.len().saturating_sub(40)).collect();
            println!("[console @ {:8}] PC=0x{:04X} A=0x{:02X} video='{}'", last_console_out, emu.cpu.pc, 
                io_events.last().map_or(0,|e|if e.1=='O' {e.3}else{0}), recent_video);
            video_chars.clear();
        }
        if last_disk_out > 0 && count - last_disk_out <= flush_interval {
            println!("[disk    @ {:8}] PC=0x{:04X}", last_disk_out, emu.cpu.pc);
        }

        if emu.cpu.halted || count >= max_instructions {
            break;
        }
    }

    println!("\n=== HYBRID TEST RESULTS ===");
    println!("Finished {} instructions at PC=0x{:04X}", count, emu.cpu.pc);
    println!("I/O events: {} ({} OUT, {} IN)", io_events.len(),
        io_events.iter().filter(|e| e.1 == 'O').count(),
        io_events.iter().filter(|e| e.1 == 'I').count());
    println!("Ever saw console output: {}", ever_saw_console_out);
    println!("Ever saw disk output: {}", ever_saw_disk_out);

    // Build final display string from captured chars
    let final_display: String = video_chars.iter().collect();
    if !final_display.is_empty() {
        println!("\nFinal display content:\n---\n{}\n---", final_display);
    } else {
        println!("\n(no visible display output captured)");
    }

    // Show first/last console OUT
    let console_outs: Vec<_> = io_events.iter().filter(|e| e.2 == 0x00 || e.2 == 0x01).collect();
    if !console_outs.is_empty() {
        println!("\nFirst console OUT: {:8} A=0x{:02X} ('{}')",
            console_outs.first().unwrap().0, console_outs.first().unwrap().2, console_outs.first().unwrap().3 as char);
        println!("Last  console OUT: {:8} A=0x{:02X} ('{}')",
            console_outs.last().unwrap().0, console_outs.last().unwrap().2, console_outs.last().unwrap().3 as char);
    } else {
        println!("\n(no console OUT detected)");
    }

    // Show first/last disk I/O (Tarbell 0x48–0x4B)
    let tarbell_outs: Vec<_> = io_events.iter().filter(|e| (0x48..=0x4B).contains(&e.2)).collect();
    if !tarbell_outs.is_empty() {
        println!("\nFirst Tarbell OUT: {:8} port=0x{:02X} A=0x{:02X}",
            tarbell_outs.first().unwrap().0, tarbell_outs.first().unwrap().2, tarbell_outs.first().unwrap().3);
        println!("Last  Tarbell OUT: {:8} port=0x{:02X} A=0x{:02X}",
            tarbell_outs.last().unwrap().0, tarbell_outs.last().unwrap().2, tarbell_outs.last().unwrap().3);
    } else {
        println!("\n(no Tarbell 0x48–0x4B OUT detected)");
    }
}
/// No terminal raw mode needed, just pure batch execution with I/O interception.
fn run_scripted(emu: &mut rust_imsai_emulator::Imsai8080, cmd: Option<&str>, max_instructions: u64) {
    // Disable video rendering (we capture console output directly)
    emu.bus.io.video.auto_render = false;

    // Pre-load keyboard with command text
    if let Some(cmd_text) = cmd {
        let input = cmd_text.replace("\\r", "\r").replace("\\n", "\n");
        emu.bus.io.keyboard.type_text(&input);
    }

    let mut output = String::new();
    let mut count: u64 = 0;
    let start_time = Instant::now();

    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.memory.read(pc);

        emu.step();
        count += 1;

        // Capture console output (port 0x00 or 0x7B OUT)
        if op == 0xD3 {
            let port = emu.bus.memory.read(pc + 1);
            if port == 0x00 || port == 0x7B {
                let ch = emu.cpu.a;
                if ch == 0x0D {
                    output.push('\r');
                } else if ch == 0x0A {
                    output.push('\n');
                } else if ch == 0x08 {
                    output.push_str("\x08 \x08");
                } else if ch >= 0x20 && ch < 0x7F {
                    output.push(ch as char);
                } else if ch < 0x20 && ch != 0x00 {
                    output.push_str(&format!("[0x{:02X}]", ch));
                }
            }
        }

        // Track disk I/O for debugging: log READ commands
        if op == 0xD3 {
            let port = emu.bus.memory.read(pc + 1);
            if port == 0x48 && emu.cpu.a == 0x80 {
                let track = emu.bus.io.tarbell.current_track();
                let sector = emu.bus.io.tarbell.current_sector();
                eprintln!("DISK READ: track={}, sector={}", track, sector);
            }
        }

        if emu.cpu.halted || count >= max_instructions {
            break;
        }
    }

    let elapsed = start_time.elapsed();
    eprintln!("Executed {} instructions in {:.2}s", count, elapsed.as_secs_f64());
    eprintln!("Final PC: 0x{:04X}", emu.cpu.pc);
    eprintln!();
    println!("=== CONSOLE OUTPUT ===");
    println!("{}", output);
    println!("=== END OUTPUT ===");

    // Quick memory dumps for debugging
    eprintln!("\n=== KEY MEMORY AREAS ===");
    dump_memory_eprint(&emu, 0x0000, 8, "Vectors");
    dump_memory_eprint(&emu, 0x0005, 3, "BDOS JMP");
    dump_memory_eprint(&emu, 0x0100, 32, "TPA (0x0100)");
}

fn dump_memory_eprint(emu: &rust_imsai_emulator::Imsai8080, start: u16, len: usize, label: &str) {
    eprint!("0x{:04X}: {} = ", start, label);
    for i in 0..len {
        if i > 0 && i % 16 == 0 {
            eprint!("\n        ");
        }
        eprint!("{:02X} ", emu.bus.memory.read(start + i as u16));
    }
    eprintln!();
}
