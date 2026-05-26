//! BIOS implementation for the IMSAI 8080 emulator
//!
//! This module provides a basic BIOS implementation that can handle
//! CP/M console I/O functions. It acts as the hardware abstraction
//! layer between the emulated system and the actual hardware.

use crate::io::IoController;

/// BIOS implementation for CP/M
pub struct Bios {
    /// Reference to the I/O controller
    io_controller: IoController,
}

impl Bios {
    /// Create a new BIOS instance
    pub fn new(io_controller: IoController) -> Self {
        Self { io_controller }
    }

    /// CONST - Check for console character ready
    /// Returns 0xFF if a character is ready to read, 0x00 otherwise
    pub fn const_func(&self) -> u8 {
        if self.io_controller.keyboard.is_char_ready() {
            0xFF
        } else {
            0x00
        }
    }

    /// CONIN - Read console character in
    /// Reads the next console character into register A
    /// Sets the parity bit (high-order bit) to zero
    pub fn conin_func(&mut self) -> u8 {
        let ch = self.io_controller.keyboard.read_char();
        // Clear the high-order bit (parity bit) to zero
        ch & 0x7F
    }

    /// CONOUT - Write console character out
    /// Sends the character from register C to the console output device
    pub fn conout_func(&mut self, ch: u8) {
        // Send character to video display
        self.io_controller.video.write_char(ch);
        // Render the updated display
        self.io_controller.video.render();
    }

    /// LIST - Write listing character out
    /// Sends character from register C to the listing device (printer)
    pub fn list_func(&mut self, _ch: u8) {
        // In a real implementation, this would send to a printer
        // For now, we'll just send to the console as well
        // self.io_controller.video.write_char(ch);
        // self.io_controller.video.render();
    }

    /// READER - Read reader device
    /// Reads next character from reader device into register A
    /// Returns ASCII CTRL-Z (1AH) for end-of-file
    pub fn reader_func(&mut self) -> u8 {
        // For now, return EOF
        0x1A // CTRL-Z
    }

    /// LISTST - Return list status
    /// Returns 0x00 if list device is not ready
    /// Returns 0xFF if character can be sent to printer
    pub fn listst_func(&self) -> u8 {
        // Assume printer is always ready for simplicity
        0xFF
    }

    /// Initialize the BIOS
    pub fn initialize(&mut self) {
        // Initialize the I/O controller
        self.io_controller.initialize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bios_basic() {
        let io_controller = IoController::new();
        let mut bios = Bios::new(io_controller);

        // Initially no characters should be ready
        assert_eq!(bios.const_func(), 0x00);

        // Reader should return EOF
        assert_eq!(bios.reader_func(), 0x1A);

        // List status should indicate ready
        assert_eq!(bios.listst_func(), 0xFF);
    }
}
