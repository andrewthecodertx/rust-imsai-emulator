//! Interactive test example for the IMSAI 8080 emulator
//!
//! Loads a simple echo program that reads from port 0 and writes to port 0,
//! then feeds it simulated keyboard input.

use rust_imsai_emulator::{Bios, Imsai8080};

fn main() {
    let mut emu = Imsai8080::new();

    // Install BIOS jump table
    Bios::install_jump_table(&mut emu.bus);

    // Echo program:
    // LOOP: IN 0x01      ; check keyboard status
    //       ANI 0x01     ; test bit 0
    //       JZ LOOP      ; spin if no key ready
    //       IN 0x00      ; read character
    //       OUT 0x00     ; echo it back
    //       JMP LOOP     ; repeat
    let program: Vec<u8> = vec![
        0xDB, 0x01,       // IN 0x01
        0xE6, 0x01,       // ANI 0x01
        0xCA, 0x00, 0x02, // JZ 0x0200
        0xDB, 0x00,       // IN 0x00
        0xD3, 0x00,       // OUT 0x00
        0xC3, 0x00, 0x02, // JMP 0x0200
    ];

    emu.load_program(0x0200, &program);
    emu.cpu.pc = 0x0200;

    // Simulate typing some text
    emu.bus.io.keyboard.type_text("Hi!\n");

    println!("IMSAI 8080 Interactive Test");
    println!("Running echo program with simulated input: \"Hi!\\n\"");

    // Run a limited number of steps (the echo program loops forever)
    for _ in 0..200 {
        emu.step();
    }

    println!("\nTest complete (ran 200 steps).");
    println!("Display contents:\n{}", emu.bus.io.video.get_display_string());
}