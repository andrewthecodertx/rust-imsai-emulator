//! Main emulator implementation

use crate::bios::Bios;
use crate::io::IoController;
use crate::memory::Memory;
use intel8080::Cpu8080;

/// The main IMSAI 8080 emulator system
pub struct Imsai8080 {
    /// The Intel 8080 CPU core
    pub cpu: Cpu8080,
    /// The memory subsystem
    pub memory: Memory,
    /// The I/O controller
    pub io: IoController,
    /// The BIOS implementation
    pub bios: Bios,
}

impl Imsai8080 {
    /// Create a new IMSAI 8080 emulator instance
    pub fn new() -> Self {
        let io = IoController::new();
        let bios = Bios::new(io);

        Self {
            cpu: Cpu8080::new(),
            memory: Memory::new(),
            io: IoController::new(),
            bios,
        }
    }

    /// Initialize the emulator
    pub fn initialize(&mut self) {
        self.bios.initialize();
    }

    /// Run the emulator
    pub fn run(&mut self) {
        self.initialize();
        println!("IMSIAI 8080 Emulator Started");
        println!("CPU Status: {:?}", self.cpu);

        // Simulate typing some text to demonstrate keyboard input
        self.io.keyboard.type_text("Hello, CP/M!\n");

        // Simulate writing some text to demonstrate video output
        self.bios.conout_func(b'H');
        self.bios.conout_func(b'e');
        self.bios.conout_func(b'l');
        self.bios.conout_func(b'l');
        self.bios.conout_func(b'o');
        self.bios.conout_func(b'\n');
    }
}
