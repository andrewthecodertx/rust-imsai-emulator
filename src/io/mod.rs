//! I/O subsystem for the IMSAI 8080 emulator

/// I/O controller for the IMSAI 8080
pub struct IoController {
    /// Keyboard input controller
    pub keyboard: Keyboard,
    /// Video display controller
    pub video: VideoDisplay,
}

impl IoController {
    /// Create a new I/O controller
    pub fn new() -> Self {
        Self {
            keyboard: Keyboard::new(),
            video: VideoDisplay::new(80, 24), // Standard 80x24 text display
        }
    }

    /// Initialize the I/O system
    pub fn initialize(&mut self) {
        // Clear the display
        self.video.clear();
        // Render the initial blank display
        self.video.render();
    }
}

mod keyboard;
mod video;

pub use keyboard::Keyboard;
pub use video::VideoDisplay;
