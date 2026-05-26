use rust_imsai_emulator::{Bios, Imsai8080};

fn main() {
    let mut emu = Imsai8080::new();

    // Install the CP/M BIOS jump table into memory
    Bios::install_jump_table(&mut emu.bus);

    // Pre-load a simple test program:
    // MVI A, 'H' ; OUT 0x00 ; ... repeat for "Hello"
    // HALT
    let program: Vec<u8> = vec![
        0x3E, b'H',       // MVI A,'H'
        0xD3, 0x00,        // OUT 0x00
        0x3E, b'e',       // MVI A,'e'
        0xD3, 0x00,        // OUT 0x00
        0x3E, b'l',       // MVI A,'l'
        0xD3, 0x00,        // OUT 0x00
        0x3E, b'l',       // MVI A,'l'
        0xD3, 0x00,        // OUT 0x00
        0x3E, b'o',       // MVI A,'o'
        0xD3, 0x00,        // OUT 0x00
        0x76,               // HALT
    ];

    emu.load_program(0x0200, &program);
    emu.cpu.pc = 0x0200;

    println!("IMSAI 8080 Emulator Started");
    println!("CPU: {:?}", emu.cpu);

    // Run until halted
    loop {
        emu.step();
        if emu.cpu.halted {
            break;
        }
    }

    println!("\nEmulation complete. CPU halted.");
    println!("Final CPU state: {:?}", emu.cpu);
}