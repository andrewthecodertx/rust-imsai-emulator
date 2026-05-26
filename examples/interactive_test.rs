//! Interactive test example for the IMSAI 8080 emulator
//!
//! This example demonstrates interactive keyboard input and video output
//! capabilities of the emulator.

use rust_imsai_emulator::{
    bios::Bios,
    io::{IoController, Keyboard, VideoDisplay},
};

fn main() {
    println!("Interactive test for IMSAI 8080 emulator");
    println!("==========================================");

    // Create I/O components
    let mut keyboard = Keyboard::new();
    let mut video = VideoDisplay::new(80, 24);

    // Create a simple I/O controller
    let io_controller = IoController { keyboard, video };

    // Create BIOS
    let mut bios = Bios::new(io_controller);

    // Initialize
    bios.initialize();

    // Show welcome message
    println!("Welcome to the IMSAI 8080 emulator interactive test!");
    println!("Type some text and press Enter to see it displayed.");
    println!("Type 'quit' to exit.\n");

    // Simple interactive loop
    loop {
        // Simulate checking for keyboard input
        if bios.const_func() == 0xFF {
            // Character ready, read it
            let ch = bios.conin_func();

            // Echo to display
            bios.conout_func(ch);

            // Check for quit command (simplified)
            if ch == b'q' || ch == b'Q' {
                break;
            }
        }

        // In a real implementation, we'd handle actual keyboard input
        // For this demo, we'll just simulate some input periodically
        // In practice, you would integrate with actual stdin/stdout

        // Sleep briefly to avoid busy waiting
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("\nInteractive test completed.");
}
