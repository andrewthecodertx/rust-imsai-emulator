//! Memory subsystem for the IMSAI 8080 emulator

/// Represents the memory system of the IMSAI 8080
pub struct Memory {
    /// The actual memory storage (64KB maximum for 8080)
    ram: [u8; 65536],
}

impl Memory {
    /// Create a new memory instance
    pub fn new() -> Self {
        Self { ram: [0; 65536] }
    }

    /// Read a byte from memory
    pub fn read(&self, address: u16) -> u8 {
        self.ram[address as usize]
    }

    /// Write a byte to memory
    pub fn write(&mut self, address: u16, value: u8) {
        self.ram[address as usize] = value;
    }
}
