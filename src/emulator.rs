//! Main emulator implementation

use crate::bus::ImsaiBus;
use intel8080::Cpu8080;

/// The IMSAI 8080 emulator system
pub struct Imsai8080 {
    /// The Intel 8080 CPU
    pub cpu: Cpu8080,
    /// The system bus (memory + I/O)
    pub bus: ImsaiBus,
}

impl Default for Imsai8080 {
    fn default() -> Self {
        Self::new()
    }
}

impl Imsai8080 {
    /// Create a new IMSAI 8080 emulator instance
    pub fn new() -> Self {
        Self {
            cpu: Cpu8080::new(),
            bus: ImsaiBus::new(),
        }
    }

    /// Load a program binary into memory
    pub fn load_program(&mut self, start: u16, data: &[u8]) {
        self.bus.load(start, data);
    }

    /// Execute a single CPU instruction
    pub fn step(&mut self) -> u32 {
        self.cpu.step(&mut self.bus)
    }

    /// Run for a given number of instructions
    pub fn run_steps(&mut self, count: u32) {
        for _ in 0..count {
            self.step();
        }
    }
}