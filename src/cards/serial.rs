use crate::chips::Uart8251;
use crate::io::Keyboard;
use crate::io::VideoDisplay;

/// IMSAI SIO-2 dual-channel serial card. Channel A = console, Channel B = auxiliary.
pub struct SerialCard {
    channel_a: Uart8251,
    channel_b: Uart8251,
    video: VideoDisplay,
    keyboard: Keyboard,
}

impl SerialCard {
    pub fn new() -> Self {
        Self {
            channel_a: Uart8251::new(),
            channel_b: Uart8251::new(),
            video: VideoDisplay::new(80, 24),
            keyboard: Keyboard::new(),
        }
    }

    pub fn type_text(&mut self, text: &str) {
        self.keyboard.type_text(text);
    }

    pub fn is_key_ready(&self) -> bool {
        self.keyboard.is_char_ready()
    }

    pub fn video(&self) -> &VideoDisplay {
        &self.video
    }

    pub fn video_mut(&mut self) -> &mut VideoDisplay {
        &mut self.video
    }

    pub fn set_auto_render(&mut self, enabled: bool) {
        self.video.auto_render = enabled;
    }

    pub fn channel_a_mut(&mut self) -> &mut Uart8251 {
        &mut self.channel_a
    }

    pub fn channel_a(&self) -> &Uart8251 {
        &self.channel_a
    }

    pub fn channel_b_mut(&mut self) -> &mut Uart8251 {
        &mut self.channel_b
    }

    /// Poll keyboard into Channel A RX without draining TX output.
    /// Use for custom rendering (raylib panel) where you manage TX yourself.
    pub fn poll_keyboard(&mut self) {
        if self.channel_a.is_rx_enabled() {
            if let Some(ch) = self.keyboard.read_char() {
                self.channel_a.inject_rx_byte(ch);
            }
        }
    }

    /// Service Channel A UART: inject keyboard input, drain TX shift register.
    /// Called from I/O read so TxRDY polling works during execution.
    /// Does NOT consume the output buffer (host calls `take_output()`).
    pub fn service_uart(&mut self) {
        if self.channel_a.is_rx_enabled() {
            if let Some(ch) = self.keyboard.read_char() {
                self.channel_a.inject_rx_byte(ch);
            }
        }
        self.channel_a.drain_tx();
        self.channel_a.update_tx();
    }

    /// Poll keyboard and drain TX output to the video display.
    /// Use for terminal-mode emulator. For raylib panel, use `poll_keyboard()`.
    pub fn poll_rx(&mut self) {
        if self.channel_a.is_rx_enabled() {
            if let Some(ch) = self.keyboard.read_char() {
                self.channel_a.inject_rx_byte(ch);
            }
        }
        let output = self.channel_a.take_output();
        for &byte in &output {
            self.video.write_char(byte);
            if self.video.auto_render {
                self.video.render();
            }
        }
        self.channel_a.drain_tx();
        self.channel_a.update_tx();
    }

    pub fn drain_output(&mut self) {
        let output = self.channel_a.take_output();
        for &byte in &output {
            self.video.write_char(byte);
            if self.video.auto_render {
                self.video.render();
            }
        }
    }
}

impl Default for SerialCard {
    fn default() -> Self {
        Self::new()
    }
}

impl SerialCard {
    pub fn io_read(&mut self, port: u8) -> u8 {
        if matches!(port, 0x00 | 0x01 | 0x79) {
            self.service_uart();
        }
        match port {
            0x00 => self.channel_a.read_data(),
            0x01 => self.channel_a.read_status(),
            0x79 => self.channel_a.read_status(),
            0x7B => self.channel_a.read_data(),
            0x02 => self.channel_b.read_data(),
            0x03 => self.channel_b.read_status(),
            _ => 0xFF,
        }
    }

    pub fn io_write(&mut self, port: u8, value: u8) {
        match port {
            0x00 => self.channel_a.write_data(value),
            0x01 => self.channel_a.write_control(value),
            0x7B => self.channel_a.write_data(value),
            0x02 => self.channel_b.write_data(value),
            0x03 => self.channel_b.write_control(value),
            _ => {}
        }
    }

    pub fn owns_port(&self, port: u8) -> bool {
        matches!(port, 0x00..=0x03 | 0x79 | 0x7B)
    }

    pub fn owns_address(&self, _addr: u16) -> bool {
        false
    }

    pub fn name(&self) -> &'static str {
        "SIO-2 Serial"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_card_port_decode() {
        let card = SerialCard::new();
        assert!(card.owns_port(0x00));
        assert!(card.owns_port(0x01));
        assert!(card.owns_port(0x02));
        assert!(card.owns_port(0x03));
        assert!(card.owns_port(0x79));
        assert!(card.owns_port(0x7B));
        assert!(!card.owns_port(0x04));
        assert!(!card.owns_port(0x48));
        assert!(!card.owns_port(0xFF));
    }

    #[test]
    fn test_serial_card_channel_a_initial_status() {
        let mut card = SerialCard::new();
        let status = card.io_read(0x01);
        assert_eq!(status & 0x01, 0x01);
        assert_eq!(status & 0x02, 0x00);
    }

    #[test]
    fn test_serial_card_channel_a_programming() {
        let mut card = SerialCard::new();
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);
        let status = card.io_read(0x01);
        assert_eq!(status & 0x01, 0x01);
    }

    #[test]
    fn test_serial_card_tx_output() {
        let mut card = SerialCard::new();
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);
        card.io_write(0x00, b'H');
        card.channel_a_mut().drain_tx();
        card.channel_a_mut().update_tx();
        let output = card.channel_a_mut().take_output();
        assert_eq!(output, vec![b'H']);
    }

    #[test]
    fn test_serial_card_rx_input() {
        let mut card = SerialCard::new();
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);
        card.type_text("A");
        card.poll_rx();
        let ch = card.io_read(0x00);
        assert_eq!(ch, b'A');
    }

    #[test]
    fn test_serial_card_port_79_alias() {
        let mut card = SerialCard::new();
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);
        let status_01 = card.io_read(0x01);
        let status_79 = card.io_read(0x79);
        assert_eq!(status_01, status_79);
    }

    #[test]
    fn test_serial_card_port_7b_output() {
        let mut card = SerialCard::new();
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);
        card.io_write(0x7B, b'X');
        card.channel_a_mut().drain_tx();
        card.channel_a_mut().update_tx();
        let output = card.channel_a_mut().take_output();
        assert_eq!(output, vec![b'X']);
    }

    #[test]
    fn test_serial_card_does_not_own_memory() {
        let card = SerialCard::new();
        assert!(!card.owns_address(0x0000));
        assert!(!card.owns_address(0xFFFF));
    }

    #[test]
    fn test_serial_card_name() {
        let card = SerialCard::new();
        assert_eq!(card.name(), "SIO-2 Serial");
    }

    #[test]
    fn test_serial_card_unowned_port_reads_ff() {
        let mut card = SerialCard::new();
        assert_eq!(card.io_read(0x04), 0xFF);
        assert_eq!(card.io_read(0x48), 0xFF);
    }
}

