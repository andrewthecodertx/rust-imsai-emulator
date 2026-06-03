//! Terminal UI (TUI) for the IMSAI 8080 CLI
//!
//! Provides the interactive 80x24 CRT display, status bar, and in-TUI
//! command modal (Ctrl+K). Uses crossterm for terminal raw mode and
//! event handling.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Instant;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};

use rust_imsai_emulator::save_memory_to_file;
use rust_imsai_emulator::{execute_panel_program, find_program_start, load_program_file};

/// Which content to show on the bottom row of the TUI.
#[derive(Copy, Clone)]
enum BottomMode {
    Status,
    Prompt,
}

/// Result of executing a TUI command.
pub struct CommandResult {
    /// Close the modal and return to the main loop?
    pub close: bool,
    /// Did a run-type command (load/program/go) execute?
    pub ran: bool,
    /// Status message to display.
    pub message: String,
}

/// Save memory to imsai_memory.json
fn save_memory(emu: &mut rust_imsai_emulator::Imsai8080) {
    let mem_path = std::path::Path::new("imsai_memory.json");
    match save_memory_to_file(&emu.bus.memory.ram, mem_path) {
        Ok(()) => eprintln!("Memory saved to {}", mem_path.display()),
        Err(e) => eprintln!("Warning: failed to save {}: {}", mem_path.display(), e),
    }
}

/// Reset console UART for a new program.
fn reset_console_for_new_program(emu: &mut rust_imsai_emulator::Imsai8080) {
    emu.bus.console().video_mut().clear();
    emu.bus.console().set_auto_render(false);
}

/// Calculate the row where the status/prompt bar anchors.
fn status_anchor_row(term_rows: u16) -> u16 {
    term_rows.max(1)
}

/// Render the bottom row of the TUI (status bar or command prompt).
fn render_bottom_row(
    emu: &rust_imsai_emulator::Imsai8080,
    program_name: &str,
    mode: BottomMode,
    running: bool,
    prompt_buf: &str,
    last_message: Option<&str>,
) {
    let term_size = crossterm::terminal::size().unwrap_or((80, 24));
    let term_cols = term_size.0 as usize;
    let anchor = status_anchor_row(term_size.1);

    match mode {
        BottomMode::Status => {
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

/// In-TUI command modal. Returns true if a run-type command executed.
fn run_command_modal(
    emu: &mut rust_imsai_emulator::Imsai8080,
    stdout: &mut io::Stdout,
    program_name: &mut String,
) -> bool {
    let mut input = String::new();
    let mut last_message: Option<String> = None;
    let mut ever_ran = false;

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
            Event::Resize(_, _) => continue,
            _ => continue,
        };

        match key {
            KeyEvent { code: KeyCode::Esc, .. } => break,
            KeyEvent { code: KeyCode::Char('k'), modifiers: KeyModifiers::CONTROL, .. } => break,
            KeyEvent { code: KeyCode::Char('d'), modifiers: KeyModifiers::CONTROL, .. } => break,
            KeyEvent { code: KeyCode::Enter, .. } => {
                let result = run_command(emu, &input, program_name);
                if result.close {
                    return result.ran;
                }
                last_message = Some(result.message.clone());
                ever_ran = ever_ran || result.ran;
                input.clear();
                render_bottom_row(emu, program_name, BottomMode::Prompt, false, &input, last_message.as_deref());
                stdout.flush().ok();
            }
            KeyEvent { code: KeyCode::Backspace, .. } => {
                input.pop();
                render_bottom_row(emu, program_name, BottomMode::Prompt, false, &input, last_message.as_deref());
                stdout.flush().ok();
            }
            KeyEvent { code: KeyCode::Char(ch), modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT, .. } => {
                input.push(ch);
                render_bottom_row(emu, program_name, BottomMode::Prompt, false, &input, last_message.as_deref());
                stdout.flush().ok();
            }
            _ => {}
        }
    }

    ever_ran
}

/// Execute a single command string. Returns a CommandResult.
fn run_command(
    emu: &mut rust_imsai_emulator::Imsai8080,
    input: &str,
    program_name: &mut String,
) -> CommandResult {
    let input = input.trim();
    if input.is_empty() {
        return CommandResult { close: false, ran: false, message: String::new() };
    }

    let parts: Vec<&str> = input.splitn(3, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let close = |msg: String| CommandResult { close: true, ran: false, message: msg };
    let stay = |msg: String| CommandResult { close: false, ran: false, message: msg };

    match cmd.as_str() {
        "help" | "?" => stay(format!("Commands: load <file> [addr]  program <file.json>  mount <file>  go  run  reset  quit")),
        "load" => {
            let path = match parts.get(1) {
                Some(p) => *p,
                None => return stay("Usage: load <file> [addr]".to_string()),
            };
            let addr: u16 = parts
                .get(2)
                .and_then(|s| {
                    u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
                })
                .unwrap_or(0);
            match std::fs::read(path) {
                Ok(data) => {
                    emu.load_program(addr, &data);
                    emu.cpu.pc = addr;
                    emu.cpu.halted = false;
                    *program_name = path.to_string();
                    close(format!("Loaded {} bytes at 0x{:04X}", data.len(), addr))
                }
                Err(e) => stay(format!("Error: {}", e)),
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
                    match execute_panel_program(emu, &prog) {
                        Ok(()) => {
                            emu.cpu.pc = start;
                            emu.cpu.halted = false;
                            *program_name = prog.name.clone();
                            close(format!("Running {} (PC={:04X})...", prog.name, start))
                        }
                        Err(e) => stay(format!("Program error: {}", e)),
                    }
                }
                Err(e) => stay(format!("Error: {}", e)),
            }
        }
        "mount" => {
            let path = match parts.get(1) {
                Some(p) => *p,
                None => return stay("Usage: mount <file>".to_string()),
            };
            match emu.bus.insert_disk(0, path) {
                Ok(()) => close(format!("Disk mounted in drive A: {}", path)),
                Err(e) => stay(format!("Error: {}", e)),
            }
        }
        "go" | "run" => {
            emu.cpu.halted = false;
            close("Resuming execution. Press Ctrl+K to pause.".to_string())
        }
        "reset" => {
            *emu = rust_imsai_emulator::Imsai8080::new();
            *program_name = String::new();
            stay("Cold reset. Memory cleared, CPU at 0x0000.".to_string())
        }
        "quit" => {
            close("Quitting.".to_string())
        }
        _ => stay("Unknown command. Type 'help' for the list.".to_string()),
    }
}

/// Interactive terminal mode: TUI with 80x24 CRT display and status bar.
pub fn run_terminal(emu: &mut rust_imsai_emulator::Imsai8080) {
    let mut program_name = String::new();
    if enable_raw_mode().is_err() {
        eprintln!("No TTY available, falling back to batch mode (use --batch to force)");
        crate::trace::run_interactive(emu, 50_000_000);
        return;
    }

    emu.bus.console().set_auto_render(false);

    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .expect("Failed to enter alternate screen");
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
    let poll_timeout = std::time::Duration::from_millis(0);
    let mut instruction_count: u64 = 0;
    let mut idle_count: u64 = 0;
    let mut last_display: String = String::new();
    let mut running = true;
    let start_time = Instant::now();

    loop {
        if running && !emu.cpu.halted {
            for _ in 0..batch_size {
                emu.step();
                instruction_count += 1;
                emu.bus.serial().poll_rx();

                if emu.cpu.halted {
                    break;
                }
            }
        }

        let display = emu.bus.console().video().get_display_string();
        if display != last_display || emu.cpu.halted {
            last_display = display.clone();
            let term_size = crossterm::terminal::size().unwrap_or((80, 24));
            let term_cols = term_size.0 as usize;
            let term_rows = term_size.1 as usize;
            let display_lines: Vec<&str> = display.trim_end_matches('\n').lines().collect();

            let crt_rows = display_lines.len().min(term_rows.saturating_sub(1));

            let mut row: u16 = 1;
            for line in display_lines.iter().take(crt_rows) {
                let truncated: String = line.chars().take(term_cols).collect();
                print!("\x1B[{};1H{}\x1B[K", row, truncated);
                row += 1;
            }

            render_bottom_row(emu, &program_name, BottomMode::Status, running, "", None);
            stdout.flush().ok();
        }

        let mut got_key = false;
        while event::poll(poll_timeout).unwrap_or(false) {
            if let Ok(ev) = event::read() {
                match ev {
                    Event::Key(key_event) => {
                        match key_event {
                            KeyEvent { code: KeyCode::Esc, .. } => {
                                emu.bus.console().type_text("\x1B");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::F(5), .. } => {
                                running = !running;
                                last_display.clear();
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Char('d'), modifiers: KeyModifiers::CONTROL, .. } => {
                                print!("\r\n\r\n--- Ctrl+D pressed, shutting down ---\r\n");
                                stdout.flush().ok();
                                stdout.execute(LeaveAlternateScreen).ok();
                                disable_raw_mode().ok();
                                let elapsed = start_time.elapsed();
                                let secs = elapsed.as_secs_f64();
                                let ips = if secs > 0.0 { instruction_count as f64 / secs } else { 0.0 };
                                eprintln!("Executed {} instructions in {:.2}s ({:.0} ips)", instruction_count, secs, ips);
                                save_memory(emu);
                                return;
                            }
                            KeyEvent { code: KeyCode::Char('k'), modifiers: KeyModifiers::CONTROL, .. } => {
                                if run_command_modal(emu, &mut stdout, &mut program_name) {
                                    running = true;
                                }
                                last_display.clear();
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Char(ch), modifiers: KeyModifiers::CONTROL, .. } => {
                                let ctrl_ch = (ch as u8) & 0x1F;
                                if ctrl_ch != 0 {
                                    emu.bus.serial().type_text(&String::from_utf8_lossy(&[ctrl_ch]));
                                    got_key = true;
                                }
                            }
                            KeyEvent { code: KeyCode::Char(ch), .. } => {
                                let upper: String = ch.to_uppercase().collect();
                                emu.bus.serial().type_text(&upper);
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Enter, .. } => {
                                emu.bus.serial().type_text("\r");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Backspace, .. } => {
                                emu.bus.serial().type_text("\x7F");
                                got_key = true;
                            }
                            KeyEvent { code: KeyCode::Tab, .. } => {
                                emu.bus.serial().type_text("\t");
                                got_key = true;
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(_, _) => {
                        last_display.clear();
                    }
                    _ => {}
                }
            }
        }

        // After keyboard input, check if the UART is waiting for data by
        // looking at the 8251A RX ready bit. If the program is polling the
        // status port for input and we didn't just type something, inject a
        // tiny sleep to avoid busy-spinning.
        if got_key {
            idle_count = 0;
        } else {
            // Check if CPU is in a CONIN busy-wait loop (polling port 0x01
            // for RXRDY). This is extremely common in 8080 programs.
            let rx_ready = emu.bus.serial().is_key_ready();
            let pc = emu.cpu.pc;
            let op = emu.bus.mem_read(pc);
            // IN 0x01 = 0xDB 0x01, the classic CONIN status poll
            if !rx_ready && op == 0xDB && emu.bus.mem_read(pc.wrapping_add(1)) == 0x01 {
                idle_count += 1;
                if idle_count > 100 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            } else {
                idle_count = 0;
            }
        }
    }
}