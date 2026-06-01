//! IMSAI SIO-2 serial interface card (2x Intel 8251A UART)
//!
//! The IMSAI SIO-2 is a dual-channel serial I/O board for the S-100 bus.
//! It contains two Intel 8251A programmable communication interface chips:
//!
//! - Channel A (console): ports 0x00 (data) and 0x01 (command/status)
//! - Channel B (auxiliary/listing): ports 0x02 (data) and 0x03 (command/status)
//!
//! The SIO-2 also decodes two additional port pairs that some configurations use:
//! - Port 0x79: Channel A status (alias for 0x01, used by some BIOS versions)
//! - Port 0x7B: Channel A data output (alias for 0x00, used by some BIOS versions)
//!
//! In the real hardware, each 8251A connects to a serial port. The host
//! terminal emulation connects to Channel A for console I/O.

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

    /// Poll the Channel A UART for keyboard input.
    /// Transfers characters from the keyboard buffer to the UART's
    /// RX data register. Must be called periodically to simulate
    /// serial data arrival from the terminal.
    pub fn poll_rx(&mut self) {
        // Transfer keyboard chars to the 8251A UART
        if self.channel_a.is_rx_enabled() && self.keyboard.is_char_ready() {
            let ch = self.keyboard.read_char();
            // Directly inject the character into the UART RX buffer
            // This bypasses the normal MODE/COMMAND programming sequence,
            // simulating immediate serial data arrival on the line.
            // The UART's poll_rx would normally do this, but our UART
            // has its own keyboard. Instead, we write directly to the
            // data register path.
            self.channel_a_mut().write_control(0x4E); // Mode: 8N1 X16
            self.channel_a_mut().write_control(0x05); // Command: TX+RX enable
            // Inject the byte directly
            if ch != 0 {
                self.channel_a.inject_rx_byte(ch);
            }
        }
        // Also drain any pending TX output to the video display
        let output = self.channel_a.take_output();
        for &byte in &output {
            self.video.write_char(byte);
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

    /// Get the current track register from the FD1771 (for diagnostics).
    /// This delegates to the Tarbell card, not the serial card,
    /// but is kept here for backward compatibility.
    pub fn current_track(&self) -> u8 {
        0 // Serial card has no track register
    }

    pub fn current_sector(&self) -> u8 {
        0
    }
}

impl Default for SerialCard {
    fn default() -> Self { Self::new() }
}

impl super::Card for SerialCard {
    fn io_read(&mut self, port: u8) -> u8 {
        // Poll keyboard input before any read from the console ports
        if matches!(port, 0x00 | 0x01 | 0x79) {
            self.poll_rx();
        }

        match port {
            // Channel A data (console input/output)
            0x00 => self.channel_a.read_data(),
            // Channel A command/status
            0x01 => self.channel_a.read_status(),
            // Port 0x79: alias for Channel A status
            0x79 => self.channel_a.read_status(),
            // Port 0x7B: alias for Channel A data (read)
            0x7B => self.channel_a.read_data(),
            // Channel B data
            0x02 => self.channel_b.read_data(),
            // Channel B command/status
            0x03 => self.channel_b.read_status(),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u8, value: u8) {
        match port {
            // Channel A data (console output)
            0x00 => {
                self.channel_a.write_data(value);
            }
            // Channel A command
            0x01 => {
                self.channel_a.write_control(value);
            }
            // Port 0x7B: alias for Channel A data output
            0x7B => {
                self.channel_a.write_data(value);
            }
            // Channel B data
            0x02 => {
                self.channel_b.write_data(value);
            }
            // Channel B command
            0x03 => {
                self.channel_b.write_control(value);
            }
            _ => {}
        }
    }

    fn owns_port(&self, port: u8) -> bool {
        matches!(port, 0x00 | 0x01 | 0x02 | 0x03 | 0x79 | 0x7B)
    }

    fn mem_read(&self, _addr: u16) -> Option<u8> { None }
    fn mem_write(&mut self, _addr: u16, _value: u8) -> bool { false }
    fn owns_address(&self, _addr: u16) -> bool { false }

    fn name(&self) -> &'static str { "SIO-2 Serial" }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Card;

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