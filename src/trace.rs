//! Trace and diagnostic execution modes for the CLI
//!
//! Non-interactive execution modes: step trace, full trace, pc-trace,
//! verbose trace, diagnostic, batch, and scripted. Each runs the CPU
//! for a fixed number of instructions and produces debug output.

use std::time::Instant;

use rust_imsai_emulator::Imsai8080;

use crate::disasm;

/// Print a hex dump of memory to stdout.
pub fn dump_memory(emu: &Imsai8080, start: u16, len: usize, label: &str) {
    print!("\n0x{:04X}: {} = ", start, label);
    for i in 0..len {
        if i > 0 && i % 16 == 0 {
            print!("\n        ");
        }
        print!("{:02X} ", emu.bus.mem_read(start.wrapping_add(i as u16)));
    }
    println!();
}

/// Print a hex dump of memory to stderr.
pub fn dump_memory_eprint(emu: &Imsai8080, start: u16, len: usize, label: &str) {
    eprint!("0x{:04X}: {} = ", start, label);
    for i in 0..len {
        if i > 0 && i % 16 == 0 {
            eprint!("\n        ");
        }
        eprint!("{:02X} ", emu.bus.mem_read(start.wrapping_add(i as u16)));
    }
    eprintln!();
}

/// Step trace: print every instruction for the first N steps.
pub fn run_step_trace(emu: &mut Imsai8080, max: u64) {
    println!("=== STEP TRACE ({} instructions) ===", max);
    let mut count: u64 = 0;

    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.mem_read(pc);

        let desc = match op {
            0xC3 => {
                let lo = emu.bus.mem_read(pc.wrapping_add(1));
                let hi = emu.bus.mem_read(pc.wrapping_add(2));
                format!("JMP 0x{:02X}{:02X}", hi, lo)
            }
            0xCD => {
                let lo = emu.bus.mem_read(pc.wrapping_add(1));
                let hi = emu.bus.mem_read(pc.wrapping_add(2));
                format!("CALL 0x{:02X}{:02X}", hi, lo)
            }
            0xC9 => "RET".to_string(),
            0xD3 => {
                let port = emu.bus.mem_read(pc.wrapping_add(1));
                format!("OUT 0x{:02X},A=0x{:02X}", port, emu.cpu.a)
            }
            0xDB => {
                let port = emu.bus.mem_read(pc.wrapping_add(1));
                format!("IN A,0x{:02X}", port)
            }
            0x31 => {
                let lo = emu.bus.mem_read(pc.wrapping_add(1));
                let hi = emu.bus.mem_read(pc.wrapping_add(2));
                format!("LXI SP,0x{:02X}{:02X}", hi, lo)
            }
            0x00 => "NOP".to_string(),
            _ => format!("0x{:02X}", op),
        };

        println!(
            "{:6}: PC=0x{:04X} {:20} A=0x{:02X} C=0x{:02X} SP=0x{:04X}",
            count, pc, desc, emu.cpu.a, emu.cpu.c, emu.cpu.sp
        );

        emu.step();
        count += 1;

        if emu.cpu.halted || count >= max {
            break;
        }
    }

    println!(
        "\nStopped at PC=0x{:04X} after {} instructions",
        emu.cpu.pc, count
    );
}

/// Diagnostic mode: run N instructions, log I/O and PC snapshots.
pub fn run_diag(emu: &mut Imsai8080, max: u64) {
    println!("=== DIAGNOSTIC TRACE ({} instructions) ===", max);
    let mut count: u64 = 0;
    let mut io_log: Vec<(u64, u8, u8, bool)> = Vec::new();
    let mut pc_snapshots: Vec<(u64, u16)> = Vec::new();

    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.mem_read(pc);
        emu.step();
        count += 1;

        if op == 0xD3 {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            io_log.push((count, port, emu.cpu.a, true));
        } else if op == 0xDB {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
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
    println!(
        "PC=0x{:04X} SP=0x{:04X} A=0x{:02X}",
        emu.cpu.pc, emu.cpu.sp, emu.cpu.a
    );
    println!(
        "0x0000: {:02X} {:02X} {:02X}",
        emu.bus.mem_read(0),
        emu.bus.mem_read(1),
        emu.bus.mem_read(2)
    );
    println!(
        "0x0005: {:02X} {:02X} {:02X}",
        emu.bus.mem_read(5),
        emu.bus.mem_read(6),
        emu.bus.mem_read(7)
    );

    let display = emu.bus.console().video().get_display_string();
    if !display.trim().is_empty() && display.trim().chars().any(|c| c != ' ') {
        println!("\nDisplay:\n{}", display);
    } else {
        println!("\n(no display output)");
    }
}

/// Full trace: run N instructions with no per-instruction output.
pub fn run_trace(emu: &mut Imsai8080, max: u64) {
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
    let display = emu.bus.console().video().get_display_string();
    println!("\nDisplay:\n{}", display);
}

/// PC ring-buffer trace: capture early/last instructions, I/O log, region transitions.
pub fn run_pc_trace(emu: &mut Imsai8080, max: u64) {
    const RING_SIZE: usize = 8192;
    println!(
        "=== PC TRACE ({} instructions, ring={}) ===",
        max, RING_SIZE
    );

    type TraceEntry = (u64, u16, [u8; 4], u8, u8, u8, u8, u8, u8, u8, u16, u8);

    let mut ring: Vec<TraceEntry> = Vec::with_capacity(RING_SIZE);
    let mut ring_idx: usize = 0;
    let mut ring_full = false;

    let mut io_log: Vec<(u64, u8, u8, bool)> = Vec::new();
    let mut call5_count: u64 = 0;
    let mut last_call5_func: u8 = 0;

    const EARLY_TRACE_SIZE: usize = 200;
    let mut early_trace: Vec<TraceEntry> = Vec::new();

    let mut last_region: u8 = 0;
    let mut transitions: Vec<(u64, u16, u8, u8)> = Vec::new();

    let mut count: u64 = 0;
    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.mem_read(pc);

        let a = emu.cpu.a;
        let b = emu.cpu.b;
        let c = emu.cpu.c;
        let d = emu.cpu.d;
        let e = emu.cpu.e;
        let h = emu.cpu.h;
        let l = emu.cpu.l;
        let sp = emu.cpu.sp;
        let flags = emu.cpu.flags.as_psw();

        let mut op_bytes = [0u8; 4];
        for j in 0..4u16 {
            op_bytes[j as usize] = emu.bus.mem_read(pc.wrapping_add(j));
        }

        emu.step();
        count += 1;

        if op == 0xCD {
            let lo = emu.bus.mem_read(pc.wrapping_add(1));
            let hi = emu.bus.mem_read(pc.wrapping_add(2));
            let target = lo as u16 | (hi as u16) << 8;
            if target == 0x0005 {
                call5_count += 1;
                last_call5_func = c;
            }
        }

        if op == 0xD3 {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            io_log.push((count, port, a, true));
        } else if op == 0xDB {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            io_log.push((count, port, a, false));
        }

        let region = if pc < 0x0100 { 0u8 } else if pc < 0xC000 { 1 } else { 2 };
        if region != last_region {
            transitions.push((count, pc, last_region, region));
            last_region = region;
        }

        if early_trace.len() < EARLY_TRACE_SIZE {
            early_trace.push((count, pc, op_bytes, a, b, c, d, e, h, l, sp, flags));
        }

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

    println!("\n=== FIRST {} INSTRUCTIONS ===", early_trace.len());
    for (cnt, pc, bytes, a, b, c, d, e, h, l, sp, flags) in &early_trace {
        let desc = disasm::disassemble_8080(*bytes);
        println!("{:8}: PC={:04X} {:30} A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:04X} F={:02X}",
            cnt, pc, desc, a, b, c, d, e, h, l, sp, flags);
    }

    let region_names = ["ZEROPAGE", "USER    ", "HIGHRAM "];
    println!("\n=== REGION TRANSITIONS ===");
    for (cnt, pc, from, to) in &transitions {
        println!(
            "  {:8}: PC=0x{:04X} {} -> {}",
            cnt, pc, region_names[*from as usize], region_names[*to as usize]
        );
    }

    println!(
        "\n=== LAST {} INSTRUCTIONS (non-NOP only) ===",
        ring.len().min(RING_SIZE)
    );
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
        let desc = disasm::disassemble_8080(bytes);
        println!("{:8}: PC={:04X} {:30} A={:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X} SP={:04X} F={:02X}",
            cnt, pc, desc, a, b, c, d, e, h, l, sp, flags);
    }
    if nop_count > 0 {
        println!("  ... {} NOP instructions ...", nop_count);
    }

    println!("\n=== I/O LOG (all {} operations) ===", io_log.len());
    for (i, (cnt, val, port, is_out)) in io_log.iter().enumerate() {
        let dir = if *is_out { "OUT" } else { "IN " };
        let name = disasm::port_name(*port);
        if name.is_empty() {
            println!("{:5}: {:08} {:3} 0x{:02X}       A=0x{:02X}", i, cnt, dir, port, val);
        } else {
            println!("{:5}: {:08} {:3} 0x{:02X} {:10} A=0x{:02X}", i, cnt, dir, port, name, val);
        }
    }
    if io_log.is_empty() {
        println!("  (none - no IN/OUT instructions executed)");
    }

    println!("\n=== CALL 5 SUMMARY ===");
    println!("  Total CALL 5 calls: {}", call5_count);
    if call5_count > 0 {
        println!("  Last function number in C: 0x{:02X}", last_call5_func);
    }

    println!("\n=== FINAL STATE ===");
    println!(
        "PC=0x{:04X} SP=0x{:04x} A=0x{:02X} BC=0x{:02X}{:02X} DE=0x{:02X}{:02X} HL=0x{:02X}{:02X}",
        emu.cpu.pc, emu.cpu.sp, emu.cpu.a,
        emu.cpu.b, emu.cpu.c, emu.cpu.d, emu.cpu.e, emu.cpu.h, emu.cpu.l
    );
    println!(
        "0x0000: {:02X} {:02X} {:02X}   (reset vector)",
        emu.bus.mem_read(0), emu.bus.mem_read(1), emu.bus.mem_read(2)
    );
    println!(
        "0x0005: {:02X} {:02X} {:02X}   (CALL 5 vector)",
        emu.bus.mem_read(5), emu.bus.mem_read(6), emu.bus.mem_read(7)
    );
}

/// Verbose trace: show I/O instructions and console output.
pub fn run_verbose_trace(emu: &mut Imsai8080, max: u64) {
    let mut count: u64 = 0;
    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.mem_read(pc);

        emu.step();
        count += 1;

        if op == 0xD3 {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            if port == 0x00
                && (emu.cpu.a >= 32 && emu.cpu.a < 127 || emu.cpu.a == 0x0D || emu.cpu.a == 0x0A)
            {
                print!("{}", emu.cpu.a as char);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            } else if (0x48..=0x4B).contains(&port) || (0xF8..=0xFD).contains(&port) {
                if count % 500 == 0 {
                    println!("{:06}: OUT 0x{:02X},A=0x{:02X}", count, port, emu.cpu.a);
                }
            }
        } else if op == 0xDB {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
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

    println!(
        "\nStopped at PC=0x{:04X} after {} instructions",
        emu.cpu.pc, count
    );
}

/// Non-interactive batch: run N instructions, print display and memory dump.
pub fn run_interactive(emu: &mut Imsai8080, max_instructions: u64) {
    println!("IMSAI 8080 ({} instructions)", max_instructions);

    if emu.panel.is_stopped() {
        emu.panel.press_switch(rust_imsai_emulator::PanelSwitch::RunStop);
        emu.process_panel();
    }

    let mut count: u64 = 0;
    loop {
        let batch = emu.run_batch(1000.min(max_instructions - count));
        count += batch;
        if batch == 0 || count >= max_instructions {
            break;
        }
    }

    println!(
        "\nStopped at PC=0x{:04X} after {} instructions",
        emu.cpu.pc, count
    );
    let display = emu.bus.console().video().get_display_string();
    if !display.trim().is_empty() && display.trim().chars().any(|c| c != ' ') {
        println!("\nDisplay:\n{}", display);
    } else {
        println!("\n(no display output)");
    }

    println!("\n=== MEMORY DUMP ===");
    dump_memory(&emu, 0x0000, 8, "Vectors");
    dump_memory(&emu, 0x0100, 16, "TPA start");
    dump_memory(&emu, 0xFF00, 32, "High RAM");
}

/// Scripted mode: run N instructions with pre-loaded input, capture output.
pub fn run_scripted(emu: &mut Imsai8080, cmd: Option<&str>, max_instructions: u64) {
    emu.bus.console().set_auto_render(false);

    if let Some(cmd_text) = cmd {
        let input = cmd_text.replace("\\r", "\r").replace("\\n", "\n");
        emu.bus.console().type_text(&input);
    }

    let mut output = String::new();
    let mut count: u64 = 0;
    let start_time = Instant::now();

    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.mem_read(pc);

        emu.step();
        count += 1;

        // Capture console output (port 0x00 or 0x7B OUT)
        if op == 0xD3 {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
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
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            if port == 0x48 && emu.cpu.a == 0x80 {
                let track = emu.bus.tarbell().current_track();
                let sector = emu.bus.tarbell().current_sector();
                eprintln!("DISK READ: track={}, sector={}", track, sector);
            }
        }

        if emu.cpu.halted || count >= max_instructions {
            break;
        }
    }

    let elapsed = start_time.elapsed();
    eprintln!(
        "Executed {} instructions in {:.2}s",
        count,
        elapsed.as_secs_f64()
    );
    eprintln!("Final PC: 0x{:04X}", emu.cpu.pc);
    eprintln!();
    println!("=== CONSOLE OUTPUT ===");
    println!("{}", output);
    println!("=== END OUTPUT ===");

    eprintln!("\n=== KEY MEMORY AREAS ===");
    dump_memory_eprint(&emu, 0x0000, 8, "Vectors");
    dump_memory_eprint(&emu, 0x0005, 3, "CALL 5 vector");
    dump_memory_eprint(&emu, 0x0100, 32, "TPA (0x0100)");
}