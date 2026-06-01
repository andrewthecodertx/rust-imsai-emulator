//! S-100 bus card trait and standard cards
//!
//! In a real IMSAI 8080, the S-100 bus is a passive backplane. Cards plug in
//! and communicate via address lines, data lines, and control signals. Each
//! card owns a range of I/O ports and responds to reads/writes on those ports.
//!
//! This module defines the `Card` trait that all S-100 cards implement, and
//! provides the standard card implementations for the IMSAI 8080 emulator.

use crate::io::Keyboard;
use crate::io::VideoDisplay;
use crate::io::TarbellController;

/// An S-100 bus card.
///
/// Each card occupies a range of I/O ports and responds to read/write
/// operations on those ports. The bus dispatches I/O operations to the
/// appropriate card based on port address.
///
/// Cards own their own state (disk images, keyboard buffers, video memory,
/// etc.) and are entirely self-contained. The bus never reaches into a
/// card's internals directly.
pub trait Card {
    /// Read from an I/O port that this card owns.
    fn io_read(&mut self, port: u8) -> u8;

    /// Write to an I/O port that this card owns.
    fn io_write(&mut self, port: u8, value: u8);

    /// Check if this card responds to the given I/O port.
    fn owns_port(&self, port: u8) -> bool;

    /// Human-readable name for this card (for diagnostics).
    fn name(&self) -> &'static str;

    /// Support downcasting for typed card access.
    /// Required because Rust traits are not `Sized` by default and
    /// we need `dyn Card` to be downcastable to concrete types.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ---------------------------------------------------------------------------
// Console Card — ports 0x00-0x01
// ---------------------------------------------------------------------------

/// Console card combining keyboard input and video display output.
///
/// Ports:
/// - 0x00: Data port. Read: get keyboard character. Write: write character to display.
/// - 0x01: Status port. Read: bit 0 = key ready, bit 1 = display ready.
///
/// This represents a single S-100 card with a serial I/O chip (like the
/// IMSAI 4PIO or SIO-2) wired to a terminal.
pub struct ConsoleCard {
    /// Keyboard input buffer
    pub keyboard: Keyboard,
    /// Video display controller
    pub video: VideoDisplay,
}

impl ConsoleCard {
    /// Create a new console card with default 80x24 display.
    pub fn new() -> Self {
        Self {
            keyboard: Keyboard::new(),
            video: VideoDisplay::new(80, 24),
        }
    }

    /// Queue characters for the CP/M keyboard to read.
    ///
    /// This is the primary way to feed input into the emulator. The
    /// keyboard buffer is consumed by the CP/M CONIN BIOS routine
    /// when it reads port 0x00.
    pub fn type_text(&mut self, text: &str) {
        self.keyboard.type_text(text);
    }

    /// Check if a keyboard character is ready to read.
    pub fn is_key_ready(&self) -> bool {
        self.keyboard.is_char_ready()
    }

    /// Get a reference to the video display for rendering.
    pub fn video(&self) -> &VideoDisplay {
        &self.video
    }

    /// Get a mutable reference to the video display.
    pub fn video_mut(&mut self) -> &mut VideoDisplay {
        &mut self.video
    }

    /// Disable auto-rendering (for terminal mode where we handle output directly).
    pub fn set_auto_render(&mut self, enabled: bool) {
        self.video.auto_render = enabled;
    }
}

impl Default for ConsoleCard {
    fn default() -> Self {
        Self::new()
    }
}

impl Card for ConsoleCard {
    fn io_read(&mut self, port: u8) -> u8 {
        match port {
            0x00 => self.keyboard.read_char(),
            0x01 => {
                let mut status = 0x02; // Display always ready
                if self.keyboard.is_char_ready() {
                    status |= 0x01; // Key ready
                }
                status
            }
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, port: u8, value: u8) {
        match port {
            0x00 | 0x7B => {
                // Port 0x00 and 0x7B both go to the display
                self.video.write_char(value);
                if self.video.auto_render {
                    self.video.render();
                }
            }
            _ => {}
        }
    }

    fn owns_port(&self, port: u8) -> bool {
        // Console card owns ports 0x00, 0x01, and 0x7B (CMI5619 alias)
        port == 0x00 || port == 0x01 || port == 0x7B || port == 0x79
    }

    fn name(&self) -> &'static str {
        "Console"
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Tarbell Disk Controller Card — ports 0x48-0x4B
// ---------------------------------------------------------------------------

/// Tarbell 1011/1011B floppy disk controller card.
///
/// Ports:
/// - 0x48: Status register (read) / Command register (write)
/// - 0x49: Track register
/// - 0x4A: Sector register
/// - 0x4B: Data register
///
/// Also responds to CMI5619 aliases at 0xF8-0xFB, 0xFC (wait/DRQ),
/// 0xFD (DMA check), and 0xFF (front panel).
pub struct TarbellCard {
    controller: TarbellController,
}

impl TarbellCard {
    /// Create a new Tarbell card with no disks inserted.
    pub fn new() -> Self {
        Self {
            controller: TarbellController::new(),
        }
    }

    /// Insert a disk image into a drive.
    pub fn insert_disk(&mut self, drive: usize, path: &str) -> Result<(), String> {
        self.controller.insert_disk(drive, path)
    }

    /// Get a reference to the disk in a drive (for boot loading).
    pub fn get_disk(&self, drive: usize) -> Option<&crate::disk::DiskImage> {
        self.controller.get_disk(drive)
    }

    /// Get the current track register value (for diagnostics).
    pub fn current_track(&self) -> u8 {
        self.controller.current_track()
    }

    /// Get the current sector register value (for diagnostics).
    pub fn current_sector(&self) -> u8 {
        self.controller.current_sector()
    }
}

impl Default for TarbellCard {
    fn default() -> Self {
        Self::new()
    }
}

impl Card for TarbellCard {
    fn io_read(&mut self, port: u8) -> u8 {
        // Map CMI5619 aliases to Tarbell ports
        let tarbell_port = match port {
            0xF8 => 0x48,
            0xF9 => 0x49,
            0xFA => 0x4A,
            0xFB => 0x4B,
            0xFC => return self.controller.wait_port_value(),
            0xFD => return 0x00, // DMA check
            0xFF => return 0x03,  // Front panel: key ready + display ready
            _ => port,
        };
        self.controller.io_in(tarbell_port)
    }

    fn io_write(&mut self, port: u8, value: u8) {
        // Map CMI5619 aliases to Tarbell ports
        let tarbell_port = match port {
            0xF8 => 0x48,
            0xF9 => 0x49,
            0xFA => 0x4A,
            0xFB => 0x4B,
            0xFC | 0xFD | 0xFF => return, // Write-only control ports, ignored
            _ => port,
        };
        self.controller.io_out(tarbell_port, value);
    }

    fn owns_port(&self, port: u8) -> bool {
        // Tarbell primary: 0x48-0x4B
        // CMI5619 aliases: 0xF8-0xFB, 0xFC, 0xFD, 0xFF
        (0x48..=0x4B).contains(&port)
            || (0xF8..=0xFD).contains(&port)
            || port == 0xFF
    }

    fn name(&self) -> &'static str {
        "Tarbell"
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}