//! Keyboard input interface for the IMSAI 8080 emulator
//!
//! This module provides a simulated keyboard interface that can be used
//! to provide input to the emulated system. It implements a simple
//! serial interface similar to what would be found in real S-100 systems.

/// Keyboard controller for the IMSAI 8080
pub struct Keyboard {
    /// Buffer for incoming keystrokes
    buffer: Vec<u8>,
    /// Current position in the buffer
    position: usize,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    /// Create a new keyboard controller
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            position: 0,
        }
    }

    /// Check if a character is ready to be read
    /// Returns true if a character is available, false otherwise
    pub fn is_char_ready(&self) -> bool {
        !self.buffer.is_empty() && self.position < self.buffer.len()
    }

    /// Read a character from the keyboard
    /// Returns 0 if no character is available (non-blocking).
    /// Callers should check is_char_ready() first to avoid reading 0.
    pub fn read_char(&mut self) -> u8 {
        if self.position < self.buffer.len() {
            let ch = self.buffer[self.position];
            self.position += 1;
            // Compact: if we've consumed all buffered chars, reset
            if self.position >= self.buffer.len() {
                self.buffer.clear();
                self.position = 0;
            }
            ch
        } else {
            0
        }
    }

    /// Simulate typing text into the keyboard buffer
    pub fn type_text(&mut self, text: &str) {
        for byte in text.bytes() {
            self.buffer.push(byte);
        }
    }

    /// Clear the keyboard buffer
    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
        self.position = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_basic() {
        let mut keyboard = Keyboard::new();
        assert!(!keyboard.is_char_ready());

        keyboard.type_text("Hello");
        assert!(keyboard.is_char_ready());

        assert_eq!(keyboard.read_char(), b'H');
        assert_eq!(keyboard.read_char(), b'e');
    }
}
