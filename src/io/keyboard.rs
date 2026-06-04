
/// Keyboard controller for the IMSAI 8080
pub struct Keyboard {
    buffer: Vec<u8>,
    position: usize,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            position: 0,
        }
    }

    pub fn is_char_ready(&self) -> bool {
        !self.buffer.is_empty() && self.position < self.buffer.len()
    }

    pub fn read_char(&mut self) -> Option<u8> {
        if self.position < self.buffer.len() {
            let ch = self.buffer[self.position];
            self.position += 1;
            
            if self.position >= self.buffer.len() {
                self.buffer.clear();
                self.position = 0;
            }
            Some(ch)
        } else {
            None
        }
    }

    pub fn type_text(&mut self, text: &str) {
        for byte in text.bytes() {
            self.buffer.push(byte);
        }
    }

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

        assert_eq!(keyboard.read_char(), Some(b'H'));
        assert_eq!(keyboard.read_char(), Some(b'e'));
    }

    #[test]
    fn test_keyboard_returns_none_when_empty() {
        let mut keyboard = Keyboard::new();
        assert_eq!(keyboard.read_char(), None);
    }

    #[test]
    fn test_keyboard_nul_byte_roundtrip() {
        let mut keyboard = Keyboard::new();
        keyboard.type_text("\x00A");
        assert_eq!(keyboard.read_char(), Some(0));
        assert_eq!(keyboard.read_char(), Some(b'A'));
        assert_eq!(keyboard.read_char(), None);
    }
}