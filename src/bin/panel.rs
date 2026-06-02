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

/// State for the in-app file picker overlay.
#[derive(Debug)]
enum PickerState {
    /// No picker visible.
    Closed,
    /// Load picker: showing a scrollable list of .json programs.
    Load {
        entries: Vec<PickerEntry>,
        scroll: i32,
        selected: i32,
    },
    /// Save picker: showing filename prompt.
    Save {
        filename: String,
        cursor_blink: i32,
    },
}

/// Action to take after the picker match block (avoids borrow conflicts).
enum PickerAction {
    Load(PathBuf),
    Save(String),
    Cancel,
}

/// One entry in the load picker file list.
#[derive(Debug, Clone)]
struct PickerEntry {
    name: String,
    description: String,
    path: PathBuf,
}

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
use raylib::consts::{KeyboardKey, MouseButton, TextureFilter};
use raylib::math::Vector2;
use raylib::text::RaylibFont;
use raylib::core::texture::RaylibTexture2D;
use serde::{Deserialize, Serialize};

/// Candidate system paths for the smooth branding font (first that loads wins).
const LOGO_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/TTF/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/TTF/Roboto-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
];

// Window size
const W: i32 = 1280;
const H: i32 = 840;

// === IMSAI 8080 front panel layout ===
// The real panel is a black face. LEDs and switches are organized into two
// 8-bit byte-blocks (A15-A8 left, A7-A0 right) separated by a central seam,
// with the IMSAI logo and the control switches on the right.
const PX: i32 = 16;   // panel left
const PY: i32 = 12;   // panel top
const PW: i32 = 1248; // panel width
const PH: i32 = 360;  // panel height

// Column geometry shared by LED rows and the switch row.
const COL_STEP: i32 = 34;   // center-to-center within a nibble
const NIBBLE_GAP: i32 = 16; // extra gap between the two nibbles of a byte
const LBYTE_X0: i32 = PX + 56;  // center of MSB (leftmost) LED, left byte
const RBYTE_X0: i32 = PX + 474; // center of MSB LED, right byte

// LED row baselines (vertical centers).
const Y_PROG: i32 = PY + 96;  // PROGRAMMED OUTPUT
const Y_STAT: i32 = PY + 166; // STATUS BYTE (left) + DATA BUS (right)
const Y_ADDR: i32 = PY + 236; // ADDRESS BUS (16) + mode LEDs (right)
const Y_SW: i32 = PY + 290;   // switch row top edge

const LED_RAD: f32 = 7.0;

// Paddle switch dimensions.
const PADDLE_W: i32 = 24;
const PADDLE_H: i32 = 50;

// Control switches (right cluster): EXAMINE, DEPOSIT, RESET, RUN, STEP, PWR.
// Grouped into a cluster, spaced enough for the word labels to stay readable.
const CTRL_STEP: i32 = 54;
const CTRL_X0: i32 = PX + 899; // center of first control paddle

// Terminal dimensions
const TERM_COLS: usize = 80;
const TERM_ROWS: usize = 24;
const TERM_CHAR_W: i32 = 7;
const TERM_CHAR_H: i32 = 12;

/// X center of address bit `i` (0 = MSB/bit15, 15 = LSB/bit0) in the LED/switch grid.
fn addr_col_x(i: usize) -> i32 {
    if i < 8 {
        LBYTE_X0 + i as i32 * COL_STEP + if i >= 4 { NIBBLE_GAP } else { 0 }
    } else {
        let j = i - 8;
        RBYTE_X0 + j as i32 * COL_STEP + if j >= 4 { NIBBLE_GAP } else { 0 }
    }
}

/// X center of a byte-block bit `j` (0 = MSB) for the single-byte rows
/// (programmed output uses the left block, data bus uses the right block).
fn byte_col_x(block_x0: i32, j: usize) -> i32 {
    block_x0 + j as i32 * COL_STEP + if j >= 4 { NIBBLE_GAP } else { 0 }
}

/// X center of control paddle `i` (0..6).
fn ctrl_col_x(i: usize) -> i32 {
    CTRL_X0 + i as i32 * CTRL_STEP
}

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
        .title("IMSAI 8080 Microcomputer")
        .build();

    rl.set_target_fps(30);

    // Smooth TTF font for the IMSAI 8080 branding (the default raylib bitmap
    // font is blocky when scaled up). Falls back to the bitmap font if no
    // system TTF is found. Loaded oversized + bilinear-filtered so it stays
    // crisp at the logo and subtitle sizes.
    let logo_font = LOGO_FONT_PATHS
        .iter()
        .find_map(|p| rl.load_font_ex(&thread, p, 96, None).ok());
    if let Some(ref f) = logo_font {
        f.texture().set_texture_filter(&thread, TextureFilter::TEXTURE_FILTER_BILINEAR);
    }

    let mut emu = Imsai8080::new();
    // The 16 sense switches set the address; the low 8 also serve as the
    // data byte for DEPOSIT (exactly as on the real IMSAI front panel).
    let mut addr_sw = [false; 16];
    let mut program_name = String::new(); // shown in UI
    let shot_arg = args.iter().position(|a| a == "--shot")
        .and_then(|i| args.get(i + 1).cloned());
    let mut frame: u64 = 0;

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
    let mut status_msg = String::new();
    let mut status_msg_timer: i32 = 0; // frames remaining
    let mut picker = PickerState::Closed;

    // Colors - IMSAI 8080: black panel face, white silk-screen text,
    // red LEDs, blue/red paddle switches, gray chassis surround.
    let rgb = |r, g, b| raylib::color::Color { r, g, b, a: 255 };
    let rgba = |r, g, b, a| raylib::color::Color { r, g, b, a };
    let bg        = rgb(78, 78, 82);     // gray chassis around the panel
    let panel_bg  = rgb(12, 12, 14);     // black panel face
    let panel_edge= rgb(38, 38, 42);     // panel inner bevel
    let led_on    = rgb(255, 60, 42);    // lit red LED
    let led_off   = rgb(46, 12, 10);     // dark (off) red LED
    let led_glow  = rgba(255, 80, 50, 70); // LED glow halo
    let txt       = rgb(232, 232, 232);  // white silk-screen label
    let txt_dim   = rgb(150, 150, 154);  // dim label
    let txt_bright= rgb(245, 245, 245);  // bright white (logo / numbers)
    let t_fg      = rgb(50, 255, 50);    // green CRT text
    let t_bg      = rgb(8, 16, 8);       // CRT background
    // Paddle switch base colors (blue = high nibble, red = low nibble).
    let sw_blue   = rgb(78, 104, 188);
    let sw_red    = rgb(196, 58, 44);

    // Panel rectangle.
    let panel_x: i32 = PX;
    let panel_y: i32 = PY;
    let panel_w: i32 = PW;
    let panel_bottom: i32 = PY + PH;

    while !rl.window_should_close() {
        // ---- Input: toggle switches (suppressed when picker is open) ----
        if matches!(picker, PickerState::Closed) && rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT) {
            let m = rl.get_mouse_position();

            // Helper: did the click land on the paddle centered at column `cx`?
            let hit_paddle = |cx: i32| -> bool {
                let left = (cx - PADDLE_W / 2) as f32;
                m.x >= left && m.x < left + PADDLE_W as f32
                    && m.y >= Y_SW as f32 && m.y < (Y_SW + PADDLE_H) as f32
            };

            // 16 address/data sense switches. Click upper half = set (1),
            // lower half = clear (0); a click anywhere just toggles.
            for i in 0..16usize {
                if hit_paddle(addr_col_x(i)) {
                    addr_sw[i] = m.y < (Y_SW + PADDLE_H / 2) as f32;
                }
            }

            // Control cluster (6 paddles). Each control has an "up" action
            // (top label) and a "down" action (bottom label). We decide which
            // by whether the click landed in the top or bottom half.
            let up = m.y < (Y_SW + PADDLE_H / 2) as f32;
            for i in 0..6usize {
                let cx = ctrl_col_x(i);
                let left = (cx - PADDLE_W / 2) as f32;
                if m.x >= left && m.x < left + PADDLE_W as f32
                    && m.y >= Y_SW as f32 && m.y < (Y_SW + PADDLE_H) as f32
                {
                    match i {
                        0 => emu.panel.press_switch(if up { PanelSwitch::Examine } else { PanelSwitch::ExamineNext }),
                        1 => emu.panel.press_switch(if up { PanelSwitch::Deposit } else { PanelSwitch::DepositNext }),
                        2 => {
                            // RESET (up) / EXT.CLR (down): restart CPU at 0
                            emu.cpu.pc = 0;
                            emu.cpu.halted = false;
                        }
                        3 => emu.panel.press_switch(PanelSwitch::RunStop), // RUN (up) / STOP (down)
                        4 => step_pending = true,                          // SINGLE STEP
                        5 => {}                                            // PWR ON/OFF (no-op)
                        _ => {}
                    }
                }
            }
        }

        // Keyboard shortcuts (F5 suppressed when picker is open)
        if matches!(picker, PickerState::Closed) && rl.is_key_pressed(KeyboardKey::KEY_F5) {
            emu.panel.press_switch(PanelSwitch::RunStop);
        }
        // F2: Open load picker (list .json programs in PROGRAMS/)
        if rl.is_key_pressed(KeyboardKey::KEY_F2) {
            let prog_dir = default_programs_dir();
            if let Ok(entries_fs) = fs::read_dir(&prog_dir) {
                let mut files: Vec<PickerEntry> = entries_fs
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                    .filter_map(|e| {
                        let path = e.path();
                        let entry = load_program_file(&path).ok()?;
                        Some(PickerEntry {
                            name: entry.name.clone(),
                            description: entry.description.clone(),
                            path,
                        })
                    })
                    .collect();
                files.sort_by(|a, b| a.name.cmp(&b.name));
                if files.is_empty() {
                    status_msg = "No .json programs in PROGRAMS/".to_string();
                    status_msg_timer = 180;
                } else {
                    picker = PickerState::Load { entries: files, scroll: 0, selected: 0 };
                }
            } else {
                status_msg = "PROGRAMS/ directory not found".to_string();
                status_msg_timer = 180;
            }
        }
        // F3: Open save picker
        if rl.is_key_pressed(KeyboardKey::KEY_F3) {
            let pc = emu.cpu.pc;
            let default_name = format!("save_{:04X}", pc);
            picker = PickerState::Save { filename: default_name, cursor_blink: 0 };
        }

        // Handle picker input
        // We check for load/save actions and store results, then apply after
        // the match to avoid borrow conflicts with picker.
        let mut picker_action: Option<PickerAction> = None;
        match &mut picker {
            PickerState::Closed => {}
            PickerState::Load { entries, scroll, selected } => {
                if rl.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
                    picker_action = Some(PickerAction::Cancel);
                } else if rl.is_key_pressed(KeyboardKey::KEY_UP) {
                    if *selected > 0 { *selected -= 1; }
                    if *selected < *scroll { *scroll = *selected; }
                } else if rl.is_key_pressed(KeyboardKey::KEY_DOWN) {
                    if *selected < entries.len() as i32 - 1 { *selected += 1; }
                    let visible_rows = 10;
                    if *selected >= *scroll + visible_rows { *scroll = *selected - visible_rows + 1; }
                } else if rl.is_key_pressed(KeyboardKey::KEY_ENTER) {
                    if let Some(entry) = entries.get(*selected as usize).cloned() {
                        picker_action = Some(PickerAction::Load(entry.path.clone()));
                    }
                }
                // Mouse click to select entry
                if rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT) {
                    let m = rl.get_mouse_position();
                    let overlay_x: i32 = 100;
                    let overlay_y: i32 = 150;
                    let overlay_w: i32 = 540;
                    let overlay_h: i32 = 400;
                    let list_y = overlay_y + 50;
                    let list_h = overlay_h - 68;
                    let row_h: i32 = 36;
                    if m.x >= overlay_x as f32 && m.x < (overlay_x + overlay_w) as f32
                        && m.y >= list_y as f32 && m.y < (list_y + list_h) as f32 {
                        let click_row = ((m.y - list_y as f32) / row_h as f32) as i32;
                        let new_sel = *scroll + click_row;
                        if new_sel >= 0 && (new_sel as usize) < entries.len() {
                            *selected = new_sel;
                        }
                    }
                }
            }
            PickerState::Save { filename, cursor_blink } => {
                *cursor_blink += 1;
                if rl.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
                    picker_action = Some(PickerAction::Cancel);
                } else if rl.is_key_pressed(KeyboardKey::KEY_ENTER) {
                    picker_action = Some(PickerAction::Save(filename.clone()));
                } else if rl.is_key_pressed(KeyboardKey::KEY_BACKSPACE) {
                    filename.pop();
                } else if let Some(ch) = rl.get_char_pressed() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                        filename.push(ch);
                    }
                }
            }
        }

        // Process picker actions (outside the borrow)
        if let Some(action) = picker_action {
            match action {
                PickerAction::Load(path) => {
                    match load_program_file(&path) {
                        Ok(prog) => {
                            eprintln!("Loaded: {} (PC=0x{:04X}, running={})", prog.name, emu.cpu.pc, emu.panel.is_running());
                            emu = Imsai8080::new();
                            execute_panel_program(&mut emu, &prog);
                            eprintln!("After execute: PC=0x{:04X}, running={}", emu.cpu.pc, emu.panel.is_running());
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
                            // If the program has a "run" step, the front panel is already in RUN mode
                            running = emu.panel.is_running();
                            status_msg = format!("Loaded: {} (PC={:04X})", program_name, emu.cpu.pc);
                            status_msg_timer = 180;
                        }
                        Err(e) => {
                            status_msg = format!("Error: {}", e);
                            status_msg_timer = 180;
                        }
                    }
                    picker = PickerState::Closed;
                }
                PickerAction::Save(filename) => {
                    let addr_val: u16 = addr_sw.iter().enumerate()
                        .fold(0u16, |a, (i, &on)| if on { a | (1 << (15 - i)) } else { a });
                    let start = addr_val;
                    let dump_len: u16 = 256;
                    let prog = memory_to_program(
                        &filename,
                        &format!("Saved from {:04X}, {} bytes", start, dump_len),
                        start, dump_len, &emu,
                    );
                    if fs::create_dir_all(default_programs_dir()).is_ok() {
                        let save_path = default_programs_dir().join(format!("{}.json", filename));
                        match save_program_file(&prog, &save_path) {
                            Ok(()) => {
                                status_msg = format!("Saved: {}", save_path.display());
                                status_msg_timer = 180;
                            }
                            Err(e) => {
                                status_msg = format!("Save error: {}", e);
                                status_msg_timer = 180;
                            }
                        }
                    }
                    picker = PickerState::Closed;
                }
                PickerAction::Cancel => {
                    picker = PickerState::Closed;
                }
            }
        }
        // R: Reset to UART test, STOPPED (only when picker is closed)
        if matches!(picker, PickerState::Closed) && rl.is_key_pressed(KeyboardKey::KEY_R) {
            emu = Imsai8080::new();
            emu.load_program(0x0000, &UART_TEST);
            emu.panel.set_address_switches(0x0000);
            emu.process_panel();
            addr_sw = [false; 16];
            cycles = 0;
            term = [[0x20u8; TERM_COLS]; TERM_ROWS];
            tcx = 0;
            tcy = 0;
            running = false;
            program_name = "UART Test".to_string();
        }

        // Keyboard input for terminal (only when running and picker closed)
        if running && matches!(picker, PickerState::Closed) {
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

        // ---- Update panel & sync visuals ----
        // User input already set addr_sw/data_sw via mouse clicks.
        // Forward those to the panel, then read back in case the panel
        // changed them (e.g., deposit_next auto-advances the address).
        let addr_val: u16 = addr_sw.iter().enumerate()
            .fold(0u16, |a, (i, &on)| if on { a | (1 << (15 - i)) } else { a });
        // Low 8 sense switches double as the data byte (real IMSAI behavior).
        let data_val: u8 = (addr_val & 0xFF) as u8;
        emu.panel.set_address_switches(addr_val);
        emu.panel.set_data_switches(data_val);
        emu.process_panel();

        // Sync visual switches from panel (catches auto-advances from
        // deposit_next, examine_next, and program loads)
        let panel_addr = emu.panel.address_switches();
        for i in 0..16 {
            addr_sw[i] = (panel_addr >> (15 - i)) & 1 != 0;
        }

        if step_pending {
            emu.single_step();
            step_pending = false;
        }

        // ---- Run ----
        // The CPU executes only while the panel is in RUN *and* the CPU has
        // not halted (HLT). run_batch stops on HLT, so once halted it returns
        // 0 and the cycle counter freezes; the front panel LEDs already show
        // the halt state (RUN off, WAIT + HLTA on).
        running = emu.panel.is_running() && !emu.cpu.halted;
        if running {
            cycles += emu.run_batch(10000);
        }
        // Always service the UART so any bytes still in the transmitter shift
        // out -- including the final ones emitted just before a HLT.
        if emu.panel.is_running() {
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
        }

        // ---- Draw ----
        let mut d = rl.begin_drawing(&thread);
        d.clear_background(bg);

        // === IMSAI 8080 front panel (black face) ===
        let leds = emu.panel.leds();

        // Panel face with a thin raised bevel against the gray chassis.
        d.draw_rectangle(panel_x - 3, panel_y - 3, panel_w + 6, PH + 6, rgb(150, 150, 154));
        d.draw_rectangle(panel_x, panel_y, panel_w, PH, panel_bg);
        d.draw_rectangle_lines(panel_x, panel_y, panel_w, PH, panel_edge);

        // Decorative mounting screws (corners + mid-edges).
        for &(sx, sy) in &[
            (panel_x + 22, panel_y + 110), (panel_x + 22, panel_y + 270),
            (panel_x + panel_w - 22, panel_y + 110), (panel_x + panel_w - 22, panel_y + 270),
            (PX + 452, panel_y + 270), (PX + 770, panel_y + 270),
        ] {
            draw_screw(&mut d, sx, sy);
        }

        // --- IMSAI 8080 logo (top right) ---
        // "IMSAI 8080" with a rule beneath it that leads into "MICROCOMPUTER
        // SYSTEM", which sits at the right end on the same line level.
        let big = "IMSAI  8080";
        let sub = "MICROCOMPUTER SYSTEM";
        let right_edge = PX + PW - 28;
        let big_y = PY + 14;
        if let Some(ref f) = logo_font {
            // Smooth scalable font.
            let big_size = 52.0_f32;
            let big_sp = 4.0;
            let big_w = f.measure_text(big, big_size, big_sp).x;
            let big_x = right_edge as f32 - big_w;
            d.draw_text_ex(f, big, Vector2::new(big_x, big_y as f32), big_size, big_sp, txt_bright);

            let sub_size = 15.0_f32;
            let sub_sp = 1.5;
            let sub_w = f.measure_text(sub, sub_size, sub_sp).x;
            let sub_x = right_edge as f32 - sub_w;
            let line_y = big_y + big_size as i32 + 5;
            d.draw_rectangle(big_x as i32 + 2, line_y, (sub_x as i32 - 10) - (big_x as i32 + 2), 2, txt_bright);
            d.draw_text_ex(f, sub, Vector2::new(sub_x, (line_y - 8) as f32), sub_size, sub_sp, txt);
        } else {
            // Fallback: built-in bitmap font.
            let big_size = 54;
            let big_w = d.measure_text(big, big_size);
            let big_x = right_edge - big_w;
            d.draw_text(big, big_x, big_y, big_size, txt_bright);
            let sub_size = 13;
            let sub_w = d.measure_text(sub, sub_size);
            let sub_x = right_edge - sub_w;
            let line_y = big_y + big_size + 2;
            d.draw_rectangle(big_x + 2, line_y, (sub_x - 10) - (big_x + 2), 2, txt_bright);
            d.draw_text(sub, sub_x, line_y - 6, sub_size, txt);
        }

        // ---------------------------------------------------------------
        // Row 1: PROGRAMMED OUTPUT (left byte block) -- not modeled, off.
        // ---------------------------------------------------------------
        for j in 0..8usize {
            let cx = byte_col_x(LBYTE_X0, j);
            d.draw_text(&format!("{}", 7 - j), cx - 3, Y_PROG - 26, 12, txt);
            draw_led(&mut d, cx, Y_PROG, false, led_on, led_off, led_glow);
        }
        d.draw_text("PROGRAMMED", PX + 332, Y_PROG - 14, 12, txt);
        d.draw_text("OUTPUT", PX + 332, Y_PROG + 2, 12, txt);

        // ---------------------------------------------------------------
        // Row 2: STATUS BYTE (left) + DATA BUS (right)
        // ---------------------------------------------------------------
        let status = [
            ("MEMR", leds.memr), ("INP", leds.ior), ("M1", leds.m1), ("OUT", leds.iow),
            ("HLTA", leds.hlta), ("STACK", false), ("WO", leds.mwrt), ("INTA", leds.int),
        ];
        for (j, (lbl, on)) in status.iter().enumerate() {
            let cx = byte_col_x(LBYTE_X0, j);
            let lw = d.measure_text(lbl, 9);
            d.draw_text(lbl, cx - lw / 2, Y_STAT - 24, 9, txt);
            draw_led(&mut d, cx, Y_STAT, *on, led_on, led_off, led_glow);
        }
        d.draw_text("STATUS", PX + 332, Y_STAT - 14, 12, txt);
        d.draw_text("BYTE", PX + 332, Y_STAT + 2, 12, txt);
        for j in 0..8usize {
            let cx = byte_col_x(RBYTE_X0, j);
            d.draw_text(&format!("{}", 7 - j), cx - 3, Y_STAT - 24, 12, txt);
            draw_led(&mut d, cx, Y_STAT, leds.data[j], led_on, led_off, led_glow);
        }
        d.draw_text("DATA", PX + 748, Y_STAT - 14, 12, txt);
        d.draw_text("BUS", PX + 748, Y_STAT + 2, 12, txt);

        // ---------------------------------------------------------------
        // Row 3: ADDRESS BUS (16 LEDs) with hex weight markers + mode LEDs.
        // ---------------------------------------------------------------
        for i in 0..16usize {
            let cx = addr_col_x(i);
            // Hex weight per nibble: 8 4 2 1.
            let weight = [8, 4, 2, 1][i % 4];
            d.draw_text(&format!("{}", weight), cx - 3, Y_ADDR - 24, 11, txt_dim);
            draw_led(&mut d, cx, Y_ADDR, leds.address[i], led_on, led_off, led_glow);
        }
        d.draw_text("ADDRESS", PX + 332, Y_ADDR - 14, 12, txt);
        d.draw_text("BUS", PX + 332, Y_ADDR + 2, 12, txt);
        d.draw_text("HEXADECIMAL", PX + 748, Y_ADDR - 26, 11, txt);
        d.draw_text("ADDRESS", PX + 748, Y_ADDR - 12, 12, txt);
        d.draw_text("BUS", PX + 748, Y_ADDR + 3, 12, txt);

        // Mode LEDs (right): INTERRUPTS ENABLED, RUN, WAIT, HOLD.
        let modes = [
            ("INTE", false), ("RUN", leds.run), ("WAIT", leds.wait), ("HOLD", leds.hlda),
        ];
        for (k, (lbl, on)) in modes.iter().enumerate() {
            let cx = ctrl_col_x(2 + k);
            let lw = d.measure_text(lbl, 10);
            d.draw_text(lbl, cx - lw / 2, Y_ADDR - 24, 10, txt);
            draw_led(&mut d, cx, Y_ADDR, *on, led_on, led_off, led_glow);
        }

        // ---------------------------------------------------------------
        // Switch row: 16 sense switches + 6 control paddles.
        // ---------------------------------------------------------------
        for i in 0..16usize {
            let cx = addr_col_x(i);
            d.draw_text(&format!("{}", 15 - i), cx - 4, Y_SW - 16, 10, txt);
            let base = if (i % 8) < 4 { sw_blue } else { sw_red };
            draw_paddle(&mut d, cx, Y_SW, base, addr_sw[i], panel_bg);
        }
        // Group labels under the two sense-switch byte blocks.
        let lbl1 = "ADDRESS - PROGRAMMED INPUT";
        let c1 = (byte_col_x(LBYTE_X0, 0) + byte_col_x(LBYTE_X0, 7)) / 2;
        let w1 = d.measure_text(lbl1, 10);
        d.draw_text(lbl1, c1 - w1 / 2, Y_SW + PADDLE_H + 4, 10, txt);
        let lbl2 = "ADDRESS - DATA";
        let c2 = (byte_col_x(RBYTE_X0, 0) + byte_col_x(RBYTE_X0, 7)) / 2;
        let w2 = d.measure_text(lbl2, 10);
        d.draw_text(lbl2, c2 - w2 / 2, Y_SW + PADDLE_H + 4, 10, txt);

        // Control paddles: (top label / bottom label, base color, up-state).
        let running_now = emu.panel.is_running();
        let controls: [(&str, &str, raylib::color::Color, bool); 6] = [
            ("EXAMINE", "EX NEXT", sw_blue, false),
            ("DEPOSIT", "DEP NEXT", sw_red, false),
            ("RESET", "EXT CLR", sw_blue, false),
            ("RUN", "STOP", sw_blue, running_now),
            ("SINGLE", "STEP", sw_blue, false),
            ("PWR ON", "PWR OFF", sw_red, true),
        ];
        for (i, (top, bot, base, up)) in controls.iter().enumerate() {
            let cx = ctrl_col_x(i);
            let tw = d.measure_text(top, 8);
            d.draw_text(top, cx - tw / 2, Y_SW - 14, 8, txt);
            draw_paddle(&mut d, cx, Y_SW, *base, *up, panel_bg);
            let bw = d.measure_text(bot, 8);
            d.draw_text(bot, cx - bw / 2, Y_SW + PADDLE_H + 4, 8, txt);
        }

        // Transient status message over the panel (load/save feedback).
        if status_msg_timer > 0 {
            let alpha = (if status_msg_timer < 30 { status_msg_timer * 8 } else { 255 }).min(255) as u8;
            d.draw_text(&status_msg, PX + 24, PY + PH - 18, 12, rgba(255, 180, 120, alpha));
            status_msg_timer -= 1;
        }

        // === Bottom bar: machine state + keyboard shortcuts ===
        d.draw_rectangle(0, H - 22, W, 22, rgb(28, 28, 30));
        let cpu = &emu.cpu;
        let state = if cpu.halted { "HALT" } else if running_now { "RUN " } else { "STOP" };
        let line = format!(
            "{}  PC:{:04X} SP:{:04X} A:{:02X} BC:{:02X}{:02X} DE:{:02X}{:02X} HL:{:02X}{:02X}   {}   cyc:{}",
            state, cpu.pc, cpu.sp, cpu.a, cpu.b, cpu.c, cpu.d, cpu.e, cpu.h, cpu.l,
            program_name, cycles,
        );
        d.draw_text(&line, 10, H - 16, 11, txt_dim);
        d.draw_text("F5 run/stop  F2 load  F3 save  R reset", W - 320, H - 16, 11, txt_dim);

        // === CRT Terminal display (below panel) ===
        let term_y: i32 = panel_bottom + 10;
        let term_h: i32 = H - term_y - 28;
        let term_x: i32 = 8;
        let term_w: i32 = W - 16;
        // CRT bezel (dark rounded border)
        d.draw_rectangle(term_x - 4, term_y - 4, term_w + 8, term_h + 8, raylib::color::Color { r: 40, g: 38, b: 35, a: 255 });
        d.draw_rectangle_lines(term_x - 4, term_y - 4, term_w + 8, term_h + 8, panel_edge);
        // CRT screen (dark green phosphor)
        d.draw_rectangle(term_x, term_y, term_w, term_h, t_bg);
        // Scanline effect: subtle horizontal lines
        let scanline_col = raylib::color::Color { r: 10, g: 22, b: 10, a: 40 };
        for scan_y in (term_y..term_y + term_h).step_by(3) {
            d.draw_rectangle(term_x, scan_y, term_w, 1, scanline_col);
        }
        d.draw_text("CONSOLE", term_x + 4, term_y + 2, 10, raylib::color::Color { r: 30, g: 80, b: 30, a: 180 });

        let term_text_y = term_y + 16;
        let term_text_x = term_x + 4;
        for row in 0..TERM_ROWS {
            let row_top = term_text_y + row as i32 * TERM_CHAR_H;
            if row_top + TERM_CHAR_H > term_y + term_h { break; }
            for col in 0..TERM_COLS {
                let ch = term[row][col];
                // Only render printable ASCII; suppress high bytes that
                // would map to garbled Unicode glyphs.
                if ch >= 0x20 && ch <= 0x7E {
                    d.draw_text(
                        &format!("{}", ch as char),
                        term_text_x + col as i32 * TERM_CHAR_W,
                        row_top,
                        10, t_fg
                    );
                }
            }
        }

        // === I/O log (right side of terminal area) ===
        let iolog_x = term_x + TERM_COLS as i32 * TERM_CHAR_W + 20;
        if iolog_x + 140 < term_x + term_w {
            d.draw_text("I/O LOG", iolog_x, term_y + 4, 10, raylib::color::Color { r: 30, g: 80, b: 30, a: 180 });
            let io_log = emu.panel.io_log();
            let log_start = io_log.len().saturating_sub(10);
            for (i, ev) in io_log[log_start..].iter().enumerate() {
                let dir = if ev.is_write { "OUT" } else { "IN " };
                let kind = if ev.is_io { "IO" } else { "MEM" };
                d.draw_text(
                    &format!("{} {:04X} {:02X} {}", dir, ev.address, ev.data, kind),
                    iolog_x, term_y + 18 + i as i32 * 13, 9, txt_dim
                );
            }
        }

        // === File picker overlay ===
        if !matches!(picker, PickerState::Closed) {
            let overlay_x: i32 = 200;
            let overlay_y: i32 = 150;
            let overlay_w: i32 = 680;
            let overlay_h: i32 = 420;
            // Dim background
            d.draw_rectangle(0, 0, W, H, raylib::color::Color { r: 0, g: 0, b: 0, a: 160 });
            // Panel background (uses front panel aluminum color)
            d.draw_rectangle(overlay_x, overlay_y, overlay_w, overlay_h, panel_bg);
            d.draw_rectangle_lines(overlay_x, overlay_y, overlay_w, overlay_h, panel_edge);

            match &picker {
                PickerState::Load { entries, scroll, selected } => {
                    d.draw_text("LOAD PROGRAM", overlay_x + 10, overlay_y + 8, 16, txt_bright);
                    d.draw_text("Up/Down = navigate    Enter = load    Esc = cancel", overlay_x + 10, overlay_y + 28, 10, txt_dim);
                    let list_y = overlay_y + 50;
                    let list_h = overlay_h - 68;
                    let row_h: i32 = 36;
                    let visible_rows = (list_h / row_h) as usize;
                    d.draw_rectangle(overlay_x + 5, list_y, overlay_w - 10, list_h, raylib::color::Color { r: 30, g: 30, b: 28, a: 255 });
                    for i in 0..visible_rows {
                        let idx = *scroll as usize + i;
                        if idx >= entries.len() { break; }
                        let entry = &entries[idx];
                        let row_y = list_y + i as i32 * row_h;
                        let is_sel = idx == *selected as usize;
                        if is_sel {
                            d.draw_rectangle(overlay_x + 5, row_y, overlay_w - 10, row_h, raylib::color::Color { r: 60, g: 30, b: 20, a: 255 });
                        }
                        d.draw_text(&entry.name, overlay_x + 15, row_y + 4, 15, if is_sel { led_on } else { txt_bright });
                        let desc_max_chars = ((overlay_w - 30) / 6) as usize;
                        let desc: String = entry.description.chars().take(desc_max_chars).collect();
                        d.draw_text(&desc, overlay_x + 15, row_y + 20, 10, txt_dim);
                    }
                    if *scroll > 0 {
                        d.draw_text("  ^ more above ^", overlay_x + 10, list_y - 14, 9, txt_dim);
                    }
                    if (*scroll as usize + visible_rows) < entries.len() {
                        d.draw_text("  v more below v", overlay_x + 10, list_y + list_h + 2, 9, txt_dim);
                    }
                    d.draw_text(&format!("{} / {} entries", selected + 1, entries.len()), overlay_x + 10, overlay_y + overlay_h - 18, 10, txt_dim);
                }
                PickerState::Save { filename, cursor_blink } => {
                    d.draw_text("SAVE PROGRAM (Enter = save, Esc = cancel)", overlay_x + 10, overlay_y + 8, 14, txt_bright);
                    d.draw_text("Saved to PROGRAMS/<name>.json", overlay_x + 10, overlay_y + 28, 10, txt_dim);
                    d.draw_text("Filename:", overlay_x + 10, overlay_y + 56, 14, txt);
                    let input_y = overlay_y + 76;
                    d.draw_rectangle(overlay_x + 10, input_y, overlay_w - 20, 30, raylib::color::Color { r: 20, g: 20, b: 18, a: 255 });
                    d.draw_rectangle_lines(overlay_x + 10, input_y, overlay_w - 20, 30, panel_edge);
                    let display_name = if *cursor_blink / 20 % 2 == 0 {
                        format!("{}.json|", filename)
                    } else {
                        format!("{}.json ", filename)
                    };
                    d.draw_text(&display_name, overlay_x + 16, input_y + 6, 16, led_on);
                    d.draw_text("Saves 256 bytes from address switches position", overlay_x + 10, overlay_y + 120, 10, txt_dim);
                    let addr_val_display: u16 = addr_sw.iter().enumerate()
                        .fold(0u16, |a, (i, &on)| if on { a | (1 << (15 - i)) } else { a });
                    d.draw_text(&format!("Start address: {:04X}", addr_val_display), overlay_x + 10, overlay_y + 140, 14, txt);
                    d.draw_text("Length: 256 bytes (0x100)", overlay_x + 10, overlay_y + 160, 14, txt);
                }
                PickerState::Closed => {}
            }
        }

        // === End picker overlay ===

        drop(d);

        // Headless verification: after a few warm-up frames, save a screenshot
        // and exit. (`--shot <path>`)
        frame += 1;
        if let Some(ref shot_path) = shot_arg {
            if frame >= 3 {
                rl.take_screenshot(&thread, shot_path);
                break;
            }
        }
    }
}

/// Lighten a color by `amt` (clamped to 255).
fn lighten(c: raylib::color::Color, amt: u8) -> raylib::color::Color {
    raylib::color::Color {
        r: c.r.saturating_add(amt),
        g: c.g.saturating_add(amt),
        b: c.b.saturating_add(amt),
        a: c.a,
    }
}

/// Darken a color by `amt` (clamped to 0).
fn darken(c: raylib::color::Color, amt: u8) -> raylib::color::Color {
    raylib::color::Color {
        r: c.r.saturating_sub(amt),
        g: c.g.saturating_sub(amt),
        b: c.b.saturating_sub(amt),
        a: c.a,
    }
}

/// Draw a round red panel LED centered at (cx, cy).
fn draw_led(
    d: &mut raylib::drawing::RaylibDrawHandle,
    cx: i32, cy: i32, on: bool,
    on_color: raylib::color::Color,
    off_color: raylib::color::Color,
    glow: raylib::color::Color,
) {
    let cxf = cx as f32;
    let cyf = cy as f32;
    // Dark bezel ring.
    d.draw_circle(cx, cy, LED_RAD + 2.5, raylib::color::Color { r: 30, g: 30, b: 32, a: 255 });
    if on {
        d.draw_circle(cx, cy, LED_RAD + 4.0, glow);
    }
    d.draw_circle(cx, cy, LED_RAD, if on { on_color } else { off_color });
    if on {
        // Specular highlight.
        d.draw_circle(cx - 2, cy - 2, 2.0, lighten(on_color, 90));
    } else {
        d.draw_circle(cxf as i32 - 2, cyf as i32 - 2, 1.5, raylib::color::Color { r: 80, g: 24, b: 20, a: 255 });
    }
}

/// Draw a decorative chrome mounting screw centered at (cx, cy).
fn draw_screw(d: &mut raylib::drawing::RaylibDrawHandle, cx: i32, cy: i32) {
    d.draw_circle(cx, cy, 7.0, raylib::color::Color { r: 22, g: 22, b: 24, a: 255 });
    d.draw_circle(cx, cy, 5.5, raylib::color::Color { r: 120, g: 120, b: 124, a: 255 });
    d.draw_circle(cx - 1, cy - 1, 2.0, raylib::color::Color { r: 175, g: 175, b: 178, a: 255 });
    // Slot.
    d.draw_rectangle(cx - 4, cy - 1, 8, 2, raylib::color::Color { r: 50, g: 50, b: 52, a: 255 });
}

/// Draw an IMSAI paddle toggle switch centered horizontally on `cx`, with its
/// top edge at `top_y`. `base` is the paddle color (blue/red); `up` tilts the
/// glossy face toward the top (switch in the up/1 position).
fn draw_paddle(
    d: &mut raylib::drawing::RaylibDrawHandle,
    cx: i32, top_y: i32,
    base: raylib::color::Color,
    up: bool,
    _bezel: raylib::color::Color,
) {
    use raylib::math::Rectangle;
    let w = PADDLE_W as f32;
    let h = PADDLE_H as f32;
    let x = (cx - PADDLE_W / 2) as f32;
    let y = top_y as f32;

    // Soft recessed shadow under the paddle (no hard outline box).
    d.draw_rectangle_rounded(
        Rectangle { x: x - 1.0, y: y + 2.0, width: w + 2.0, height: h },
        0.45, 8, raylib::color::Color { r: 0, g: 0, b: 0, a: 130 },
    );

    // Paddle body: a rounded rectangle in the base color.
    let body = Rectangle { x, y, width: w, height: h };
    d.draw_rectangle_rounded(body, 0.4, 8, base);

    // Vertical shading: the half tilted toward the viewer is brighter, the
    // receding half darker. Up = top half forward, down = bottom half forward.
    let light = lighten(base, 60);
    let dark = darken(base, 60);
    let half = (h / 2.0) - 1.0;
    let (fwd_y, rcv_y) = if up { (y, y + h / 2.0) } else { (y + h / 2.0, y) };
    d.draw_rectangle_rounded(
        Rectangle { x: x + 2.0, y: rcv_y + 1.0, width: w - 4.0, height: half }, 0.5, 6, dark,
    );
    d.draw_rectangle_rounded(
        Rectangle { x: x + 2.0, y: fwd_y + 1.0, width: w - 4.0, height: half }, 0.5, 6, light,
    );

    // Bright gloss band + specular highlight near the forward edge.
    let band_y = if up { top_y + 5 } else { top_y + PADDLE_H - 12 };
    d.draw_rectangle(cx - PADDLE_W / 2 + 4, band_y, PADDLE_W - 8, 6, lighten(base, 95));
    d.draw_circle(cx - 4, band_y + 2, 1.8, lighten(base, 130));
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