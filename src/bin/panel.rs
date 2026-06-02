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

const W: i32 = 960;
const H: i32 = 680;
const LS: usize = 18;
const LG: usize = 26;
const SWW: usize = 24;
const SWH: usize = 32;
const SWG: usize = 30;
const BW: usize = 100;
const BH: usize = 30;

/// UART test program: initializes 8251A and prints 'A' forever.
/// 15 bytes at address 0x0000.
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
            // Load disk image and boot CP/M
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
            // Load raw binary file
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
        // Set address switches to show the start address
        let start: u16 = 0x0000;
        for i in 0..16 {
            addr_sw[i] = (start >> (15 - i)) & 1 != 0;
        }
    }

    // Terminal buffer
    let mut term = [[0x20u8; 80]; 24];
    let mut tcx: usize = 0;
    let mut tcy: usize = 0;

    let mut running = false;
    let mut cycles: u64 = 0;
    let mut step_pending = false;

    let bg = raylib::color::Color { r: 30, g: 30, b: 35, a: 255 };
    let led_on = raylib::color::Color { r: 0, g: 255, b: 60, a: 255 };
    let led_off = raylib::color::Color { r: 20, g: 20, b: 20, a: 255 };
    let led_red = raylib::color::Color { r: 255, g: 40, b: 40, a: 255 };
    let sw_on = raylib::color::Color { r: 220, g: 200, b: 160, a: 255 };
    let sw_off = raylib::color::Color { r: 60, g: 55, b: 50, a: 255 };
    let txt = raylib::color::Color { r: 200, g: 200, b: 180, a: 255 };
    let t_fg = raylib::color::Color { r: 0, g: 220, b: 80, a: 255 };
    let t_bg = raylib::color::Color { r: 10, g: 20, b: 10, a: 255 };
    let btn_c = raylib::color::Color { r: 70, g: 70, b: 75, a: 255 };

    while !rl.window_should_close() {
        // ---- Input: toggle switches ----
        if rl.is_mouse_button_pressed(MouseButton::MOUSE_BUTTON_LEFT) {
            let m = rl.get_mouse_position();
            for i in 0..16usize {
                let x = 30 + i * LG;
                let y = 320;
                if m.x >= x as f32 && m.x < (x + SWW) as f32 && m.y >= y as f32 && m.y < (y + SWH) as f32 {
                    addr_sw[i] = !addr_sw[i];
                }
            }
            for i in 0..8usize {
                let x = 30 + i * LG;
                let y = 410;
                if m.x >= x as f32 && m.x < (x + SWW) as f32 && m.y >= y as f32 && m.y < (y + SWH) as f32 {
                    data_sw[i] = !data_sw[i];
                }
            }

            // Function buttons
            let btn_actions = [PanelSwitch::RunStop, PanelSwitch::SingleStep,
                               PanelSwitch::Examine, PanelSwitch::Deposit,
                               PanelSwitch::ExamineNext, PanelSwitch::DepositNext];
            for (i, action) in btn_actions.iter().enumerate() {
                let x = 30 + i * (BW + LG);
                let y = 490;
                if m.x >= x as f32 && m.x < (x + BW) as f32 && m.y >= y as f32 && m.y < (y + BH) as f32 {
                    if *action == PanelSwitch::SingleStep {
                        step_pending = true;
                    } else {
                        emu.panel.press_switch(*action);
                    }
                }
            }
        }

        // Keyboard shortcuts: F5=Run/Stop, F10=Quit, R=Reset to UART test
        if rl.is_key_pressed(KeyboardKey::KEY_F5) {
            emu.panel.press_switch(PanelSwitch::RunStop);
        }
        if rl.is_key_pressed(KeyboardKey::KEY_R) {
            // Reset: reload UART test program
            emu = Imsai8080::new();
            emu.load_program(0x0000, &UART_TEST);
            emu.panel.set_address_switches(0x0000);
            emu.panel.press_switch(PanelSwitch::RunStop);
            emu.process_panel();
            addr_sw = [false; 16];
            data_sw = [false; 8];
            cycles = 0;
            term = [[0x20u8; 80]; 24];
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

            // Collect UART output directly from the 8251A.
            // We bypass SerialCard's poll_rx/drain_output which route to the
            // internal VideoDisplay. Instead, we manage TX state and read
            // output ourselves for the raylib terminal buffer.
            emu.bus.serial().channel_a_mut().drain_tx();
            emu.bus.serial().channel_a_mut().update_tx();
            // Transfer any pending keyboard input to the UART RX register
            emu.bus.serial().poll_keyboard();
            // Collect all TX output bytes for the terminal display
            let output = emu.bus.serial().channel_a_mut().take_output();
            for &b in &output {
                match b {
                    0x0D | 0x0A => { tcx = 0; tcy += 1; if tcy >= 24 { tcy = 23; } }
                    0x08 => { if tcx > 0 { tcx -= 1; } }
                    0x20..=0x7E => {
                        if tcx < 80 && tcy < 24 {
                            term[tcy][tcx] = b;
                            tcx += 1;
                            if tcx >= 80 { tcx = 0; tcy += 1; if tcy >= 24 { tcy = 23; } }
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

        // Title and status
        d.draw_text("IMSAI 8080", 30, 10, 24, txt);
        let state_str = if emu.panel.is_running() { "RUNNING" } else { "STOPPED" };
        let state_col = if emu.panel.is_running() { led_on } else { led_red };
        d.draw_text(state_str, 250, 14, 16, state_col);
        d.draw_text(&format!("Cycles: {}", cycles), 400, 14, 14, txt);

        // Address LEDs (bit 15 = A15 on left, bit 0 = A0 on right)
        d.draw_text("ADDRESS", 30, 38, 14, txt);
        let leds = emu.panel.leds();
        for i in 0..16usize {
            let x = (30 + (15 - i) * LG) as i32;
            let on = leds.address[i];
            d.draw_rectangle(x, 58, LS as i32, LS as i32, if on { led_on } else { led_off });
        }

        // Data LEDs
        d.draw_text("DATA", 30, 98, 14, txt);
        for i in 0..8usize {
            let x = (30 + (7 - i) * LG) as i32;
            let on = leds.data[i];
            d.draw_rectangle(x, 118, LS as i32, LS as i32, if on { led_on } else { led_off });
        }

        // Status LEDs
        d.draw_text("STATUS", 30, 150, 14, txt);
        let status = [
            ("RUN", leds.run, led_on), ("M1", leds.m1, led_on),
            ("WAIT", leds.wait, led_red), ("MEMR", leds.memr, led_on),
            ("MWRT", leds.mwrt, led_red), ("IOR", leds.ior, led_on),
            ("IOW", leds.iow, led_on), ("PWR", leds.power, led_on),
        ];
        for (i, (lbl, on, oc)) in status.iter().enumerate() {
            let x = (30 + i * 56) as i32;
            d.draw_rectangle(x, 170, 10, 10, if *on { *oc } else { led_off });
            d.draw_text(lbl, x + 14, 170, 10, txt);
        }

        // Address switches
        d.draw_text(&format!("ADDR {:04X}", addr_val), 470, 298, 14, txt);
        for i in 0..16usize {
            let x = (30 + i * SWG) as i32;
            d.draw_rectangle(x, 320, SWW as i32, SWH as i32, if addr_sw[i] { sw_on } else { sw_off });
            d.draw_text(if addr_sw[i] { "1" } else { "0" }, x + 7, 328, 12, txt);
        }

        // Data switches
        d.draw_text(&format!("DATA {:02X}", data_val), 410, 388, 14, txt);
        for i in 0..8usize {
            let x = (30 + i * SWG) as i32;
            d.draw_rectangle(x, 410, SWW as i32, SWH as i32, if data_sw[i] { sw_on } else { sw_off });
            d.draw_text(if data_sw[i] { "1" } else { "0" }, x + 7, 418, 12, txt);
        }

        // Function buttons
        let btn_labels = ["RUN/STOP", "STEP", "EXAM", "DEPOSIT", "EX NXT", "DEP NXT"];
        for (i, lbl) in btn_labels.iter().enumerate() {
            let x = (30 + i * (BW + 10)) as i32;
            d.draw_rectangle(x, 490, BW as i32, BH as i32, btn_c);
            d.draw_rectangle_lines(x, 490, BW as i32, BH as i32, txt);
            d.draw_text(lbl, x + 5, 497, 10, txt);
        }

        // Terminal output
        d.draw_rectangle(460, 38, 480, 460, t_bg);
        d.draw_text("CONSOLE (Port 0x00)", 465, 40, 14, txt);
        for row in 0..24usize {
            for col in 0..80usize {
                let ch = term[row][col];
                if ch != 0x20 && ch != 0x00 {
                    d.draw_text(&format!("{}", ch as char), 465 + (col * 7) as i32, 58 + (row * 14) as i32, 10, t_fg);
                }
            }
        }

        // I/O log
        d.draw_text("I/O LOG", 460, 510, 14, txt);
        let io_log = emu.panel.io_log();
        let start = io_log.len().saturating_sub(10);
        for (i, ev) in io_log[start..].iter().enumerate() {
            let dir = if ev.is_write { "OUT" } else { "IN " };
            let kind = if ev.is_io { "IO" } else { "MEM" };
            d.draw_text(&format!("{} {:04X} {:02X} {}", dir, ev.address, ev.data, kind),
                        460, 528 + (i * 14) as i32, 10, txt);
        }

        // Help line
        d.draw_text("F5:RUN/STOP  R:Reset  ESC:Quit", 30, 660, 12, txt);

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