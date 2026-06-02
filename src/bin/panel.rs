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
use rust_imsai_emulator::Imsai8080;
use rust_imsai_emulator::TarbellCard;
use rust_imsai_emulator::cards::PanelSwitch;
use raylib::consts::{KeyboardKey, MouseButton};
use raylib::prelude::RaylibDraw;

// Window size
const W: i32 = 1100;
const H: i32 = 720;

// LED dimensions
const LED_SIZE: i32 = 14;
const LED_GAP: i32 = 22;

// Switch dimensions
const SW_W: i32 = 22;
const SW_H: i32 = 36;
const SW_GAP: i32 = 28;

// Button dimensions
const BTN_W: i32 = 80;
const BTN_H: i32 = 28;
const BTN_GAP: i32 = 8;

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

    let (mut rl, thread) = raylib::init()
        .size(W, H)
        .title("IMSAI 8080")
        .build();

    rl.set_target_fps(30);

    let mut emu = Imsai8080::new();
    let mut addr_sw = [false; 16];
    let mut data_sw = [false; 8];

    // Load program/disk based on arguments
    let auto_run = if !bare {
        if let Some(ref path) = disk_arg {
            match emu.bus.card_mut::<TarbellCard>().unwrap().insert_disk(0, path) {
                Ok(()) => {
                    boot_cpm(&mut emu);
                    emu.panel.press_switch(PanelSwitch::RunStop);
                    emu.process_panel();
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
                    emu.panel.press_switch(PanelSwitch::RunStop);
                    emu.process_panel();
                }
                Err(e) => eprintln!("Error loading '{}': {}", path, e),
            }
            true
        } else {
            // Default: load UART test program
            emu.load_program(0x0000, &UART_TEST);
            emu.panel.set_address_switches(0x0000);
            emu.panel.press_switch(PanelSwitch::RunStop);
            emu.process_panel();
            true
        }
    } else {
        false
    };

    if auto_run {
        let start: u16 = 0x0000;
        for i in 0..16 {
            addr_sw[i] = (start >> (15 - i)) & 1 != 0;
        }
    }

    let mut term = [[0x20u8; TERM_COLS]; TERM_ROWS];
    let mut tcx: usize = 0;
    let mut tcy: usize = 0;

    let mut running = false;
    let mut cycles: u64 = 0;
    let mut step_pending = false;

    // Colors
    let bg        = raylib::color::Color { r: 25, g: 25, b: 30, a: 255 };
    let panel_bg  = raylib::color::Color { r: 40, g: 38, b: 36, a: 255 };
    let led_on    = raylib::color::Color { r: 0, g: 255, b: 60, a: 255 };
    let led_off   = raylib::color::Color { r: 30, g: 30, b: 30, a: 255 };
    let led_red   = raylib::color::Color { r: 255, g: 40, b: 40, a: 255 };
    let sw_on     = raylib::color::Color { r: 220, g: 200, b: 160, a: 255 };
    let sw_off    = raylib::color::Color { r: 60, g: 55, b: 50, a: 255 };
    let txt       = raylib::color::Color { r: 200, g: 200, b: 180, a: 255 };
    let txt_dim   = raylib::color::Color { r: 120, g: 120, b: 110, a: 255 };
    let t_fg      = raylib::color::Color { r: 0, g: 220, b: 80, a: 255 };
    let t_bg      = raylib::color::Color { r: 5, g: 12, b: 5, a: 255 };
    let btn_bg    = raylib::color::Color { r: 70, g: 70, b: 75, a: 255 };
    let border    = raylib::color::Color { r: 100, g: 100, b: 90, a: 255 };

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

            // Function buttons (matches draw at btn_y = 470)
            let btn_actions = [PanelSwitch::RunStop, PanelSwitch::SingleStep,
                               PanelSwitch::Examine, PanelSwitch::Deposit,
                               PanelSwitch::ExamineNext, PanelSwitch::DepositNext];
            let btn_y_f: f32 = 470.0;
            for (i, action) in btn_actions.iter().enumerate() {
                let x = (lp_x + 10 + i as i32 * (BTN_W + BTN_GAP)) as f32;
                if m.x >= x && m.x < x + BTN_W as f32 && m.y >= btn_y_f && m.y < btn_y_f + BTN_H as f32 {
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
        if rl.is_key_pressed(KeyboardKey::KEY_R) {
            emu = Imsai8080::new();
            emu.load_program(0x0000, &UART_TEST);
            emu.panel.set_address_switches(0x0000);
            emu.panel.press_switch(PanelSwitch::RunStop);
            emu.process_panel();
            addr_sw = [false; 16];
            data_sw = [false; 8];
            cycles = 0;
            term = [[0x20u8; TERM_COLS]; TERM_ROWS];
            tcx = 0;
            tcy = 0;
            running = true;
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

        // === Address switches (row at y=320) ===
        let addr_sw_y: i32 = 310;
        d.draw_text(&format!("ADDR {:04X}", addr_val), lp_x + 10, addr_sw_y - 16, 13, txt);
        for i in 0..16usize {
            let x = lp_x + 10 + i as i32 * SW_GAP;
            d.draw_rectangle(x, addr_sw_y, SW_W, SW_H, if addr_sw[i] { sw_on } else { sw_off });
            d.draw_text(if addr_sw[i] { "1" } else { "0" }, x + 6, addr_sw_y + 10, 11, txt);
        }

        // === Data switches (row at y=400) ===
        let data_sw_y: i32 = 385;
        d.draw_text(&format!("DATA {:02X}", data_val), lp_x + 10, data_sw_y - 16, 13, txt);
        for i in 0..8usize {
            let x = lp_x + 10 + i as i32 * SW_GAP;
            d.draw_rectangle(x, data_sw_y, SW_W, SW_H, if data_sw[i] { sw_on } else { sw_off });
            d.draw_text(if data_sw[i] { "1" } else { "0" }, x + 6, data_sw_y + 10, 11, txt);
        }

        // === Function buttons (row at y=470) ===
        let btn_y: i32 = 470;
        let btn_labels = ["RUN/STOP", "STEP", "EXAM", "DEPOSIT", "EX NXT", "DEP NXT"];
        d.draw_text("FUNCTION", lp_x + 10, btn_y - 16, 12, txt_dim);
        for (i, lbl) in btn_labels.iter().enumerate() {
            let x = lp_x + 10 + i as i32 * (BTN_W + BTN_GAP);
            d.draw_rectangle(x, btn_y, BTN_W, BTN_H, btn_bg);
            d.draw_rectangle_lines(x, btn_y, BTN_W, BTN_H, border);
            // Center text in button
            let text_w = d.measure_text(lbl, 10);
            d.draw_text(lbl, x + (BTN_W - text_w) / 2, btn_y + 9, 10, txt);
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
        d.draw_text("F5:Run/Stop  R:Reset  Click switches/buttons to interact", lp_x + 10, H - 20, 11, txt_dim);

        drop(d);
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