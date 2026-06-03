//! IMSAI SIO-2 serial interface card (2x Intel 8251A UART).
//!
//! See `docs/INTERNALS.md` for TX/RX data flow, UART state machine,
//! and port mapping.

use crate::chips::Uart8251;
use crate::io::Keyboard;
use crate::io::VideoDisplay;

/// IMSAI SIO-2 dual-channel serial interface card.
///
/// Combines two 8251A UARTs with board-level address decoding.
/// Channel A is connected to the host keyboard and video display
/// for console I/O. Channel B is available for auxiliary serial I/O.
pub struct SerialCard {
    /// Channel A UART (console)
    channel_a: Uart8251,
    /// Channel B UART (auxiliary/listing)
    channel_b: Uart8251,
    /// Video display for console output
    video: VideoDisplay,
    /// Direct keyboard access for the host terminal
    keyboard: Keyboard,
}

impl SerialCard {
    /// Create a new SIO-2 card with default UART configuration.
    pub fn new() -> Self {
        Self {
            channel_a: Uart8251::new(),
            channel_b: Uart8251::new(),
            video: VideoDisplay::new(80, 24),
            keyboard: Keyboard::new(),
        }
    }

    /// Type text into the console keyboard buffer.
    /// Characters are queued and will be read by the 8251A UART's
    /// CONIN (port 0x00) on the next poll.
    pub fn type_text(&mut self, text: &str) {
        self.keyboard.type_text(text);
    }

    /// Check if the console keyboard has a character ready.
    pub fn is_key_ready(&self) -> bool {
        self.keyboard.is_char_ready()
    }

    /// Get a reference to the video display.
    pub fn video(&self) -> &VideoDisplay {
        &self.video
    }

    /// Get a mutable reference to the video display.
    pub fn video_mut(&mut self) -> &mut VideoDisplay {
        &mut self.video
    }

    /// Set whether the video display auto-renders on every character.
    pub fn set_auto_render(&mut self, enabled: bool) {
        self.video.auto_render = enabled;
    }

    /// Get a mutable reference to Channel A UART.
    pub fn channel_a_mut(&mut self) -> &mut Uart8251 {
        &mut self.channel_a
    }

    /// Get a reference to Channel A UART.
    pub fn channel_a(&self) -> &Uart8251 {
        &self.channel_a
    }

    /// Get a mutable reference to Channel B UART.
    pub fn channel_b_mut(&mut self) -> &mut Uart8251 {
        &mut self.channel_b
    }

    /// Poll the Channel A UART keyboard buffer and transfer one character
    /// to the UART RX data register if ready.
    ///
    /// Unlike `poll_rx()`, this method does NOT drain TX output or
    /// consume the output buffer. Use this for custom rendering pipelines
    /// (e.g., raylib panel) where you manage TX output yourself via
    /// `channel_a_mut().take_output()`.
    pub fn poll_keyboard(&mut self) {
        if self.channel_a.is_rx_enabled() {
            if let Some(ch) = self.keyboard.read_char() {
                self.channel_a.inject_rx_byte(ch);
            }
        }
    }

    /// Service the Channel A UART during CPU I/O operations.
    ///
    /// This method:
    /// 1. Injects keyboard input into the RX buffer (without reprogramming)
    /// 2. Drains the TX shift register and restores TxRDY so the CPU can
    ///    poll for TX readiness without hanging
    /// 3. Does NOT consume the output buffer (the host calls `take_output()`)
    ///
    /// Called from the I/O read path so TxRDY polling works during execution.
    pub fn service_uart(&mut self) {
        // Inject keyboard input without reprogramming the UART
        if self.channel_a.is_rx_enabled() {
            if let Some(ch) = self.keyboard.read_char() {
                self.channel_a.inject_rx_byte(ch);
            }
        }
        // Drain TX shift register and restore TxRDY/TxEMPTY flags.
        // This simulates the byte being transmitted on the serial line,
        // which clears the TX buffer in the real chip and makes TxRDY
        // available for the next OUT instruction.
        self.channel_a.drain_tx();
        self.channel_a.update_tx();
    }

    /// Poll the Channel A UART for keyboard input and drain TX output
    /// to the video display.
    ///
    /// Use this for the terminal-mode emulator (main.rs) that uses the
    /// VideoDisplay. For the panel (raylib), use `poll_keyboard()` and
    /// `take_output()` instead.
    pub fn poll_rx(&mut self) {
        // Transfer keyboard chars to the 8251A UART RX buffer
        if self.channel_a.is_rx_enabled() {
            if let Some(ch) = self.keyboard.read_char() {
                self.channel_a.inject_rx_byte(ch);
            }
        }
        // Drain any pending TX output to the video display
        let output = self.channel_a.take_output();
        for &byte in &output {
            self.video.write_char(byte);
            if self.video.auto_render {
                self.video.render();
            }
        }
        // Update TX state
        self.channel_a.drain_tx();
        self.channel_a.update_tx();
    }

    /// Drain all console output from Channel A UART.
    /// Writes characters to the video display buffer.
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
    fn default() -> Self { Self::new() }
}

// Inherent I/O methods for direct dispatch (no trait object needed).
impl SerialCard {
    /// Read from an I/O port this card owns.
    pub fn io_read(&mut self, port: u8) -> u8 {
        // Service the UART on every read from console ports.
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

    /// Write to an I/O port this card owns.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_card_port_decode() {
        let card = SerialCard::new();
        // Channel A ports
        assert!(card.owns_port(0x00));
        assert!(card.owns_port(0x01));
        // Channel B ports
        assert!(card.owns_port(0x02));
        assert!(card.owns_port(0x03));
        // Aliases
        assert!(card.owns_port(0x79));
        assert!(card.owns_port(0x7B));
        // Other ports should not be owned
        assert!(!card.owns_port(0x04));
        assert!(!card.owns_port(0x48));
        assert!(!card.owns_port(0xFF));
    }

    #[test]
    fn test_serial_card_channel_a_initial_status() {
        let mut card = SerialCard::new();
        // After reset, UART should be in ExpectMode state
        // and TxReady should be set
        let status = card.io_read(0x01);
        assert_eq!(status & 0x01, 0x01); // TxRDY set
        assert_eq!(status & 0x02, 0x00); // RxRDY clear
    }

    #[test]
    fn test_serial_card_channel_a_programming() {
        let mut card = SerialCard::new();

        // Program Channel A: mode instruction (8N1, X16)
        card.io_write(0x01, 0x4E);
        // Command instruction: TX+RX enable
        card.io_write(0x01, 0x05);

        let status = card.io_read(0x01);
        assert_eq!(status & 0x01, 0x01); // TxRDY should be set
    }

    #[test]
    fn test_serial_card_tx_output() {
        let mut card = SerialCard::new();

        // Program Channel A
        card.io_write(0x01, 0x4E); // Mode
        card.io_write(0x01, 0x05); // Command: TX+RX enable

        // Write a character
        card.io_write(0x00, b'H');
        card.channel_a_mut().drain_tx();
        card.channel_a_mut().update_tx();

        let output = card.channel_a_mut().take_output();
        assert_eq!(output, vec![b'H']);
    }

    #[test]
    fn test_serial_card_rx_input() {
        let mut card = SerialCard::new();

        // Program Channel A
        card.io_write(0x01, 0x4E); // Mode
        card.io_write(0x01, 0x05); // Command: TX+RX enable

        // Type a character via the keyboard
        card.type_text("A");
        card.poll_rx();

        // Read data from Channel A
        let ch = card.io_read(0x00);
        assert_eq!(ch, b'A');
    }

    #[test]
    fn test_serial_card_port_79_alias() {
        let mut card = SerialCard::new();

        // Program Channel A
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);

        // Port 0x79 should read the same status as port 0x01
        let status_01 = card.io_read(0x01);
        let status_79 = card.io_read(0x79);
        assert_eq!(status_01, status_79);
    }

    #[test]
    fn test_serial_card_port_7b_output() {
        let mut card = SerialCard::new();

        // Program Channel A
        card.io_write(0x01, 0x4E);
        card.io_write(0x01, 0x05);

        // Write via port 0x7B (alias)
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