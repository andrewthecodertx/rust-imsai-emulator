use crate::io::Keyboard;

const CMD_TX_ENABLE: u8 = 0x01;
const CMD_DTR: u8 = 0x02;
const CMD_RX_ENABLE: u8 = 0x04;
const CMD_SEND_BREAK: u8 = 0x08;
const CMD_RESET: u8 = 0x40;
#[allow(dead_code)]
const CMD_HUNT: u8 = 0x80;

const STATUS_TX_READY: u8 = 0x01;
const STATUS_RX_READY: u8 = 0x02;
const STATUS_TX_EMPTY: u8 = 0x04;
const STATUS_PARITY_ERR: u8 = 0x08;
const STATUS_OVERRUN_ERR: u8 = 0x10;
const STATUS_FRAMING_ERR: u8 = 0x20;
#[allow(dead_code)]
const STATUS_SYNC_DET: u8 = 0x40;
#[allow(dead_code)]
const STATUS_DSR: u8 = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UartState {
    ExpectMode,
    ExpectCommand,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaudDivisor {
    Sync = 1,
    X16 = 16,
    X64 = 64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharLength {
    Bits5 = 5,
    Bits6 = 6,
    Bits7 = 7,
    Bits8 = 8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    None,
    Even,
    Odd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBits {
    Invalid,
    One,
    OneAndHalf,
    Two,
}

/// Intel 8251A UART chip. One channel; the IMSAI SIO-2 has two (A=console, B=aux).
pub struct Uart8251 {
    state: UartState,
    baud_divisor: BaudDivisor,
    char_length: CharLength,
    parity: Parity,
    stop_bits: StopBits,
    tx_enabled: bool,
    rx_enabled: bool,
    dtr: bool,
    rts: bool,
    send_break: bool,
    status: u8,
    tx_data: Option<u8>,
    tx_draining: bool,
    rx_data: Option<u8>,
    keyboard: Option<Keyboard>,
    output_buffer: Vec<u8>,
}

impl Default for Uart8251 {
    fn default() -> Self {
        Self::new()
    }
}

impl Uart8251 {
    pub fn new() -> Self {
        Self {
            state: UartState::ExpectMode,
            baud_divisor: BaudDivisor::X16,
            char_length: CharLength::Bits8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            tx_enabled: false,
            rx_enabled: false,
            dtr: false,
            rts: false,
            send_break: false,
            status: STATUS_TX_EMPTY | STATUS_TX_READY,
            tx_data: None,
            tx_draining: false,
            rx_data: None,
            keyboard: None,
            output_buffer: Vec::new(),
        }
    }

    pub fn with_keyboard(mut self, keyboard: Keyboard) -> Self {
        self.keyboard = Some(keyboard);
        self
    }

    pub fn reset(&mut self) {
        self.state = UartState::ExpectMode;
        self.tx_enabled = false;
        self.rx_enabled = false;
        self.dtr = false;
        self.rts = false;
        self.send_break = false;
        self.status = STATUS_TX_EMPTY | STATUS_TX_READY;
        self.tx_data = None;
        self.tx_draining = false;
        self.rx_data = None;
        self.baud_divisor = BaudDivisor::X16;
        self.char_length = CharLength::Bits8;
        self.parity = Parity::None;
        self.stop_bits = StopBits::One;
    }

    pub fn read_data(&mut self) -> u8 {
        if let Some(data) = self.rx_data.take() {
            self.status &= !STATUS_RX_READY;
            self.status &= !STATUS_OVERRUN_ERR;
            data
        } else {
            0
        }
    }

    pub fn read_status(&self) -> u8 {
        self.status
    }

    pub fn write_data(&mut self, value: u8) {
        if !self.tx_enabled {
            return;
        }
        self.tx_data = Some(value);
        self.status &= !STATUS_TX_READY;
        self.status &= !STATUS_TX_EMPTY;
        self.tx_draining = false;
        self.output_buffer.push(value);
    }

    /// Control port write: mode instruction, then command instruction, then commands.
    pub fn write_control(&mut self, value: u8) {
        match self.state {
            UartState::ExpectMode => self.write_mode(value),
            UartState::ExpectCommand => {
                self.write_command(value);
                self.state = UartState::Ready;
            }
            UartState::Ready => self.write_command(value),
        }
    }

    // Mode instruction: bits 1:0=baud, 3:2=charlen, 4=parity en, 5=parity type, 7:6=stop
    fn write_mode(&mut self, value: u8) {
        self.baud_divisor = match value & 0x03 {
            0 | 1 => BaudDivisor::Sync,
            2 => BaudDivisor::X16,
            3 => BaudDivisor::X64,
            _ => unreachable!(),
        };
        self.char_length = match (value >> 2) & 0x03 {
            0 => CharLength::Bits5,
            1 => CharLength::Bits6,
            2 => CharLength::Bits7,
            3 => CharLength::Bits8,
            _ => unreachable!(),
        };
        self.parity = if value & 0x10 != 0 {
            if value & 0x20 != 0 {
                Parity::Even
            } else {
                Parity::Odd
            }
        } else {
            Parity::None
        };
        self.stop_bits = match (value >> 6) & 0x03 {
            0 => StopBits::Invalid,
            1 => StopBits::One,
            2 => StopBits::OneAndHalf,
            3 => StopBits::Two,
            _ => unreachable!(),
        };
        if self.baud_divisor != BaudDivisor::Sync {
            self.state = UartState::ExpectCommand;
        }
    }

    // Command instruction: bit0=TXen, 1=DTR, 2=RXen, 3=break, 4=err reset, 5=RTS, 6=reset, 7=hunt
    fn write_command(&mut self, value: u8) {
        if value & CMD_RESET != 0 {
            self.state = UartState::ExpectMode;
            self.tx_enabled = false;
            self.rx_enabled = false;
            self.status = STATUS_TX_EMPTY | STATUS_TX_READY;
            self.tx_data = None;
            self.rx_data = None;
            return;
        }
        self.tx_enabled = value & CMD_TX_ENABLE != 0;
        self.rx_enabled = value & CMD_RX_ENABLE != 0;
        self.dtr = value & CMD_DTR != 0;
        self.send_break = value & CMD_SEND_BREAK != 0;
        self.rts = value & 0x20 != 0;
        if value & 0x10 != 0 {
            self.status &= !(STATUS_PARITY_ERR | STATUS_OVERRUN_ERR | STATUS_FRAMING_ERR);
        }
        if self.tx_enabled && self.tx_data.is_none() {
            self.status |= STATUS_TX_READY;
        } else if !self.tx_enabled {
            self.status &= !STATUS_TX_READY;
        }
        if self.tx_data.is_none() {
            self.status |= STATUS_TX_EMPTY;
        }
    }

    pub fn poll_rx(&mut self) {
        if !self.rx_enabled {
            return;
        }
        if self.rx_data.is_none() {
            if let Some(ref mut kbd) = self.keyboard {
                if let Some(ch) = kbd.read_char() {
                    self.rx_data = Some(ch);
                    self.status |= STATUS_RX_READY;
                }
            }
        } else if self.keyboard.as_ref().is_some_and(|k| k.is_char_ready()) {
            self.status |= STATUS_OVERRUN_ERR;
        }
    }

    pub fn drain_tx(&mut self) {
        if self.tx_data.is_some() && !self.tx_draining {
            self.tx_draining = true;
        }
    }

    pub fn update_tx(&mut self) {
        if self.tx_data.is_some() && self.tx_draining {
            self.tx_data = None;
            self.tx_draining = false;
            self.status |= STATUS_TX_EMPTY;
        }
        if self.tx_enabled && self.tx_data.is_none() {
            self.status |= STATUS_TX_READY;
        }
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output_buffer)
    }

    pub fn has_output(&self) -> bool {
        !self.output_buffer.is_empty()
    }

    pub fn is_rx_ready(&self) -> bool {
        self.status & STATUS_RX_READY != 0
    }

    pub fn is_tx_ready(&self) -> bool {
        self.status & STATUS_TX_READY != 0
    }

    pub fn state(&self) -> UartState {
        self.state
    }

    pub fn is_tx_enabled(&self) -> bool {
        self.tx_enabled
    }

    pub fn is_rx_enabled(&self) -> bool {
        self.rx_enabled
    }

    /// Bypass keyboard and inject a byte directly into RX.
    pub fn inject_rx_byte(&mut self, value: u8) {
        self.rx_data = Some(value);
        self.status |= STATUS_RX_READY;
    }

    pub fn baud_divisor(&self) -> BaudDivisor {
        self.baud_divisor
    }

    pub fn char_length(&self) -> CharLength {
        self.char_length
    }

    pub fn parity(&self) -> Parity {
        self.parity
    }

    pub fn stop_bits(&self) -> StopBits {
        self.stop_bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let uart = Uart8251::new();
        assert_eq!(uart.state(), UartState::ExpectMode);
        assert!(uart.is_tx_ready());
        assert!(!uart.is_rx_ready());
    }

    #[test]
    fn test_reset_returns_to_expect_mode() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x37);
        assert_eq!(uart.state(), UartState::Ready);
        uart.write_data(b'A');
        assert!(!uart.is_tx_ready());
        uart.write_control(0x40);
        assert_eq!(uart.state(), UartState::ExpectMode);
        assert!(uart.is_tx_ready());
    }

    #[test]
    fn test_mode_instruction_async_8n1() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        assert_eq!(uart.state(), UartState::ExpectCommand);
        assert_eq!(uart.baud_divisor(), BaudDivisor::X16);
        assert_eq!(uart.char_length(), CharLength::Bits8);
        assert_eq!(uart.parity(), Parity::None);
        assert_eq!(uart.stop_bits(), StopBits::One);
    }

    #[test]
    fn test_mode_instruction_7e2() {
        let mut uart = Uart8251::new();
        uart.write_control(0xFA);
        assert_eq!(uart.baud_divisor(), BaudDivisor::X16);
        assert_eq!(uart.char_length(), CharLength::Bits7);
        assert_eq!(uart.parity(), Parity::Even);
        assert_eq!(uart.stop_bits(), StopBits::Two);
    }

    #[test]
    fn test_command_instruction_tx_rx_enable() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        assert!(!uart.is_tx_enabled());
        assert!(!uart.is_rx_enabled());
        uart.write_control(0x05);
        assert!(uart.is_tx_enabled());
        assert!(uart.is_rx_enabled());
        assert!(uart.is_tx_ready());
    }

    #[test]
    fn test_tx_flow() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x05);
        assert!(uart.is_tx_ready());
        uart.write_data(b'H');
        assert!(!uart.is_tx_ready());
        assert!(!uart.read_status() & STATUS_TX_EMPTY != 0);
        uart.drain_tx();
        uart.update_tx();
        assert!(uart.is_tx_ready());
        assert!(uart.read_status() & STATUS_TX_EMPTY != 0);
        let output = uart.take_output();
        assert_eq!(output, vec![b'H']);
    }

    #[test]
    fn test_tx_disabled_drops_data() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x04);
        uart.write_data(b'X');
        let output = uart.take_output();
        assert!(output.is_empty());
    }

    #[test]
    fn test_rx_flow_with_keyboard() {
        let mut kbd = Keyboard::new();
        kbd.type_text("AB");
        let mut uart = Uart8251::new().with_keyboard(kbd);
        uart.write_control(0x4E);
        uart.write_control(0x05);
        uart.poll_rx();
        assert!(uart.is_rx_ready());
        assert_eq!(uart.read_data(), b'A');
        assert!(!uart.is_rx_ready());
        uart.poll_rx();
        assert!(uart.is_rx_ready());
        assert_eq!(uart.read_data(), b'B');
    }

    #[test]
    fn test_rx_overrun_error() {
        let mut kbd = Keyboard::new();
        kbd.type_text("ABC");
        let mut uart = Uart8251::new().with_keyboard(kbd);
        uart.write_control(0x4E);
        uart.write_control(0x05);
        uart.poll_rx();
        assert!(uart.is_rx_ready());
        uart.poll_rx();
        assert!(uart.read_status() & STATUS_OVERRUN_ERR != 0);
        assert_eq!(uart.read_data(), b'A');
        uart.write_control(0x14);
        assert!(uart.read_status() & STATUS_OVERRUN_ERR == 0);
    }

    #[test]
    fn test_rx_disabled_ignores_input() {
        let mut kbd = Keyboard::new();
        kbd.type_text("X");
        let mut uart = Uart8251::new().with_keyboard(kbd);
        uart.write_control(0x4E);
        uart.write_control(0x01);
        uart.poll_rx();
        assert!(!uart.is_rx_ready());
        uart.write_control(0x05);
        uart.poll_rx();
        assert!(uart.is_rx_ready());
    }

    #[test]
    fn test_status_register_bits() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x05);
        let status = uart.read_status();
        assert_eq!(status & STATUS_TX_READY, STATUS_TX_READY);
        assert_eq!(status & STATUS_TX_EMPTY, STATUS_TX_EMPTY);
        assert_eq!(status & STATUS_RX_READY, 0);
    }

    #[test]
    fn test_error_reset_command() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x05);
        uart.write_control(0x10);
        let status = uart.read_status();
        assert_eq!(
            status & (STATUS_PARITY_ERR | STATUS_OVERRUN_ERR | STATUS_FRAMING_ERR),
            0
        );
    }

    #[test]
    fn test_dtr_rts_bits() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x37);
        assert!(uart.dtr);
        assert!(uart.rts);
        assert!(uart.is_tx_enabled());
        assert!(uart.is_rx_enabled());
    }

    #[test]
    fn test_multiple_writes_and_drains() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x05);
        uart.write_data(b'H');
        uart.drain_tx();
        uart.update_tx();
        assert!(uart.is_tx_ready());
        uart.write_data(b'i');
        uart.drain_tx();
        uart.update_tx();
        let output = uart.take_output();
        assert_eq!(output, vec![b'H', b'i']);
    }

    #[test]
    fn test_internal_reset_clears_config() {
        let mut uart = Uart8251::new();
        uart.write_control(0x4E);
        uart.write_control(0x37);
        assert_eq!(uart.state(), UartState::Ready);
        uart.write_control(0x40);
        assert_eq!(uart.state(), UartState::ExpectMode);
        assert!(!uart.is_tx_enabled());
        assert!(!uart.is_rx_enabled());
        uart.write_control(0x4E);
        uart.write_control(0x05);
        assert_eq!(uart.state(), UartState::Ready);
        assert!(uart.is_tx_enabled());
    }
}

