use rust_imsai_emulator::{io::IoController, Bios};

fn main() {
    // Create a simple test of our components
    let io = IoController::new();
    let mut bios = Bios::new(io);

    println!("Testing BIOS and I/O components:");

    // Initialize BIOS
    bios.initialize();

    // Test keyboard functionality
    println!("Checking keyboard status...");
    let status = bios.const_func();
    println!("Keyboard status: 0x{:02X}", status);

    // Test video output
    println!("Writing 'Hello' to display...");
    bios.conout_func(b'H');
    bios.conout_func(b'e');
    bios.conout_func(b'l');
    bios.conout_func(b'l');
    bios.conout_func(b'o');
    bios.conout_func(b'\n');

    println!("Tests completed.");
}
