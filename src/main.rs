use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};

use rust_imsai_emulator::load_memory_from_file;
use rust_imsai_emulator::save_memory_to_file;
use rust_imsai_emulator::TarbellCard;
use rust_imsai_emulator::{execute_panel_program, find_program_start, load_program_file};

fn print_usage(args: &Vec<String>) {
    eprintln!("IMSAI 8080 Emulator - Terminal Mode");
    eprintln!();
    eprintln!(
        "Usage: {} [OPTIONS]",
        args.get(0).unwrap_or(&"imsai-cli".to_string())
    );
    eprintln!();
    eprintln!("Mode (choose one):");
    eprintln!("  --load <file> [addr]      Load raw binary at address (default 0x0000)");
    eprintln!("  --program <file.json>      Load a front panel program (.json)");
    eprintln!("  (no arguments)             Start with saved memory (or empty if first run)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --disk <file>              Mount disk image in drive A");
    eprintln!("  --batch, -b                Batch mode (non-interactive, 50M instructions)");
    eprintln!("  --trace, -t                Trace every instruction");
    eprintln!("  --vtrace, -v               Verbose trace (with I/O logging)");
    eprintln!("  --diag, -d                 Diagnostic mode (I/O log + region tracking)");
    eprintln!("  --step, -s                 Step trace (first 500 instructions)");
    eprintln!("  --pctrace, -p              PC ring-buffer trace (last 8K instructions)");
    eprintln!("  --script                   Scripted mode (captures console output)");
    eprintln!("  --cmd \"text\"             Pre-load keyboard input for scripted testing");
    eprintln!("  --help, -h                 Show this help");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage(&args);
        return;
    }

    let trace = args.contains(&"--trace".to_string()) || args.contains(&"-t".to_string());
    let verbose_trace = args.contains(&"--vtrace".to_string()) || args.contains(&"-v".to_string());
    let diag = args.contains(&"--diag".to_string()) || args.contains(&"-d".to_string());
    let step_trace = args.contains(&"--step".to_string()) || args.contains(&"-s".to_string());
    let pc_trace = args.contains(&"--pctrace".to_string()) || args.contains(&"-p".to_string());
    let batch_mode = args.contains(&"--batch".to_string()) || args.contains(&"-b".to_string());

    // --cmd "DIR\r" pre-loads keyboard input for scripted testing
    let cmd_text = args
        .iter()
        .position(|a| a == "--cmd")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone());

    // --load <file> [addr]: load raw binary at address
    let load_arg = args
        .iter()
        .position(|a| a == "--load")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    // --program <file.json>: load front panel program
    let program_arg = args
        .iter()
        .position(|a| a == "--program")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    // --disk <file>: mount disk image in drive A
    let disk_arg = args
        .iter()
        .position(|a| a == "--disk")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    // --load address (optional, after the filename)
    let load_addr: u16 = if load_arg.is_some() {
        let load_pos = args.iter().position(|a| a == "--load").unwrap_or(0);
        args.get(load_pos + 2)
            .and_then(|s| {
                u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let mut emu = rust_imsai_emulator::Imsai8080::new();
    let mut start_pc: Option<u16> = None;

    if let Some(ref path) = program_arg {
        // Load a front panel program (.json)
        let pbuf = PathBuf::from(path);
        match load_program_file(&pbuf) {
            Ok(prog) => {
                let start = find_program_start(&prog).unwrap_or(0);
                eprintln!("Loaded program: {} (start at 0x{:04X})", prog.name, start);
                execute_panel_program(&mut emu, &prog);
                start_pc = Some(start);
            }
            Err(e) => {
                eprintln!("Error loading program '{}': {}", path, e);
                return;
            }
        }
    } else if let Some(ref path) = load_arg {
        // Load a raw binary file at an address
        match std::fs::read(path) {
            Ok(data) => {
                eprintln!(
                    "Loaded {} bytes from {} at 0x{:04X}",
                    data.len(),
                    path,
                    load_addr
                );
                emu.load_program(load_addr, &data);
                start_pc = Some(load_addr);
            }
            Err(e) => {
                eprintln!("Error loading '{}': {}", path, e);
                return;
            }
        }
    } else {
        // No program/load specified: restore saved memory if it exists
        let mem_path = std::path::Path::new("imsai_memory.json");
        if mem_path.exists() {
            match load_memory_from_file(&mut emu.bus.memory().ram, mem_path) {
                Ok(()) => eprintln!("Restored memory from {}", mem_path.display()),
                Err(e) => eprintln!("Warning: failed to load {}: {}", mem_path.display(), e),
            }
        } else {
            eprintln!("IMSAI 8080 - Bare-metal mode (empty memory, no program loaded)");
            eprintln!("Use --load <file> or --program <file.json> to load a program.");
        }
    }

    // Mount disk image if specified (orthogonal to program/load)
    if let Some(ref path) = disk_arg {
        match emu
            .bus
            .card_mut::<TarbellCard>()
            .expect("Tarbell card")
            .insert_disk(0, path)
        {
            Ok(()) => eprintln!("Disk mounted in drive A: {}", path),
            Err(e) => {
                eprintln!("Error mounting disk '{}': {}", path, e);
                return;
            }
        }
    }

    // Set start PC if we loaded something
    if let Some(pc) = start_pc {
        emu.cpu.pc = pc;
    }

    // Pre-load keyboard with --cmd text (convert escape sequences)
    if let Some(ref cmd) = cmd_text {
        let input = cmd.replace("\\r", "\r").replace("\\n", "\n");
        emu.bus.console().type_text(&input);
    }

    // Could use match here but it's a bit more verbose
    if step_trace {
        run_step_trace(&mut emu, 500);
    } else if diag {
        run_diag(&mut emu, 50_000);
    } else if pc_trace {
        run_pc_trace(&mut emu, 5_000_000);
    } else if verbose_trace {
        run_verbose_trace(&mut emu, 200_000);
    } else if trace {
        run_trace(&mut emu, 50_000);
    } else if args.contains(&"--script".to_string()) {
        run_scripted(&mut emu, cmd_text.as_deref(), 100_000_000);
    } else if batch_mode {
        run_interactive(&mut emu, 50_000_000);
    } else {
        run_terminal(&mut emu);
    }

    // Save memory state on exit
    let mem_path = std::path::Path::new("imsai_memory.json");
    match save_memory_to_file(&emu.bus.memory().ram, mem_path) {
        Ok(()) => eprintln!("Memory saved to {}", mem_path.display()),
        Err(e) => eprintln!("Warning: failed to save {}: {}", mem_path.display(), e),
    }
}

fn run_step_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
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

fn run_diag(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
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
        println!(
            "{:5}: {:08} {:3} 0x{:02X} A=0x{:02X}",
            i, cnt, dir, port, val
        );
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

/// Helper: save memory to imsai_memory.json
fn save_memory(emu: &mut rust_imsai_emulator::Imsai8080) {
    let mem_path = std::path::Path::new("imsai_memory.json");
    match save_memory_to_file(&emu.bus.memory().ram, mem_path) {
        Ok(()) => eprintln!("Memory saved to {}", mem_path.display()),
        Err(e) => eprintln!("Warning: failed to save {}: {}", mem_path.display(), e),
    }
}

/// Interactive terminal mode: TUI with 80x24 CRT display and status bar.
fn run_terminal(emu: &mut rust_imsai_emulator::Imsai8080) {
    let mut program_name = String::new();
    if enable_raw_mode().is_err() {
        eprintln!("No TTY available, falling back to batch mode (use --batch to force)");
        run_interactive(emu, 50_000_000);
        return;
    }

    emu.bus.console().set_auto_render(false);

    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .expect("Failed to enter alternate screen");
    // Clear the alternate screen and home the cursor so no stale content
    // (or rows below our compact panel) lingers.
    print!("\x1B[2J\x1B[H");
    stdout.flush().ok();

    // If no program loaded, enter command mode immediately
    if emu.bus.mem_read(0x0000) == 0xFF && emu.cpu.pc == 0x0000 {
        run_command_modal(emu, &mut stdout, &mut program_name);
        if emu.bus.mem_read(0x0000) == 0xFF && emu.cpu.pc == 0x0000 {
            println!("\r\nNo program loaded. Use --load, --program, or Ctrl+K to load one.");
            stdout.execute(LeaveAlternateScreen).ok();
            disable_raw_mode().ok();
            return;
        }
    }

    let batch_size: u64 = 5000;
    let idle_sleep = std::time::Duration::from_millis(5);
    let poll_timeout = std::time::Duration::from_millis(0);
    let mut instruction_count: u64 = 0;
    let mut idle_count: u64 = 0;
    let mut last_display: String = String::new();
    // RUN/STOP state, toggled by F5 (mirrors the GUI's F5 run/stop). The CLI
    // auto-runs a loaded program, so we start RUNNING.
    let mut running = true;
    let start_time = Instant::now();

    loop {
        // Run a batch of CPU instructions, but only while in RUN state and the
        // CPU hasn't executed HLT.
        if running && !emu.cpu.halted {
            for _ in 0..batch_size {
                emu.step();
                instruction_count += 1;
                // poll_rx (not service_uart): inject keyboard input AND drain
                // the UART TX output buffer into the VideoDisplay so it
                // actually shows on the CRT. service_uart only manages TX
                // flags and leaves the output buffered, which left the screen
                // blank.
                emu.bus.serial().poll_rx();

                if emu.cpu.halted {
                    break;
                }
            }
        }

        // Render the display after each batch (or when halted). The command
        // modal is a synchronous blocking call, so the main loop is never
        // here while it owns the screen.
        let display = emu.bus.console().video().get_display_string();
        if display != last_display || emu.cpu.halted {
            last_display = display.clone();
            // crossterm returns (columns, rows).
            let term_size = crossterm::terminal::size().unwrap_or((80, 24));
            let term_cols = term_size.0 as usize;
            let term_rows = term_size.1 as usize;
            let display_lines: Vec<&str> = display.trim_end_matches('\n').lines().collect();

            // The CRT is a fixed-height block (the VideoDisplay buffer, 24
            // rows) anchored at the top. It scrolls internally, so its rows
            // already are the visible screen -- we render them top-aligned,
            // not "the bottom N lines". On a short terminal we clamp so the
            // status bar still fits.
            let crt_rows = display_lines.len().min(term_rows.saturating_sub(1));

            // Paint each line at an absolute row/column. We can't rely on
            // "\n" to wrap to the next line's column 0: raw mode clears
            // OPOST, so LF is a bare line-feed with no carriage return and
            // the lines would staircase diagonally across the screen.
            let mut row: u16 = 1;
            for line in display_lines.iter().take(crt_rows) {
                let truncated: String = line.chars().take(term_cols).collect();
                print!("\x1B[{};1H{}\x1B[K", row, truncated);
                row += 1;
            }

            // Status bar (inverted video) directly below the CRT block.
            render_bottom_row(emu, &program_name, BottomMode::Status, running, "", None);
            stdout.flush().ok();
        }

        // Keyboard input
        let mut got_key = false;
        while event::poll(poll_timeout).unwrap_or(false) {
            if let Ok(ev) = event::read() {
                match ev {
                    Event::Key(key_event) => {
                        match key_event {
                            KeyEvent {
                                code: KeyCode::Esc, ..
                            } => {
                                emu.bus.console().type_text("\x1B");
                                got_key = true;
                            }
                            // F5 = start/stop the computer (RUN/STOP toggle),
                            // matching the GUI's F5. Freezes/resumes the CPU in
                            // place; force a repaint so the status bar updates.
                            KeyEvent {
                                code: KeyCode::F(5),
                                ..
                            } => {
                                running = !running;
                                last_display.clear();
                                got_key = true;
                            }
                            // Ctrl+D = graceful shutdown (save memory, restore
                            // terminal). crossterm delivers Ctrl+D as
                            // Char('d')+CONTROL.
                            KeyEvent {
                                code: KeyCode::Char('d'),
                                modifiers: KeyModifiers::CONTROL,
                                ..
                            } => {
                                print!("\r\n\r\n--- Ctrl+D pressed, shutting down ---\r\n");
                                stdout.flush().ok();
                                stdout.execute(LeaveAlternateScreen).ok();
                                disable_raw_mode().ok();
                                let elapsed = start_time.elapsed();
                                print_instructions_summary(instruction_count, elapsed);
                                save_memory(emu);
                                return;
                            }
                            KeyEvent {
                                code: KeyCode::Char('k'),
                                modifiers: KeyModifiers::CONTROL,
                                ..
                            } => {
                                // A run-type command (load/program/go) resumes
                                // execution even if we were stopped (F5) before.
                                if run_command_modal(emu, &mut stdout, &mut program_name) {
                                    running = true;
                                }
                                last_display.clear();
                                got_key = true;
                            }
                            KeyEvent {
                                code: KeyCode::Char(ch),
                                modifiers: KeyModifiers::CONTROL,
                                ..
                            } => {
                                let ctrl_ch = (ch as u8) & 0x1F;
                                if ctrl_ch != 0 {
                                    emu.bus
                                        .serial()
                                        .type_text(&String::from_utf8_lossy(&[ctrl_ch]));
                                    got_key = true;
                                }
                            }
                            KeyEvent {
                                code: KeyCode::Char(ch),
                                ..
                            } => {
                                let upper: String = ch.to_uppercase().collect();
                                emu.bus.serial().type_text(&upper);
                                got_key = true;
                            }
                            KeyEvent {
                                code: KeyCode::Enter,
                                ..
                            } => {
                                emu.bus.serial().type_text("\r");
                                got_key = true;
                            }
                            KeyEvent {
                                code: KeyCode::Backspace,
                                ..
                            } => {
                                emu.bus.serial().type_text("\x7F");
                                got_key = true;
                            }
                            KeyEvent {
                                code:
                                    KeyCode::F(_)
                                    | KeyCode::Null
                                    | KeyCode::CapsLock
                                    | KeyCode::ScrollLock
                                    | KeyCode::NumLock
                                    | KeyCode::PrintScreen
                                    | KeyCode::Pause
                                    | KeyCode::KeypadBegin
                                    | KeyCode::Media(_)
                                    | KeyCode::Modifier(_),
                                ..
                            } => {}
                            _ => {}
                        }
                    }
                    Event::Resize(_, _) => {
                        last_display.clear(); // force re-render
                    }
                    Event::Mouse(_) | Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
                }
            }
        }

        if got_key {
            idle_count = 0;
        } else {
            idle_count += 1;
        }

        if idle_count > 3 {
            std::thread::sleep(idle_sleep);
            emu.bus.serial().poll_rx();
        }
    }
}
/// Terminal row where the status bar / command prompt is anchored: the very
/// bottom row of the terminal. The CRT block is painted at the top; the status
/// line lives at the bottom (standard TUI layout), with any space between left
/// blank.
fn status_anchor_row(term_rows: u16) -> u16 {
    term_rows.max(1)
}

/// Render the status/prompt area of the TUI. `mode` picks what's shown:
/// - `BottomMode::Status` — the normal inverted-video status bar.
/// - `BottomMode::Prompt` — a `> ` prompt with the in-progress command.
///
/// Both anchor on the row directly below the CRT block (see
/// `status_anchor_row`) rather than the absolute bottom of the terminal, so
/// the panel stays compact regardless of how tall the terminal window is.
fn render_bottom_row(
    emu: &rust_imsai_emulator::Imsai8080,
    program_name: &str,
    mode: BottomMode,
    running: bool,
    prompt_buf: &str,
    last_message: Option<&str>,
) {
    // crossterm returns (columns, rows).
    let term_size = crossterm::terminal::size().unwrap_or((80, 24));
    let term_cols = term_size.0 as usize;
    let anchor = status_anchor_row(term_size.1);

    match mode {
        BottomMode::Status => {
            // HALT (executed HLT) > STOP (F5 paused) > RUN.
            let state = if emu.cpu.halted {
                "HALT"
            } else if running {
                "RUN "
            } else {
                "STOP"
            };
            let status = format!(
                " {} PC:{:04X} SP:{:04X} A:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X}  {}  F5=run/stop  Ctrl+K=cmd  Ctrl+D=exit ",
                state,
                emu.cpu.pc, emu.cpu.sp, emu.cpu.a,
                emu.cpu.b, emu.cpu.c, emu.cpu.d, emu.cpu.e, emu.cpu.h, emu.cpu.l,
                program_name
            );
            let truncated: String = status.chars().take(term_cols).collect();
            print!("\x1B[{};1H\x1B[2K\x1B[7m{}\x1B[0m", anchor, truncated);
        }
        BottomMode::Prompt => {
            // The prompt sits on the anchor row; the result message (if any)
            // goes on the row directly above it. On a 24-row terminal that
            // message row is the last CRT row -- fine, it gets repainted when
            // the modal closes (the caller clears last_display).
            let prompt_row = anchor;
            let message_row = anchor.saturating_sub(1).max(1);
            if let Some(msg) = last_message {
                let truncated: String = msg.chars().take(term_cols).collect();
                print!("\x1B[{};1H\x1B[2K{}", message_row, truncated);
            }
            let line = format!("> {}", prompt_buf);
            let truncated: String = line.chars().take(term_cols.saturating_sub(1)).collect();
            print!("\x1B[{};1H\x1B[2K{}", prompt_row, truncated);
        }
    }
}

#[derive(Copy, Clone)]
enum BottomMode {
    Status,
    Prompt,
}

/// In-TUI command modal. Keeps the alternate screen + raw mode active, shows
/// a `> ` prompt on the bottom row of the TUI, runs a command, and returns
/// when the user hits Esc (or enters an empty line after at least one command).
/// The CPU is paused for the duration of the modal.
///
/// Returns `true` if a run-type command (`load`/`program`/`go`) executed, so
/// the caller should resume execution (set RUN state). Returns `false` if the
/// modal was just dismissed (Esc/Ctrl+K/Ctrl+D, or a non-run command).
fn run_command_modal(
    emu: &mut rust_imsai_emulator::Imsai8080,
    stdout: &mut io::Stdout,
    program_name: &mut String,
) -> bool {
    // Force a redraw of the CRT area above the bottom row so the screen
    // looks fresh before we show the prompt. The caller's `last_display`
    // tracking will pick up changes naturally on the next loop iteration.
    let mut input = String::new();
    let mut last_message: Option<String> = None;
    let mut ever_ran = false;
    let mut resume = false;

    // Initial paint: blank the prompt row, leave the row above for messages.
    render_bottom_row(emu, program_name, BottomMode::Prompt, false, &input, None);
    stdout.flush().ok();

    loop {
        if !event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
            continue;
        }
        let ev = match event::read() {
            Ok(ev) => ev,
            Err(_) => break,
        };
        let key = match ev {
            Event::Key(k) => k,
            Event::Resize(_, _) => continue, // caller's loop will re-render
            _ => continue,
        };

        match key {
            // Esc closes the modal. If the user typed something but didn't
            // submit it, drop it silently (matches less/vim convention).
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                break;
            }
            // Ctrl+K also closes — symmetric with the way it opens.
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                break;
            }
            // Ctrl+D closes the modal here (use the `quit` command, or Ctrl+D
            // at the main TUI, to shut the emulator down).
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                break;
            }
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => {
                let cmd = input.trim().to_string();
                input.clear();
                if cmd.is_empty() {
                    // Empty Enter: close if we've already run something,
                    // otherwise just stay open.
                    if ever_ran {
                        break;
                    }
                    render_bottom_row(emu, program_name, BottomMode::Prompt, false, &input, None);
                    stdout.flush().ok();
                    continue;
                }
                let result = run_command(emu, &cmd, program_name);
                if result.quit {
                    // Cleanup TUI state before exiting so the user's terminal
                    // isn't left in alt-screen + raw mode.
                    stdout.execute(LeaveAlternateScreen).ok();
                    disable_raw_mode().ok();
                    eprintln!("{}", result.message);
                    std::process::exit(0);
                }
                last_message = Some(result.message.clone());
                ever_ran = true;
                render_bottom_row(
                    emu,
                    program_name,
                    BottomMode::Prompt,
                    false,
                    &input,
                    last_message.as_deref(),
                );
                stdout.flush().ok();
                if result.close_after {
                    // A run-type command (load/program/go): resume execution
                    // when the modal closes so the program actually runs even
                    // if we were stopped (e.g. F5) before opening the modal.
                    resume = true;
                    // No sleep: the message is on screen for one frame, then
                    // the main loop reclaims the row. Fast enough to feel
                    // responsive.
                    break;
                }
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                input.pop();
                render_bottom_row(
                    emu,
                    program_name,
                    BottomMode::Prompt,
                    false,
                    &input,
                    last_message.as_deref(),
                );
                stdout.flush().ok();
            }
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::SHIFT,
                ..
            } => {
                input.push(c);
                render_bottom_row(
                    emu,
                    program_name,
                    BottomMode::Prompt,
                    false,
                    &input,
                    last_message.as_deref(),
                );
                stdout.flush().ok();
            }
            _ => {}
        }
    }

    // Clear the modal's rows (prompt + the message row above it). The caller
    // forces a CRT/status repaint, but on a tall terminal the message row sits
    // in the blank gap below the CRT, which neither the CRT nor the status bar
    // would otherwise overwrite.
    let term_rows = crossterm::terminal::size().unwrap_or((80, 24)).1;
    let anchor = status_anchor_row(term_rows);
    print!("\x1B[{};1H\x1B[2K", anchor.saturating_sub(1).max(1));
    print!("\x1B[{};1H\x1B[2K", anchor);
    stdout.flush().ok();

    resume
}

/// Result of executing one command in the TUI modal.
struct CommandResult {
    /// One-line human-readable result, shown on the row above the prompt.
    message: String,
    /// If true, the modal loop should break after showing the message.
    /// Used by `go` (resume execution and return to the TUI) so the
    /// user sees the result before the modal closes.
    close_after: bool,
    /// If true, the process should exit. The modal handles the TUI cleanup
    /// (leave alternate screen, disable raw mode) and then `std::process::exit`s.
    quit: bool,
}

/// Reset the console hardware for a freshly launched program, mirroring a
/// front-panel RESET. Resets both 8251A UART channels back to power-on
/// (ExpectMode) state and clears the CRT.
///
/// Without the UART reset, a program that re-initializes the 8251A
/// (mode byte, then command byte) is misread when the chip is still in the
/// previous program's `Ready` state: the mode byte is taken as a command, and
/// because it carries the internal-reset bit the following command byte is
/// then taken as a mode -- leaving TX disabled, so the program's output is
/// silently dropped.
fn reset_console_for_new_program(emu: &mut rust_imsai_emulator::Imsai8080) {
    emu.bus.serial().channel_a_mut().reset();
    emu.bus.serial().channel_b_mut().reset();
    emu.bus.serial().video_mut().clear();
}

/// Execute a single command string. Returns a `CommandResult` describing
/// what to display and whether the modal should close.
fn run_command(
    emu: &mut rust_imsai_emulator::Imsai8080,
    input: &str,
    program_name: &mut String,
) -> CommandResult {
    let parts: Vec<&str> = input.splitn(3, ' ').collect();
    let close = |msg: String| CommandResult {
        message: msg,
        close_after: true,
        quit: false,
    };
    let stay = |msg: String| CommandResult {
        message: msg,
        close_after: false,
        quit: false,
    };
    match parts[0].to_lowercase().as_str() {
        "load" => {
            let path = match parts.get(1) {
                Some(p) => *p,
                None => return stay("Usage: load <file> [addr]".to_string()),
            };
            let addr: u16 = parts
                .get(2)
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok())
                .unwrap_or(0);
            match std::fs::read(path) {
                Ok(data) => {
                    reset_console_for_new_program(emu);
                    emu.load_program(addr, &data);
                    emu.cpu.pc = addr;
                    emu.cpu.halted = false;
                    *program_name = format!("{} @ {:04X}", path, addr);
                    // Close the modal so the TUI's main loop resumes
                    // execution and the program's output appears on the CRT.
                    close(format!("Loaded {} bytes at {:04X}, running...", data.len(), addr))
                }
                Err(e) => stay(format!("Error loading '{}': {}", path, e)),
            }
        }
        "mount" => {
            let path = match parts.get(1) {
                Some(p) => *p,
                None => return stay("Usage: mount <disk.img>".to_string()),
            };
            match emu
                .bus
                .card_mut::<TarbellCard>()
                .expect("Tarbell card")
                .insert_disk(0, path)
            {
                Ok(()) => stay(format!("Disk mounted in drive A: {}", path)),
                Err(e) => stay(format!("Error mounting '{}': {}", path, e)),
            }
        }
        "program" => {
            let path = match parts.get(1) {
                Some(p) => *p,
                None => return stay("Usage: program <file.json>".to_string()),
            };
            let pbuf = PathBuf::from(path);
            match load_program_file(&pbuf) {
                Ok(prog) => {
                    let start = find_program_start(&prog).unwrap_or(0);
                    reset_console_for_new_program(emu);
                    execute_panel_program(emu, &prog);
                    emu.cpu.pc = start;
                    emu.cpu.halted = false;
                    *program_name = prog.name.clone();
                    // Close the modal so the TUI's main loop resumes and the
                    // program actually runs (matching the `load` command).
                    // Without this the program sits loaded-but-paused and the
                    // screen stays blank.
                    close(format!("Running {} (PC={:04X})...", prog.name, start))
                }
                Err(e) => stay(format!("Error: {}", e)),
            }
        }
        "go" | "run" => {
            emu.cpu.halted = false;
            // Resume execution and return to the TUI so the user can see it run.
            close("Resuming execution. Press Ctrl+K to pause.".to_string())
        }
        "reset" => {
            *emu = rust_imsai_emulator::Imsai8080::new();
            *program_name = String::new();
            stay("Cold reset. Memory cleared, CPU at 0x0000.".to_string())
        }
        "status" => {
            stay(format!(
                "PC={:04X} SP={:04X} A={:02X} B={:02X} C={:02X} D={:02X} E={:02X} H={:02X} L={:02X} {}",
                emu.cpu.pc, emu.cpu.sp, emu.cpu.a,
                emu.cpu.b, emu.cpu.c, emu.cpu.d, emu.cpu.e, emu.cpu.h, emu.cpu.l,
                if emu.cpu.halted { "HALT" } else { "RUN" }
            ))
        }
        "quit" | "exit" => {
            // Save and signal the modal to exit the process. The modal
            // handles the TUI cleanup (leave alternate screen, disable raw
            // mode) before calling `std::process::exit`.
            let mem_path = std::path::Path::new("imsai_memory.json");
            match save_memory_to_file(&emu.bus.memory().ram, mem_path) {
                Ok(()) => eprintln!("Memory saved to {}", mem_path.display()),
                Err(e) => eprintln!("Warning: failed to save {}: {}", mem_path.display(), e),
            }
            CommandResult {
                message: "Exiting...".to_string(),
                close_after: true,
                quit: true,
            }
        }
        "help" | "?" => {
            stay("Commands: load <file> [addr], mount <disk.img>, program <file.json>, go, reset, status, quit".to_string())
        }
        _ => stay("Unknown command. Type 'help' for the list.".to_string()),
    }
}

fn print_instructions_summary(count: u64, elapsed: std::time::Duration) {
    let secs = elapsed.as_secs_f64();
    let ips = if secs > 0.0 { count as f64 / secs } else { 0.0 };
    eprintln!(
        "Executed {} instructions in {:.2}s ({:.0} ips)",
        count, secs, ips
    );
}

fn run_interactive(emu: &mut rust_imsai_emulator::Imsai8080, max_instructions: u64) {
    println!("IMSAI 8080 ({} instructions)", max_instructions);

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

    // Memory dump for debugging
    println!("\n=== MEMORY DUMP ===");
    dump_memory(&emu, 0x0000, 8, "Vectors");
    dump_memory(&emu, 0x0100, 16, "TPA start");
    dump_memory(&emu, 0xFF00, 32, "High RAM");
}

fn dump_memory(emu: &rust_imsai_emulator::Imsai8080, start: u16, len: usize, label: &str) {
    print!("\n0x{:04X}: {} = ", start, label);
    for i in 0..len {
        if i > 0 && i % 16 == 0 {
            print!("\n        ");
        }
        print!("{:02X} ", emu.bus.mem_read(start.wrapping_add(i as u16)));
    }
    println!();
}

fn run_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    println!(
        "Tracing {} instructions from PC=0x{:04X}...",
        max, emu.cpu.pc
    );
    let mut count: u64 = 0;
    loop {
        emu.step();
        count += 1;
        if emu.cpu.halted || count >= max {
            break;
        }
    }
    println!(
        "Stopped at PC=0x{:04X} after {} instructions",
        emu.cpu.pc, count
    );
    let display = emu.bus.console().video().get_display_string();
    println!("\nDisplay:\n{}", display);
}
fn run_pc_trace(emu: &mut rust_imsai_emulator::Imsai8080, max: u64) {
    const RING_SIZE: usize = 8192;
    println!(
        "=== PC TRACE ({} instructions, ring={}) ===",
        max, RING_SIZE
    );

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
    let mut last_region: u8 = 0; // 0=zero-page, 1=user code, 2=high RAM
    let mut transitions: Vec<(u64, u16, u8, u8)> = Vec::new(); // (count, pc, from, to)

    let mut count: u64 = 0;
    loop {
        let pc = emu.cpu.pc;
        let op = emu.bus.mem_read(pc);

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
            op_bytes[j as usize] = emu.bus.mem_read(pc.wrapping_add(j));
        }

        emu.step();
        count += 1;

        // Detect CALL 5
        if op == 0xCD {
            let lo = emu.bus.mem_read(pc.wrapping_add(1));
            let hi = emu.bus.mem_read(pc.wrapping_add(2));
            let target = lo as u16 | (hi as u16) << 8;
            if target == 0x0005 {
                call5_count += 1;
                last_call5_func = c; // function number is in C
            }
        }

        // Detect I/O
        if op == 0xD3 {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            io_log.push((count, port, a, true));
        } else if op == 0xDB {
            let port = emu.bus.mem_read(pc.wrapping_add(1));
            io_log.push((count, port, a, false));
        }

        // Track region transitions
        let region = if pc < 0x0100 {
            0u8
        }
        // zero page
        else if pc < 0xC000 {
            1
        }
        // user code
        else {
            2
        }; // high RAM
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
    let region_names = ["ZEROPAGE", "USER    ", "HIGHRAM "];
    println!("\n=== REGION TRANSITIONS ===");
    for (cnt, pc, from, to) in &transitions {
        println!(
            "  {:8}: PC=0x{:04X} {} -> {}",
            cnt, pc, region_names[*from as usize], region_names[*to as usize]
        );
    }

    // Dump ring buffer (tail) — only non-NOP
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
            println!(
                "{:5}: {:08} {:3} 0x{:02X}       A=0x{:02X}",
                i, cnt, dir, port, val
            );
        } else {
            println!(
                "{:5}: {:08} {:3} 0x{:02X} {:10} A=0x{:02X}",
                i, cnt, dir, port, port_name, val
            );
        }
    }
    if io_log.is_empty() {
        println!("  (none — no IN/OUT instructions executed)");
    }

    // Dump CALL 5 summary
    println!("\n=== CALL 5 SUMMARY ===");
    println!("  Total CALL 5 calls: {}", call5_count);
    if call5_count > 0 {
        println!("  Last function number in C: 0x{:02X}", last_call5_func);
    }

    // Final state
    println!("\n=== FINAL STATE ===");
    println!(
        "PC=0x{:04X} SP=0x{:04x} A=0x{:02X} BC=0x{:02X}{:02X} DE=0x{:02X}{:02X} HL=0x{:02X}{:02X}",
        emu.cpu.pc,
        emu.cpu.sp,
        emu.cpu.a,
        emu.cpu.b,
        emu.cpu.c,
        emu.cpu.d,
        emu.cpu.e,
        emu.cpu.h,
        emu.cpu.l
    );
    println!(
        "0x0000: {:02X} {:02X} {:02X}   (reset vector)",
        emu.bus.mem_read(0),
        emu.bus.mem_read(1),
        emu.bus.mem_read(2)
    );
    println!(
        "0x0005: {:02X} {:02X} {:02X}   (CALL 5 vector)",
        emu.bus.mem_read(5),
        emu.bus.mem_read(6),
        emu.bus.mem_read(7)
    );
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
            let names = ["ADD", "ADC", "SUB", "SBB", "ANA", "XRA", "ORA", "CMP"];
            let reg = op & 0x07;
            let op_name = names[((op >> 3) & 7) as usize];
            let reg_name = ["B", "C", "D", "E", "H", "L", "M", "A"][reg as usize];
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
                let reg_names = ["B", "C", "D", "E", "H", "L", "M", "A"];
                format!(
                    "MOV {},{}",
                    reg_names[dst as usize], reg_names[src as usize]
                )
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

/// No terminal raw mode needed, just pure batch execution with I/O interception.
fn run_scripted(
    emu: &mut rust_imsai_emulator::Imsai8080,
    cmd: Option<&str>,
    max_instructions: u64,
) {
    // Disable video rendering (we capture console output directly)
    emu.bus.console().set_auto_render(false);

    // Pre-load keyboard with command text
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

    // Quick memory dumps for debugging
    eprintln!("\n=== KEY MEMORY AREAS ===");
    dump_memory_eprint(&emu, 0x0000, 8, "Vectors");
    dump_memory_eprint(&emu, 0x0005, 3, "CALL 5 vector");
    dump_memory_eprint(&emu, 0x0100, 32, "TPA (0x0100)");
}

fn dump_memory_eprint(emu: &rust_imsai_emulator::Imsai8080, start: u16, len: usize, label: &str) {
    eprint!("0x{:04X}: {} = ", start, label);
    for i in 0..len {
        if i > 0 && i % 16 == 0 {
            eprint!("\n        ");
        }
        eprint!("{:02X} ", emu.bus.mem_read(start.wrapping_add(i as u16)));
    }
    eprintln!();
}
