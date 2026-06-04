
use intel8080::Bus;
use crate::cards::{MemoryCard, SerialCard, TarbellCard};

/// S-100 backplane: MemoryCard + SerialCard + TarbellCard, no dynamic dispatch.
pub struct ImsaiBus {
    pub memory: MemoryCard,
    serial: SerialCard,
    tarbell: TarbellCard,
}

impl ImsaiBus {
    pub fn new() -> Self {
        Self {
            memory: MemoryCard::new(),
            serial: SerialCard::new(),
            tarbell: TarbellCard::new(),
        }
    }

    pub fn load(&mut self, start: u16, data: &[u8]) {
        for (i, &byte) in data.iter().enumerate() {
            self.memory.ram[start.wrapping_add(i as u16) as usize] = byte;
        }
    }

    pub fn mem_read(&self, addr: u16) -> u8 {
        self.memory.ram[addr as usize]
    }

    pub fn mem_write(&mut self, addr: u16, value: u8) {
        self.memory.ram[addr as usize] = value;
    }

    pub fn serial(&mut self) -> &mut SerialCard {
        &mut self.serial
    }

    pub fn console(&mut self) -> &mut SerialCard {
        &mut self.serial
    }

    pub fn tarbell(&mut self) -> &mut TarbellCard {
        &mut self.tarbell
    }

    pub fn tarbell_ref(&self) -> &TarbellCard {
        &self.tarbell
    }

    pub fn memory_mut(&mut self) -> &mut MemoryCard {
        &mut self.memory
    }

    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        self.tarbell.insert_disk(drive, path)
    }
}

impl Default for ImsaiBus {
    fn default() -> Self { Self::new() }
}

/// I/O port dispatch: 0x00-0x03+0x79/0x7B=Serial, 0x48-0x4B+0xF8-0xFF=Tarbell, else 0xFF.
impl Bus for ImsaiBus {
    fn mem_read(&self, addr: u16) -> u8 {
        self.memory.ram[addr as usize]
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        self.memory.ram[addr as usize] = value;
    }

    fn io_in(&mut self, port: u8) -> u8 {
        match port {
            0x00 | 0x7B => self.serial.io_read(port),
            0x01 | 0x79 => self.serial.io_read(port),
            0x02 => self.serial.io_read(port),
            0x03 => self.serial.io_read(port),
            0x48..=0x4B | 0xF8..=0xFF => self.tarbell.io_read(port),
            _ => 0xFF,
        }
    }

    fn io_out(&mut self, port: u8, value: u8) {
        match port {
            0x00 | 0x7B => self.serial.io_write(port, value),
            0x01 | 0x79 => self.serial.io_write(port, value),
            0x02 => self.serial.io_write(port, value),
            0x03 => self.serial.io_write(port, value),
            0x48..=0x4B | 0xF8..=0xFF => self.tarbell.io_write(port, value),
            _ => {}
        }
    }
}