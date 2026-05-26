//! I/O subsystem for the IMSAI 8080 emulator
//!
//! The I/O controller manages all peripheral devices on the S-100 bus:
//! - Console (keyboard + video display via ports 0x00-0x01)
//! - Tarbell floppy disk controller (ports 0x48-0x4B)

/// I/O controller for the IMSAI 8080
pub struct IoController {
    /// Keyboard input controller
    pub keyboard: Keyboard,
    /// Video display controller
    pub video: VideoDisplay,
    /// Tarbell floppy disk controller
    pub tarbell: TarbellController,
}

impl Default for IoController {
    fn default() -> Self {
        Self::new()
    }
}

impl IoController {
    /// Create a new I/O controller
    pub fn new() -> Self {
        Self {
            keyboard: Keyboard::new(),
            video: VideoDisplay::new(80, 24),
            tarbell: TarbellController::new(),
        }
    }

    /// Initialize the I/O system
    pub fn initialize(&mut self) {
        self.video.clear();
        self.video.render();
    }
}

mod keyboard;
mod tarbell;
mod video;

pub use keyboard::Keyboard;
pub use tarbell::TarbellController;
pub use video::VideoDisplay;
