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
use raylib::consts::{KeyboardKey, MouseButton};
use serde::{Deserialize, Serialize};

// Window size
const W: i32 = 1280;
const H: i32 = 860;

// LED dimensions
const LED_R: i32 = 6;    // radius of circular LEDs
const LED_GAP: i32 = 26; // center-to-center spacing

// Switch dimensions (IMSAI paddle toggles)
const SW_W: i32 = 16;
const SW_H: i32 = 50;
const SW_GAP: i32 = 27;
const SW_PADDLE_H: i32 = 20; // visible paddle height
const SW_PLATE_H: i32 = 4;   // tip cap thickness

// Button dimensions
const BTN_W: i32 = 76;
const BTN_H: i32 = 32;
const BTN_GAP: i32 = 8;

// Terminal dimensions
const TERM_COLS: usize = 80;
const TERM_ROWS: usize = 24;
const TERM_CHAR_W: i32 = 7;
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
        .title("IMSAI 8080 Microcomputer")
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
    let mut status_msg = String::new();
    let mut status_msg_timer: i32 = 0; // frames remaining
    let mut picker = PickerState::Closed;

    // Colors - IMSAI warm aluminum/chassis palette
    let bg        = raylib::color::Color { r: 20, g: 20, b: 24, a: 255 };   // dark surround
    let panel_bg  = raylib::color::Color { r: 180, g: 175, b: 162, a: 255 };  // warm aluminum
    let _panel_bg2 = raylib::color::Color { r: 165, g: 160, b: 148, a: 255 };  // slightly darker accent
    let panel_edge= raylib::color::Color { r: 100, g: 96, b: 85, a: 255 };   // panel border/bevel
    let led_on    = raylib::color::Color { r: 255, g: 40, b: 30, a: 255 };   // IMSAI red LED
    let led_off   = raylib::color::Color { r: 50, g: 45, b: 42, a: 255 };   // dark LED (off)
    let led_glow  = raylib::color::Color { r: 255, g: 80, b: 60, a: 80 };   // LED glow halo
    let led_red   = raylib::color::Color { r: 255, g: 50, b: 20, a: 255 };   // status LED (same red)
    // Switch colors: paddle is dark metal, slot is black recess
    let sw_paddle = raylib::color::Color { r: 55, g: 55, b: 58, a: 255 };   // dark metal paddle
    let sw_paddle_hi = raylib::color::Color { r: 80, g: 80, b: 84, a: 255 }; // paddle highlight
    let sw_paddle_lo = raylib::color::Color { r: 30, g: 30, b: 32, a: 255 }; // paddle shadow
    let sw_slot   = raylib::color::Color { r: 10, g: 10, b: 12, a: 255 };    // deep black slot
    let sw_slot_rim = raylib::color::Color { r: 70, g: 68, b: 62, a: 255 };  // slot chrome rim
    let sw_tip_on  = raylib::color::Color { r: 220, g: 50, b: 30, a: 255 };  // red tip for ON (up)
    let sw_tip_off = raylib::color::Color { r: 35, g: 35, b: 38, a: 255 };   // dark tip for OFF (down)
    let txt       = raylib::color::Color { r: 30, g: 25, b: 20, a: 255 };   // dark text on aluminum
    let txt_dim   = raylib::color::Color { r: 80, g: 76, b: 68, a: 255 };   // dim text on aluminum
    let txt_bright= raylib::color::Color { r: 20, g: 18, b: 14, a: 255 };   // bright label text
    let t_fg      = raylib::color::Color { r: 50, g: 255, b: 50, a: 255 };  // green CRT text
    let t_bg      = raylib::color::Color { r: 8, g: 16, b: 8, a: 255 };    // CRT background
    let _border    = raylib::color::Color { r: 60, g: 58, b: 52, a: 255 };   // border lines
    // Momentary pushbutton colors (silver/chrome buttons)
    let mom_face  = raylib::color::Color { r: 155, g: 150, b: 140, a: 255 };
    let mom_hi    = raylib::color::Color { r: 190, g: 185, b: 175, a: 255 };
    let mom_lo    = raylib::color::Color { r: 90, g: 86, b: 78, a: 255 };
    let mom_text  = raylib::color::Color { r: 20, g: 18, b: 14, a: 255 };

    // Layout: Panel takes upper portion, CRT terminal below
    // Panel area: full width, y=0 to panel_bottom
    // CRT terminal: full width, below panel
    let panel_x: i32 = 8;               // panel left margin
    let panel_y: i32 = 4;                // panel top margin
    let panel_w: i32 = W - 16;           // panel width (almost full)
    let panel_bottom: i32 = 510;         // bottom edge of front panel area

    while !rl.window_should_close() {
        // ---- Input: toggle switches (suppressed when picker is open) ----
        if matches!(picker, PickerState::Closed) && rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT) {
            let m = rl.get_mouse_position();

            // Address switches row  
            // Layout: panel_x + 16 + i * SW_GAP, at addr_sw_y
            let addr_sw_y: f32 = 228.0;
            for i in 0..16usize {
                let x = (panel_x + 16 + i as i32 * SW_GAP) as f32;
                let w = SW_W as f32;
                let h = SW_H as f32;
                if m.x >= x && m.x < x + w && m.y >= addr_sw_y && m.y < addr_sw_y + h {
                    addr_sw[i] = !addr_sw[i];
                }
            }

            // Data switches row
            let data_sw_y: f32 = 305.0;
            for i in 0..8usize {
                let x = (panel_x + 16 + i as i32 * SW_GAP) as f32;
                if m.x >= x && m.x < x + SW_W as f32 && m.y >= data_sw_y && m.y < data_sw_y + SW_H as f32 {
                    data_sw[i] = !data_sw[i];
                }
            }

            // Function controls at ctrl_y
            let ctrl_y: i32 = 380;
            // RUN/STOP toggle switch
            let runstop_x = panel_x + 16;
            if m.x >= runstop_x as f32 && m.x < (runstop_x + SW_W) as f32
                && m.y >= ctrl_y as f32 && m.y < (ctrl_y + SW_H) as f32 {
                emu.panel.press_switch(PanelSwitch::RunStop);
            }

            // Momentary buttons (STEP, EXAM, DEP, EX NXT, DEP NXT)
            let mom_start_x = (panel_x + 16 + SW_W + 16) as i32;
            let mom_actions = [PanelSwitch::SingleStep, PanelSwitch::Examine,
                               PanelSwitch::Deposit, PanelSwitch::ExamineNext,
                               PanelSwitch::DepositNext];
            for (i, action) in mom_actions.iter().enumerate() {
                let x = (mom_start_x + i as i32 * (BTN_W + BTN_GAP)) as f32;
                if m.x >= x && m.x < x + BTN_W as f32 && m.y >= ctrl_y as f32 && m.y < (ctrl_y + BTN_H) as f32 {
                    if *action == PanelSwitch::SingleStep {
                        step_pending = true;
                    } else {
                        emu.panel.press_switch(*action);
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
            data_sw = [false; 8];
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
        let data_val: u8 = data_sw.iter().enumerate()
            .fold(0u8, |a, (i, &on)| if on { a | (1 << (7 - i)) } else { a });
        emu.panel.set_address_switches(addr_val);
        emu.panel.set_data_switches(data_val);
        emu.process_panel();

        // Sync visual switches from panel (catches auto-advances from
        // deposit_next, examine_next, and program loads)
        let panel_addr = emu.panel.address_switches();
        let panel_data = emu.panel.data_switches();
        for i in 0..16 {
            addr_sw[i] = (panel_addr >> (15 - i)) & 1 != 0;
        }
        for i in 0..8 {
            data_sw[i] = (panel_data >> (7 - i)) & 1 != 0;
        }

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

        // === IMSAI Front Panel (full-width aluminum chassis) ===
        // Panel background with beveled edge
        d.draw_rectangle(panel_x, panel_y, panel_w, panel_bottom, panel_bg);
        // Inner dark border (recessed panel effect)
        d.draw_rectangle(panel_x + 2, panel_y + 2, panel_w - 4, panel_bottom - panel_y - 4, panel_bg);
        // Outer bevel
        let bevel_hi = raylib::color::Color { r: 210, g: 205, b: 195, a: 255 };
        let bevel_lo = raylib::color::Color { r: 120, g: 115, b: 105, a: 255 };
        // Top highlight
        d.draw_rectangle(panel_x, panel_y, panel_w, 2, bevel_hi);
        d.draw_rectangle(panel_x, panel_y, 2, panel_bottom - panel_y, bevel_hi);
        // Bottom shadow
        d.draw_rectangle(panel_x, panel_y + panel_bottom - panel_y - 2, panel_w, 2, bevel_lo);
        d.draw_rectangle(panel_x + panel_w - 2, panel_y, 2, panel_bottom - panel_y, bevel_lo);

        // Title bar: IMSAI 8080 badge
        d.draw_text("IMSAI 8080", panel_x + 14, panel_y + 8, 24, txt_bright);
        let state_str = if emu.panel.is_running() { "RUNNING" } else { "STOPPED" };
        let state_col = if emu.panel.is_running() { led_on } else { led_red };
        d.draw_text(state_str, panel_x + 240, panel_y + 12, 18, state_col);
        d.draw_text(&format!("Cycles: {}", cycles), panel_x + 400, panel_y + 14, 11, txt_dim);
        if !program_name.is_empty() {
            d.draw_text(&program_name, panel_x + 14, panel_y + 36, 11, txt_dim);
        }

        // Status message (fades out over panel)
        if status_msg_timer > 0 {
            let alpha = if status_msg_timer < 30 { status_msg_timer * 8 } else { 255 }.min(255) as u8;
            let msg_col = raylib::color::Color { r: 40, g: 20, b: 10, a: alpha };
            d.draw_text(&status_msg, panel_x + 14, panel_y + panel_bottom - panel_y - 22, 11, msg_col);
            status_msg_timer -= 1;
        }

        // Section separator line under title
        let sep_y = panel_y + 50;
        d.draw_rectangle(panel_x + 8, sep_y, panel_w - 16, 1, panel_edge);

        // === Address LEDs (y = 70) ===
        let addr_led_y: i32 = 70;
        d.draw_text("ADDRESS", panel_x + 14, addr_led_y - 14, 11, txt_bright);
        let leds = emu.panel.leds();
        for i in 0..16usize {
            let x = panel_x + 14 + (15 - i) as i32 * LED_GAP;
            let on = leds.address[i];
            // Draw circular LED with glow
            if on {
                d.draw_circle(x + LED_R, addr_led_y + LED_R, (LED_R + 4) as f32, led_glow);
            }
            d.draw_circle(x + LED_R, addr_led_y + LED_R, LED_R as f32, if on { led_on } else { led_off });
            d.draw_circle_lines(x + LED_R, addr_led_y + LED_R, LED_R as f32, panel_edge);
            // Bit number labels every 4 bits
            if i % 4 == 0 {
                d.draw_text(&format!("{}", 15 - i), x, addr_led_y + LED_R * 2 + 3, 8, txt_dim);
            }
            // Group separator lines every 4 bits
            if i > 0 && i % 4 == 0 {
                let sep_x = x - LED_GAP / 2;
                d.draw_rectangle(sep_x, addr_led_y - 2, 1, LED_R * 2 + 4, panel_edge);
            }
        }
        d.draw_text(&format!("= {:04X}", addr_val), panel_x + 14 + 16 * LED_GAP + 8, addr_led_y + 1, 13, txt_bright);

        // === Data LEDs (y = 125) ===
        let data_led_y: i32 = 125;
        d.draw_text("DATA", panel_x + 14, data_led_y - 14, 11, txt_bright);
        for i in 0..8usize {
            let x = panel_x + 14 + (7 - i) as i32 * LED_GAP;
            let on = leds.data[i];
            if on {
                d.draw_circle(x + LED_R, data_led_y + LED_R, (LED_R + 4) as f32, led_glow);
            }
            d.draw_circle(x + LED_R, data_led_y + LED_R, LED_R as f32, if on { led_on } else { led_off });
            d.draw_circle_lines(x + LED_R, data_led_y + LED_R, LED_R as f32, panel_edge);
        }
        // Group separator between high and low nibble
        let nib_sep = panel_x + 14 + 4 * LED_GAP;
        d.draw_rectangle(nib_sep - LED_GAP / 2, data_led_y - 2, 1, LED_R * 2 + 4, panel_edge);
        d.draw_text(&format!("= {:02X}", data_val), panel_x + 14 + 8 * LED_GAP + 8, data_led_y + 1, 13, txt_bright);

        // === Status LEDs (y = 178) ===
        let status_y: i32 = 178;
        d.draw_text("STATUS", panel_x + 14, status_y - 14, 11, txt_bright);
        let status = [
            ("RUN", leds.run, led_on), ("M1", leds.m1, led_on),
            ("WAIT", leds.wait, led_red), ("MEMR", leds.memr, led_on),
            ("MWRT", leds.mwrt, led_red), ("IOR", leds.ior, led_on),
            ("IOW", leds.iow, led_on), ("PWR", leds.power, led_on),
        ];
        for (i, (lbl, on, oc)) in status.iter().enumerate() {
            let x = panel_x + 14 + i as i32 * 70;
            if *on {
                d.draw_circle(x + 6, status_y + 6, 10.0_f32, led_glow);
            }
            d.draw_circle(x + 6, status_y + 6, 6.0_f32, if *on { *oc } else { led_off });
            d.draw_circle_lines(x + 6, status_y + 6, 6.0_f32, panel_edge);
            d.draw_text(lbl, x + 16, status_y, 9, txt);
        }

        // Separator before switches
        let sep2_y: i32 = 205;
        d.draw_rectangle(panel_x + 8, sep2_y, panel_w - 16, 1, panel_edge);

        // === Address switches (y = 228) ===
        let addr_sw_y: i32 = 228;
        d.draw_text(&format!("ADDR {:04X}", addr_val), panel_x + 14, addr_sw_y - 14, 13, txt_bright);
        // Bit labels above switches
        for i in 0..16usize {
            let x = panel_x + 16 + i as i32 * SW_GAP;
            let bit_num = 15 - i;
            d.draw_text(&format!("{}", bit_num), x + 1, addr_sw_y - 6, 7, txt_dim);
        }
        for i in 0..16usize {
            let x = panel_x + 16 + i as i32 * SW_GAP;
            draw_toggle_switch(&mut d, x, addr_sw_y, SW_W, SW_H, SW_PADDLE_H, SW_PLATE_H,
                addr_sw[i], sw_slot, sw_slot_rim, sw_paddle, sw_paddle_hi, sw_paddle_lo,
                sw_tip_on, sw_tip_off);
        }

        // === Data switches (y = 305) ===
        let data_sw_draw_y: i32 = 305;
        d.draw_text(&format!("DATA {:02X}", data_val), panel_x + 14, data_sw_draw_y - 14, 13, txt_bright);
        // Bit labels above switches
        for i in 0..8usize {
            let x = panel_x + 16 + i as i32 * SW_GAP;
            let bit_num = 7 - i;
            d.draw_text(&format!("{}", bit_num), x + 3, data_sw_draw_y - 6, 7, txt_dim);
        }
        for i in 0..8usize {
            let x = panel_x + 16 + i as i32 * SW_GAP;
            draw_toggle_switch(&mut d, x, data_sw_draw_y, SW_W, SW_H, SW_PADDLE_H, SW_PLATE_H,
                data_sw[i], sw_slot, sw_slot_rim, sw_paddle, sw_paddle_hi, sw_paddle_lo,
                sw_tip_on, sw_tip_off);
        }

        // Separator before controls
        let sep3_y: i32 = data_sw_draw_y + SW_H + 10;
        d.draw_rectangle(panel_x + 8, sep3_y, panel_w - 16, 1, panel_edge);

        // === Function controls (y = 380) ===
        let ctrl_y: i32 = 380;
        d.draw_text("CONTROLS", panel_x + 14, ctrl_y - 16, 11, txt_bright);

        // RUN/STOP toggle switch (red paddle, shows RUN/STOP state)
        let runstop_x = panel_x + 16;
        draw_toggle_switch(&mut d, runstop_x, ctrl_y, SW_W, SW_H, SW_PADDLE_H, SW_PLATE_H,
            emu.panel.is_running(), sw_slot, sw_slot_rim, sw_paddle, sw_paddle_hi, sw_paddle_lo,
            raylib::color::Color { r: 220, g: 50, b: 30, a: 255 },  // bright red tip ON
            sw_tip_off);
        d.draw_text("RUN/", runstop_x - 2, ctrl_y + SW_H + 2, 8, txt_dim);
        d.draw_text("STOP", runstop_x - 2, ctrl_y + SW_H + 12, 8, txt_dim);

        // Momentary push-buttons (STEP, EXAM, DEP, EX NXT, DEP NXT)
        let mom_labels = ["STEP", "EXAM", "DEP", "EX NXT", "DEP NXT"];
        let mom_start_x = panel_x + 16 + SW_W + 16;
        for (i, lbl) in mom_labels.iter().enumerate() {
            let x = mom_start_x + i as i32 * (BTN_W + BTN_GAP);
            // 3D raised button: highlight on top/left, shadow on bottom/right
            d.draw_rectangle(x, ctrl_y, BTN_W, BTN_H, mom_hi);
            d.draw_rectangle(x, ctrl_y + 2, BTN_W, BTN_H, mom_face);
            d.draw_rectangle(x, ctrl_y + BTN_H - 2, BTN_W, 2, mom_lo);
            d.draw_rectangle(x + BTN_W - 2, ctrl_y, 2, BTN_H, mom_lo);
            d.draw_rectangle_lines(x, ctrl_y, BTN_W, BTN_H, panel_edge);
            let text_w = d.measure_text(lbl, 9);
            d.draw_text(lbl, x + (BTN_W - text_w) / 2, ctrl_y + 10, 9, mom_text);
        }

        // === Registers and memory (right side of controls row) ===
        let reg_x = mom_start_x + 5 * (BTN_W + BTN_GAP) - BTN_GAP;
        d.draw_text("REGISTERS", reg_x, ctrl_y - 16, 11, txt_bright);
        let cpu = &emu.cpu;
        d.draw_text(&format!("A:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X}", cpu.a, cpu.b, cpu.c, cpu.d, cpu.e), reg_x, ctrl_y, 10, txt);
        d.draw_text(&format!("H:{:02X} L:{:02X} SP:{:04X} PC:{:04X}", cpu.h, cpu.l, cpu.sp, cpu.pc), reg_x, ctrl_y + 14, 10, txt);
        let f = &cpu.flags;
        d.draw_text(&format!("S{} Z{} AC{} P{} CY{}", f.s as u8, f.z as u8, f.ac as u8, f.p as u8, f.cy as u8), reg_x, ctrl_y + 28, 10, txt_dim);

        // Memory at PC
        let mem_y = ctrl_y + 46;
        d.draw_text("MEMORY AT PC", reg_x, mem_y, 10, txt_dim);
        let pc = emu.cpu.pc;
        for i in 0..4usize {
            let addr = pc.wrapping_add(i as u16);
            let val = emu.bus.mem_read(addr);
            d.draw_text(&format!("{:04X}: {:02X}", addr, val), reg_x, mem_y + 14 + i as i32 * 13, 9, txt_dim);
        }

        // === Bottom bar: keyboard shortcuts ===
        d.draw_rectangle(0, H - 24, W, 24, raylib::color::Color { r: 30, g: 28, b: 26, a: 255 });
        d.draw_text("F5: Run/Stop    F2: Load Program    F3: Save    R: Reset", 12, H - 18, 11, txt_dim);

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