//! IMSAI 8080 Front Panel - Raylib GUI
//!
//! Visual emulation of the IMSAI 8080 front panel. Toggle switches, LEDs,
//! and function buttons. No ROM, no CP/M, just hardware.
//!
//! Usage:
//!   imsai-panel              Start with UART test program running (demo mode)
//!   imsai-panel --bare       Start with empty memory, front panel only
//!   imsai-panel --load <file> [addr]  Load binary at address (default 0x0000)
//!   imsai-panel --disk <file>         Load disk image and boot CP/M

use std::env;
use std::fs;
use std::path::PathBuf;
use raylib::prelude::RaylibDraw;

// === Front panel program format ===
// A program is a saved sequence of switch positions and button presses,
// exactly as you would operate the real IMSAI 8080 front panel.
// The "load" action writes raw bytes at an address (like load_program).
// The "step" actions set switches then press a button.

/// One step in a front panel program.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
#[serde(rename_all = "snake_case")]
enum PanelStep {
    /// Set address switches to this value, set data switches to this value,
    /// then press DEPOSIT. Advances address by 1 (like real hardware).
    Deposit { address: String, data: String },
    /// Set data switches, press DEPOSIT NEXT. Address auto-advances.
    DepositNext { data: String },
    /// Set address switches, press EXAMINE. Reads byte at that address.
    Examine { address: String },
    /// Press EXAMINE NEXT. Reads next byte.
    ExamineNext,
    /// Set address switches, then press RUN/STOP to start execution.
    Run { address: String },
    /// Load raw bytes into memory starting at address (bypasses front panel).
    /// Used for convenience when you don't want to toggle every byte.
    Load { address: String, data: String },
}

/// A front panel program: named sequence of steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PanelProgram {
    name: String,
    #[serde(default)]
    description: String,
    steps: Vec<PanelStep>,
}

fn default_programs_dir() -> PathBuf {
    // Look for PROGRAMS/ relative to current working directory first,
    // then relative to the executable directory
    let cwd = PathBuf::from(".");
    let cwd_prog = cwd.join("PROGRAMS");
    if cwd_prog.exists() {
        return cwd_prog;
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            let exe_prog = parent.join("PROGRAMS");
            if exe_prog.exists() {
                return exe_prog;
            }
        }
    }
    PathBuf::from("PROGRAMS")
}

fn load_program_file(path: &PathBuf) -> Result<PanelProgram, String> {
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

fn save_program_file(prog: &PanelProgram, path: &PathBuf) -> Result<(), String> {
    let json = serde_json::to_string_pretty(prog)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Parse a hex string like "3E" or "0x3E" into a u8.
fn parse_hex8(s: &str) -> Result<u8, String> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(s, 16).map_err(|e| format!("Invalid hex byte '{}': {}", s, e))
}

/// Parse a hex address like "0000" or "0x0000" into a u16.
fn parse_hex16(s: &str) -> Result<u16, String> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| format!("Invalid hex address '{}': {}", s, e))
}

/// Parse a hex data string like "3E 4E D3 01" into bytes.
fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    s.split_whitespace()
        .map(|b| parse_hex8(b))
        .collect()
}
use rust_imsai_emulator::Imsai8080;
use rust_imsai_emulator::TarbellCard;
use rust_imsai_emulator::cards::PanelSwitch;
use raylib::consts::{KeyboardKey, MouseButton};
use serde::{Deserialize, Serialize};

// Window size
const W: i32 = 1100;
const H: i32 = 720;

// LED dimensions
const LED_SIZE: i32 = 14;
const LED_GAP: i32 = 22;

// Switch dimensions (paddle-style toggle switches)
// Each switch slot is taller to show the paddle flipping up/down
const SW_W: i32 = 18;
const SW_H: i32 = 44;
const SW_GAP: i32 = 28;
const SW_PADDLE_H: i32 = 18;  // visible paddle height
const SW_PLATE_H: i32 = 4;    // base plate thickness

// Button dimensions
const BTN_W: i32 = 72;
const BTN_H: i32 = 30;
const BTN_GAP: i32 = 8;

// Momentary button colors
const BTN_TOP: u8 = 85;
const BTN_FACE: u8 = 65;
const BTN_SHADOW: u8 = 40;

// Terminal dimensions
const TERM_COLS: usize = 80;
const TERM_ROWS: usize = 24;
const TERM_CHAR_W: i32 = 6;
const TERM_CHAR_H: i32 = 12;

/// UART test program: initializes 8251A and prints 'A' forever.
const UART_TEST: [u8; 15] = [
    0x3E, 0x4E,       // MVI A, 0x4E (8 data, no parity, 1 stop, 16x)
    0xD3, 0x01,       // OUT 0x01  (mode command)
    0x3E, 0x05,       // MVI A, 0x05 (TX enable, RX enable)
    0xD3, 0x01,       // OUT 0x01  (command)
    // loop: print 'A'
    0x3E, 0x41,       // MVI A, 0x41 ('A')
    0xD3, 0x00,       // OUT 0x00  (data port)
    0xC3, 0x0A, 0x00, // JMP 0x000A
];

/// CCP base address for CP/M 2.2 64K system.
const CPMB: u16 = 0xE400;

fn main() {
    let args: Vec<String> = env::args().collect();
    let bare = args.contains(&"--bare".to_string());
    let load_arg = args.iter().position(|a| a == "--load")
        .and_then(|i| args.get(i + 1).cloned());
    let disk_arg = args.iter().position(|a| a == "--disk")
        .and_then(|i| args.get(i + 1).cloned());
    let program_arg = args.iter().position(|a| a == "--program")
        .and_then(|i| args.get(i + 1).cloned());

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        eprintln!("IMSAI 8080 Front Panel");
        eprintln!();
        eprintln!("Usage: imsai-panel [OPTIONS]");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  (default)           Start with UART test program loaded, STOPPED");
        eprintln!("  --bare              Start with empty memory (front panel only)");
        eprintln!("  --load <file> [addr] Load raw binary at address (default 0x0000)");
        eprintln!("  --disk <file>       Load disk image and boot CP/M 2.2");
        eprintln!("  --program <file>    Load a front panel program (.json), STOPPED");
        eprintln!("  --help, -h          Show this help");
        return;
    }

    let (mut rl, thread) = raylib::init()
        .size(W, H)
        .title("IMSAI 8080")
        .build();

    rl.set_target_fps(30);

    let mut emu = Imsai8080::new();
    let mut addr_sw = [false; 16];
    let mut data_sw = [false; 8];
    let mut program_name = String::new(); // shown in UI

    // Load program/disk based on arguments. All modes start STOPPED.
    // The user presses F5 or clicks RUN/STOP to begin execution.
    let _loaded_program = if !bare {
        if let Some(ref path) = program_arg {
            let pbuf = PathBuf::from(path);
            match load_program_file(&pbuf) {
                Ok(prog) => {
                    eprintln!("Loaded program: {}", prog.name);
                    execute_panel_program(&mut emu, &prog);
                    // Set address switches to start address of program
                    if let Some(start_addr) = find_program_start(&prog) {
                        for i in 0..16 {
                            addr_sw[i] = (start_addr >> (15 - i)) & 1 != 0;
                        }
                    }
                    program_name = prog.name.clone();
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        } else if let Some(ref path) = disk_arg {
            match emu.bus.card_mut::<TarbellCard>().unwrap().insert_disk(0, path) {
                Ok(()) => {
                    boot_cpm(&mut emu);
                    // Start STOPPED at CCP entry point, user presses F5 to run
                    program_name = "CP/M 2.2".to_string();
                }
                Err(e) => eprintln!("Error loading disk '{}': {}", path, e),
            }
            true
        } else if let Some(ref path) = load_arg {
            let addr_idx = args.iter().position(|a| a == "--load").unwrap() + 2;
            let addr: u16 = args.get(addr_idx)
                .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0);
            match std::fs::read(path) {
                Ok(data) => {
                    emu.load_program(addr, &data);
                    emu.panel.set_address_switches(addr);
                    emu.process_panel();
                    for i in 0..16 {
                        addr_sw[i] = (addr >> (15 - i)) & 1 != 0;
                    }
                    program_name = format!("{} @ {:04X}", path, addr);
                }
                Err(e) => eprintln!("Error loading '{}': {}", path, e),
            }
            true
        } else {
            // Default: load UART test program, start STOPPED
            emu.load_program(0x0000, &UART_TEST);
            emu.panel.set_address_switches(0x0000);
            emu.process_panel();
            program_name = "UART Test".to_string();
            true
        }
    } else {
        false
    };

    let mut term = [[0x20u8; TERM_COLS]; TERM_ROWS];
    let mut tcx: usize = 0;
    let mut tcy: usize = 0;

    let mut running = false;
    let mut cycles: u64 = 0;
    let mut step_pending = false;
    let mut load_program_index: usize = 0;
    let mut status_msg = String::new();
    let mut status_msg_timer: i32 = 0; // frames remaining

    // Colors
    let bg        = raylib::color::Color { r: 25, g: 25, b: 30, a: 255 };
    let panel_bg  = raylib::color::Color { r: 40, g: 38, b: 36, a: 255 };
    let led_on    = raylib::color::Color { r: 0, g: 255, b: 60, a: 255 };
    let led_off   = raylib::color::Color { r: 30, g: 30, b: 30, a: 255 };
    let led_red   = raylib::color::Color { r: 255, g: 40, b: 40, a: 255 };
    // Switch colors: paddle is metallic, slot is dark recess
    let sw_paddle = raylib::color::Color { r: 200, g: 195, b: 185, a: 255 }; // metallic silver
    let sw_paddle_hi = raylib::color::Color { r: 240, g: 235, b: 225, a: 255 }; // highlight edge
    let sw_paddle_lo = raylib::color::Color { r: 140, g: 135, b: 125, a: 255 }; // shadow edge
    let sw_slot   = raylib::color::Color { r: 15, g: 15, b: 18, a: 255 };       // recessed slot
    let sw_slot_rim = raylib::color::Color { r: 55, g: 52, b: 48, a: 255 };      // slot rim
    let sw_tip_on  = raylib::color::Color { r: 255, g: 60, b: 60, a: 255 };      // red tip for ON
    let sw_tip_off = raylib::color::Color { r: 80, g: 80, b: 80, a: 255 };       // dark tip for OFF
    let txt       = raylib::color::Color { r: 200, g: 200, b: 180, a: 255 };
    let txt_dim   = raylib::color::Color { r: 120, g: 120, b: 110, a: 255 };
    let t_fg      = raylib::color::Color { r: 0, g: 220, b: 80, a: 255 };
    let t_bg      = raylib::color::Color { r: 5, g: 12, b: 5, a: 255 };
    let border    = raylib::color::Color { r: 100, g: 100, b: 90, a: 255 };
    // Momentary pushbutton colors (3D raised button look)
    let mom_face  = raylib::color::Color { r: BTN_FACE, g: BTN_FACE, b: BTN_FACE + 5, a: 255 };
    let mom_hi    = raylib::color::Color { r: BTN_TOP, g: BTN_TOP, b: BTN_TOP + 5, a: 255 };
    let mom_lo    = raylib::color::Color { r: BTN_SHADOW, g: BTN_SHADOW, b: BTN_SHADOW + 5, a: 255 };
    let mom_text  = raylib::color::Color { r: 220, g: 220, b: 210, a: 255 };

    // Layout constants (left panel x=20, terminal x=580)
    let lp_x: i32 = 20;              // left panel left edge
    let tp_x: i32 = 580;             // terminal panel left edge
    let tp_w: i32 = W - tp_x - 10;  // terminal panel width

    while !rl.window_should_close() {
        // ---- Input: toggle switches ----
        if rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT) {
            let m = rl.get_mouse_position();

            // Address switches row (matches draw at addr_sw_y = 310)
            let addr_sw_y: f32 = 310.0;
            for i in 0..16usize {
                let x = (lp_x + 10 + i as i32 * SW_GAP) as f32;
                let w = SW_W as f32;
                let h = SW_H as f32;
                if m.x >= x && m.x < x + w && m.y >= addr_sw_y && m.y < addr_sw_y + h {
                    addr_sw[i] = !addr_sw[i];
                }
            }

            // Data switches row (matches draw at data_sw_y = 385)
            let data_sw_y: f32 = 385.0;
            for i in 0..8usize {
                let x = (lp_x + 10 + i as i32 * SW_GAP) as f32;
                if m.x >= x && m.x < x + SW_W as f32 && m.y >= data_sw_y && m.y < data_sw_y + SW_H as f32 {
                    data_sw[i] = !data_sw[i];
                }
            }

            // Function controls (matches draw at btn_y = 470)
            // RUN/STOP is a toggle switch at runstop_x
            let runstop_x = lp_x + 10 + 6;
            if m.x >= runstop_x as f32 && m.x < (runstop_x + SW_W) as f32
                && m.y >= 470.0 && m.y < (470 + BTN_H) as f32 {
                emu.panel.press_switch(PanelSwitch::RunStop);
            }

            // Momentary buttons (STEP, EXAM, DEP, EX NXT, DEP NXT)
            let mom_start_x = (lp_x + 10 + SW_W + 16) as f32;
            let mom_actions = [PanelSwitch::SingleStep, PanelSwitch::Examine,
                               PanelSwitch::Deposit, PanelSwitch::ExamineNext,
                               PanelSwitch::DepositNext];
            for (i, action) in mom_actions.iter().enumerate() {
                let x = mom_start_x + i as f32 * (BTN_W + BTN_GAP) as f32;
                if m.x >= x && m.x < x + BTN_W as f32 && m.y >= 470.0 && m.y < 470.0 + BTN_H as f32 {
                    if *action == PanelSwitch::SingleStep {
                        step_pending = true;
                    } else {
                        emu.panel.press_switch(*action);
                    }
                }
            }
        }

        // Keyboard shortcuts
        if rl.is_key_pressed(KeyboardKey::KEY_F5) {
            emu.panel.press_switch(PanelSwitch::RunStop);
        }
        // F2: Load program from PROGRAMS/ directory (cycles through available files)
        // F3: Save current memory region as a program file
        if rl.is_key_pressed(KeyboardKey::KEY_F2) {
            // Scan PROGRAMS/ for .json files
            let prog_dir = default_programs_dir();
            if let Ok(entries) = fs::read_dir(&prog_dir) {
                let mut json_files: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                    .filter_map(|e| e.path().to_str().map(|s| s.to_string()))
                    .collect();
                json_files.sort();
                // Cycle to next program
                if !json_files.is_empty() {
                    load_program_index = (load_program_index + 1) % json_files.len();
                    let path = PathBuf::from(&json_files[load_program_index]);
                    match load_program_file(&path) {
                        Ok(prog) => {
                            eprintln!("Loaded: {}", prog.name);
                            // Reset emulator, load program, start STOPPED
                            emu = Imsai8080::new();
                            execute_panel_program(&mut emu, &prog);
                            if let Some(start_addr) = find_program_start(&prog) {
                                for i in 0..16 {
                                    addr_sw[i] = (start_addr >> (15 - i)) & 1 != 0;
                                }
                            }
                            program_name = prog.name.clone();
                            cycles = 0;
                            term = [[0x20u8; TERM_COLS]; TERM_ROWS];
                            tcx = 0;
                            tcy = 0;
                            running = false;
                            status_msg = format!("Loaded: {}", program_name);
                            status_msg_timer = 180; // ~6 seconds at 30fps
                        }
                        Err(e) => {
                            status_msg = format!("Error: {}", e);
                            status_msg_timer = 180;
                        }
                    }
                } else {
                    status_msg = "No .json programs in PROGRAMS/".to_string();
                    status_msg_timer = 180;
                }
            }
        }
        if rl.is_key_pressed(KeyboardKey::KEY_F3) {
            // Save current memory as a program
            // Dumps 256 bytes from the current address switches position
            let pc = emu.cpu.pc;
            let dump_len: u16 = 256;
            let prog = memory_to_program(
                &format!("dump_{:04X}", pc),
                &format!("Memory dump from {:04X}, {} bytes", pc, dump_len),
                pc, dump_len, &emu,
            );
            if fs::create_dir_all(default_programs_dir()).is_ok() {
                let filename = default_programs_dir().join(format!("dump_{:04X}.json", pc));
                match save_program_file(&prog, &filename) {
                    Ok(()) => {
                        status_msg = format!("Saved: {}", filename.display());
                        status_msg_timer = 180;
                    }
                    Err(e) => {
                        status_msg = format!("Save error: {}", e);
                        status_msg_timer = 180;
                    }
                }
            }
        }
        // R: Reset to UART test, STOPPED
        if rl.is_key_pressed(KeyboardKey::KEY_R) {
            emu = Imsai8080::new();
            emu.load_program(0x0000, &UART_TEST);
            emu.panel.set_address_switches(0x0000);
            emu.process_panel();
            addr_sw = [false; 16];
            data_sw = [false; 8];
            cycles = 0;
            term = [[0x20u8; TERM_COLS]; TERM_ROWS];
            tcx = 0;
            tcy = 0;
            running = false;
            program_name = "UART Test".to_string();
        }

        // Keyboard input for terminal (only when running)
        if running {
            if let Some(ch) = rl.get_char_pressed() {
                emu.bus.serial().type_text(&ch.to_uppercase().collect::<String>());
            }
            if rl.is_key_pressed(KeyboardKey::KEY_ENTER) {
                emu.bus.serial().type_text("\r");
            }
            if rl.is_key_pressed(KeyboardKey::KEY_BACKSPACE) {
                emu.bus.serial().type_text("\x7F");
            }
        }

        // ---- Update panel ----
        let addr_val: u16 = addr_sw.iter().enumerate()
            .fold(0u16, |a, (i, &on)| if on { a | (1 << (15 - i)) } else { a });
        let data_val: u8 = data_sw.iter().enumerate()
            .fold(0u8, |a, (i, &on)| if on { a | (1 << (7 - i)) } else { a });
        emu.panel.set_address_switches(addr_val);
        emu.panel.set_data_switches(data_val);
        emu.process_panel();

        if step_pending {
            emu.single_step();
            step_pending = false;
        }

        // ---- Run ----
        if emu.panel.is_running() {
            running = true;
            let n: u64 = 10000;
            emu.run_batch(n);
            cycles += n;

            emu.bus.serial().channel_a_mut().drain_tx();
            emu.bus.serial().channel_a_mut().update_tx();
            emu.bus.serial().poll_keyboard();
            let output = emu.bus.serial().channel_a_mut().take_output();
            for &b in &output {
                match b {
                    0x0D | 0x0A => { tcx = 0; tcy += 1; if tcy >= TERM_ROWS { tcy = TERM_ROWS - 1; } }
                    0x08 => { if tcx > 0 { tcx -= 1; } }
                    0x20..=0x7E => {
                        if tcx < TERM_COLS && tcy < TERM_ROWS {
                            term[tcy][tcx] = b;
                            tcx += 1;
                            if tcx >= TERM_COLS { tcx = 0; tcy += 1; if tcy >= TERM_ROWS { tcy = TERM_ROWS - 1; } }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            running = false;
        }

        // ---- Draw ----
        let mut d = rl.begin_drawing(&thread);
        d.clear_background(bg);

        // === Left panel background ===
        d.draw_rectangle(lp_x, 5, 540, 700, panel_bg);
        d.draw_rectangle_lines(lp_x, 5, 540, 700, border);

        // Title bar
        d.draw_text("IMSAI 8080", lp_x + 10, 12, 22, txt);
        let state_str = if emu.panel.is_running() { "RUNNING" } else { "STOPPED" };
        let state_col = if emu.panel.is_running() { led_on } else { led_red };
        d.draw_text(state_str, lp_x + 200, 14, 18, state_col);
        d.draw_text(&format!("Cycles: {}", cycles), lp_x + 340, 16, 12, txt_dim);
        if !program_name.is_empty() {
            d.draw_text(&program_name, lp_x + 10, 38, 11, txt_dim);
        }
        // Status message (load/save feedback, fades out)
        if status_msg_timer > 0 {
            let alpha = if status_msg_timer < 30 { status_msg_timer * 8 } else { 255 }.min(255) as u8;
            let msg_col = raylib::color::Color { r: 255, g: 255, b: 200, a: alpha };
            d.draw_text(&status_msg, tp_x + 5, 5, 11, msg_col);
            status_msg_timer -= 1;
        }

        // === Address LEDs (row at y=50) ===
        let led_row_y: i32 = 50;
        d.draw_text("ADDRESS", lp_x + 10, led_row_y - 14, 12, txt_dim);
        let leds = emu.panel.leds();
        for i in 0..16usize {
            let x = lp_x + 10 + (15 - i) as i32 * LED_GAP;
            let on = leds.address[i];
            d.draw_rectangle(x, led_row_y, LED_SIZE, LED_SIZE, if on { led_on } else { led_off });
            // Bit labels every 4 bits
            if i % 4 == 0 {
                d.draw_text(&format!("{}", 15 - i), x - 1, led_row_y + LED_SIZE + 2, 8, txt_dim);
            }
        }
        d.draw_text(&format!("= {:04X}", addr_val), lp_x + 10 + 16 * LED_GAP + 4, led_row_y + 1, 13, txt);

        // === Data LEDs (row at y=115) ===
        let data_led_y: i32 = 115;
        d.draw_text("DATA", lp_x + 10, data_led_y - 14, 12, txt_dim);
        for i in 0..8usize {
            let x = lp_x + 10 + (7 - i) as i32 * LED_GAP;
            let on = leds.data[i];
            d.draw_rectangle(x, data_led_y, LED_SIZE, LED_SIZE, if on { led_on } else { led_off });
        }
        d.draw_text(&format!("= {:02X}", data_val), lp_x + 10 + 8 * LED_GAP + 4, data_led_y + 1, 13, txt);

        // === Status LEDs (row at y=165) ===
        let status_y: i32 = 165;
        d.draw_text("STATUS", lp_x + 10, status_y - 14, 12, txt_dim);
        let status = [
            ("RUN", leds.run, led_on), ("M1", leds.m1, led_on),
            ("WAIT", leds.wait, led_red), ("MEMR", leds.memr, led_on),
            ("MWRT", leds.mwrt, led_red), ("IOR", leds.ior, led_on),
            ("IOW", leds.iow, led_on), ("PWR", leds.power, led_on),
        ];
        for (i, (lbl, on, oc)) in status.iter().enumerate() {
            let x = lp_x + 10 + i as i32 * 65;
            d.draw_rectangle(x, status_y, 10, 10, if *on { *oc } else { led_off });
            d.draw_text(lbl, x + 14, status_y - 1, 10, txt);
        }

        // === Address switches (paddle toggles at y=310) ===
        let addr_sw_y: i32 = 310;
        d.draw_text(&format!("ADDR {:04X}", addr_val), lp_x + 10, addr_sw_y - 16, 13, txt);
        for i in 0..16usize {
            let x = lp_x + 10 + i as i32 * SW_GAP;
            draw_toggle_switch(&mut d, x, addr_sw_y, SW_W, SW_H, SW_PADDLE_H, SW_PLATE_H,
                addr_sw[i], sw_slot, sw_slot_rim, sw_paddle, sw_paddle_hi, sw_paddle_lo,
                sw_tip_on, sw_tip_off);
        }

        // === Data switches (paddle toggles at y=385) ===
        let data_sw_y: i32 = 385;
        d.draw_text(&format!("DATA {:02X}", data_val), lp_x + 10, data_sw_y - 16, 13, txt);
        for i in 0..8usize {
            let x = lp_x + 10 + i as i32 * SW_GAP;
            draw_toggle_switch(&mut d, x, data_sw_y, SW_W, SW_H, SW_PADDLE_H, SW_PLATE_H,
                data_sw[i], sw_slot, sw_slot_rim, sw_paddle, sw_paddle_hi, sw_paddle_lo,
                sw_tip_on, sw_tip_off);
        }

        // === Function controls (row at y=470) ===
        // RUN/STOP is a latching toggle (like address/data switches but red).
        // The other 5 are momentary push-buttons (press and release).
        let btn_y: i32 = 470;
        d.draw_text("CONTROLS", lp_x + 10, btn_y - 16, 12, txt_dim);

        // RUN/STOP toggle switch (red paddle, shows current RUN/STOP state)
        let runstop_x = lp_x + 10;
        d.draw_text("RUN/STOP", runstop_x, btn_y + BTN_H as i32 + 2, 9, txt_dim);
        draw_toggle_switch(&mut d, runstop_x + 6, btn_y, SW_W, BTN_H, 14, 4,
            emu.panel.is_running(), sw_slot, sw_slot_rim, sw_paddle, sw_paddle_hi, sw_paddle_lo,
            raylib::color::Color { r: 255, g: 50, b: 50, a: 255 },  // red tip ON
            sw_tip_off);

        // Momentary push-buttons (STEP, EXAM, DEPOSIT, EX NXT, DEP NXT)
        let mom_labels = ["STEP", "EXAM", "DEP", "EX NXT", "DEP NXT"];
        let mom_start_x = lp_x + 10 + SW_W + 16;
        for (i, lbl) in mom_labels.iter().enumerate() {
            let x = mom_start_x + i as i32 * (BTN_W + BTN_GAP);
            // 3D raised button: top edge lighter, bottom edge darker
            d.draw_rectangle(x, btn_y, BTN_W, BTN_H, mom_hi);     // top highlight
            d.draw_rectangle(x, btn_y + 2, BTN_W, BTN_H, mom_face); // face
            d.draw_rectangle(x, btn_y + BTN_H - 2, BTN_W, 2, mom_lo); // shadow bottom
            d.draw_rectangle(x + BTN_W - 2, btn_y, 2, BTN_H, mom_lo);  // shadow right
            d.draw_rectangle_lines(x, btn_y, BTN_W, BTN_H, border);
            // Centered label
            let text_w = d.measure_text(lbl, 9);
            d.draw_text(lbl, x + (BTN_W - text_w) / 2, btn_y + 10, 9, mom_text);
        }

        // === Terminal display (right panel) ===
        let term_border_y: i32 = 12;
        let term_border_h: i32 = TERM_ROWS as i32 * TERM_CHAR_H + 24;
        d.draw_rectangle(tp_x, term_border_y, tp_w, term_border_h, t_bg);
        d.draw_rectangle_lines(tp_x, term_border_y, tp_w, term_border_h, border);
        d.draw_text("CONSOLE", tp_x + 5, term_border_y + 3, 11, txt_dim);

        let term_text_y = term_border_y + 18;
        for row in 0..TERM_ROWS {
            for col in 0..TERM_COLS {
                let ch = term[row][col];
                if ch != 0x20 && ch != 0x00 {
                    d.draw_text(
                        &format!("{}", ch as char),
                        tp_x + 5 + col as i32 * TERM_CHAR_W,
                        term_text_y + row as i32 * TERM_CHAR_H,
                        9, t_fg
                    );
                }
            }
        }

        // === I/O log (below terminal) ===
        let iolog_y = term_border_y + term_border_h + 8;
        d.draw_text("I/O LOG", tp_x + 5, iolog_y, 12, txt_dim);
        let io_log = emu.panel.io_log();
        let log_start = io_log.len().saturating_sub(12);
        for (i, ev) in io_log[log_start..].iter().enumerate() {
            let dir = if ev.is_write { "OUT" } else { "IN " };
            let kind = if ev.is_io { "IO" } else { "MEM" };
            d.draw_text(
                &format!("{} {:04X} {:02X} {}", dir, ev.address, ev.data, kind),
                tp_x + 5, iolog_y + 16 + i as i32 * 14, 10, txt_dim
            );
        }

        // === Memory readout (small hex view at PC, below switches on left) ===
        let mem_y: i32 = 510;
        d.draw_text("MEMORY AT PC", lp_x + 10, mem_y, 12, txt_dim);
        let pc = emu.cpu.pc;
        for i in 0..4usize {
            let addr = pc.wrapping_add(i as u16);
            let val = emu.bus.mem_read(addr);
            d.draw_text(
                &format!("{:04X}: {:02X}", addr, val),
                lp_x + 10, mem_y + 16 + i as i32 * 14, 10, txt_dim
            );
        }

        // === Registers ===
        let reg_y = mem_y + 80;
        d.draw_text("REGISTERS", lp_x + 10, reg_y, 12, txt_dim);
        let cpu = &emu.cpu;
        d.draw_text(&format!("A:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X}", cpu.a, cpu.b, cpu.c, cpu.d, cpu.e), lp_x + 10, reg_y + 16, 10, txt_dim);
        d.draw_text(&format!("H:{:02X} L:{:02X} SP:{:04X} PC:{:04X}", cpu.h, cpu.l, cpu.sp, cpu.pc), lp_x + 10, reg_y + 30, 10, txt_dim);
        let f = &cpu.flags;
        d.draw_text(&format!("FLAGS: S{} Z{} AC{} P{} CY{}", f.s as u8, f.z as u8, f.ac as u8, f.p as u8, f.cy as u8), lp_x + 10, reg_y + 44, 10, txt_dim);

        // Help line at bottom
        d.draw_text("F5:Run/Stop  F2:LoadProg  F3:SaveDump  R:Reset", lp_x + 10, H - 20, 11, txt_dim);

        drop(d);
    }
}

/// Draw an IMSAI-style paddle toggle switch.
///
/// The switch sits in a recessed slot. When ON, the paddle flips up
/// (paddle extends from top half of slot, red tip visible at top).
/// When OFF, the paddle flips down (paddle extends from bottom half,
/// dark tip visible at bottom).
fn draw_toggle_switch(
    d: &mut raylib::drawing::RaylibDrawHandle,
    x: i32, y: i32, w: i32, h: i32, paddle_h: i32, plate_h: i32,
    is_on: bool,
    slot_color: raylib::color::Color,
    rim_color: raylib::color::Color,
    paddle_color: raylib::color::Color,
    paddle_hi: raylib::color::Color,
    paddle_lo: raylib::color::Color,
    tip_on: raylib::color::Color,
    tip_off: raylib::color::Color,
) {
    // Outer slot (recessed dark rectangle with rim)
    d.draw_rectangle(x - 1, y - 1, w + 2, h + 2, rim_color);
    d.draw_rectangle(x, y, w, h, slot_color);

    // The paddle fills half the slot vertically, positioned at top (ON) or bottom (OFF)
    let paddle_y = if is_on { y } else { y + h - paddle_h };
    let tip_y = if is_on { y } else { y + h - plate_h };

    // Paddle body
    d.draw_rectangle(x + 1, paddle_y + plate_h, w - 2, paddle_h - plate_h, paddle_color);
    // Paddle highlight (left edge lighter)
    d.draw_rectangle(x + 1, paddle_y + plate_h, 2, paddle_h - plate_h, paddle_hi);
    // Paddle shadow (right edge darker)
    d.draw_rectangle(x + w - 3, paddle_y + plate_h, 2, paddle_h - plate_h, paddle_lo);
    // Top cap of paddle (the tip, colored red=ON or dark=OFF)
    d.draw_rectangle(x + 1, tip_y, w - 2, plate_h, if is_on { tip_on } else { tip_off });
    // Tip highlight
    d.draw_rectangle(x + 1, tip_y, 2, plate_h, if is_on {
        raylib::color::Color { r: 255, g: 120, b: 120, a: 255 }
    } else {
        raylib::color::Color { r: 100, g: 100, b: 100, a: 255 }
    });
}

/// Execute a front panel program: walk through each step, setting switches
/// and pressing buttons via the front panel interface, just like a human would.
fn execute_panel_program(emu: &mut Imsai8080, prog: &PanelProgram) {
    for step in &prog.steps {
        match step {
            PanelStep::Deposit { address, data } => {
                let addr = parse_hex16(address).unwrap_or(0);
                let byte = parse_hex8(data).unwrap_or(0);
                emu.panel.set_address_switches(addr);
                emu.panel.set_data_switches(byte);
                emu.panel.press_switch(PanelSwitch::Deposit);
                emu.process_panel();
            }
            PanelStep::DepositNext { data } => {
                let byte = parse_hex8(data).unwrap_or(0);
                emu.panel.set_data_switches(byte);
                emu.panel.press_switch(PanelSwitch::DepositNext);
                emu.process_panel();
            }
            PanelStep::Examine { address } => {
                let addr = parse_hex16(address).unwrap_or(0);
                emu.panel.set_address_switches(addr);
                emu.panel.press_switch(PanelSwitch::Examine);
                emu.process_panel();
            }
            PanelStep::ExamineNext => {
                emu.panel.press_switch(PanelSwitch::ExamineNext);
                emu.process_panel();
            }
            PanelStep::Run { address } => {
                let addr = parse_hex16(address).unwrap_or(0);
                emu.panel.set_address_switches(addr);
                emu.panel.press_switch(PanelSwitch::RunStop);
                emu.process_panel();
            }
            PanelStep::Load { address, data } => {
                let addr = parse_hex16(address).unwrap_or(0);
                if let Ok(bytes) = parse_hex_bytes(data) {
                    emu.load_program(addr, &bytes);
                }
            }
        }
    }
}

/// Find the start address from a panel program (first "run" or "deposit" step).
fn find_program_start(prog: &PanelProgram) -> Option<u16> {
    for step in &prog.steps {
        match step {
            PanelStep::Run { address } => return parse_hex16(address).ok(),
            PanelStep::Deposit { address, .. } => return parse_hex16(address).ok(),
            PanelStep::Load { address, .. } => return parse_hex16(address).ok(),
            _ => {}
        }
    }
    None
}

/// Build a panel program from the current memory contents.
/// Dumps `len` bytes starting at `start` into a program with deposit_next steps.
fn memory_to_program(name: &str, description: &str, start: u16, len: u16, emu: &Imsai8080) -> PanelProgram {
    let mut steps = Vec::new();
    // First byte uses deposit (sets address + data)
    let first_byte = emu.bus.mem_read(start);
    steps.push(PanelStep::Deposit {
        address: format!("{:04X}", start),
        data: format!("{:02X}", first_byte),
    });
    // Remaining bytes use deposit_next
    for i in 1..len {
        let addr = start.wrapping_add(i);
        steps.push(PanelStep::Load {
            address: format!("{:04X}", addr),
            data: format!("{:02X}", emu.bus.mem_read(addr)),
        });
    }
    // Run at start address
    steps.push(PanelStep::Run {
        address: format!("{:04X}", start),
    });
    PanelProgram {
        name: name.to_string(),
        description: description.to_string(),
        steps,
    }
}

/// Boot CP/M 2.2 from disk: load system tracks and install BIOS.
fn boot_cpm(emu: &mut rust_imsai_emulator::Imsai8080) {
    const BIOS_BASE: u16 = 0xFA00;
    let mut mem_addr: u16 = CPMB;
    let mut sectors_loaded: u16 = 0;

    for track in 0..2u8 {
        for sector in 1..=26u8 {
            if track == 0 && sector == 1 {
                continue;
            }
            let sector_data = match emu.bus.tarbell().get_disk(0) {
                Some(disk) => match disk.read_sector(track, sector) {
                    Ok(data) => data,
                    Err(e) => {
                        eprintln!("Error reading track {} sector {}: {}", track, sector, e);
                        return;
                    }
                },
                None => {
                    eprintln!("No disk in drive 0");
                    return;
                }
            };

            if mem_addr >= BIOS_BASE {
                continue;
            }
            let end = mem_addr as usize + sector_data.len();
            if end > BIOS_BASE as usize {
                let avail = (BIOS_BASE - mem_addr) as usize;
                for j in 0..avail {
                    emu.bus.memory().write(mem_addr + j as u16, sector_data[j]);
                }
                mem_addr = BIOS_BASE;
                sectors_loaded += 1;
                continue;
            }
            for j in 0..sector_data.len() {
                emu.bus.memory().write(mem_addr + j as u16, sector_data[j]);
            }
            mem_addr += sector_data.len() as u16;
            sectors_loaded += 1;
        }
    }

    let bytes_loaded = mem_addr - CPMB;
    eprintln!("Loaded {} sectors ({} bytes) into 0x{:04X}-0x{:04X}",
        sectors_loaded, bytes_loaded, CPMB, CPMB + bytes_loaded);

    rust_imsai_emulator::Bios::install_jump_table(&mut emu.bus);
    emu.cpu.pc = CPMB;
    emu.cpu.sp = 0x0000;
}