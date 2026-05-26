//! System components for the IMSAI 8080 emulator

/// System configuration for the IMSAI 8080
pub struct SystemConfig;

impl Default for SystemConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemConfig {
    /// Create a new system configuration
    pub fn new() -> Self {
        Self
    }
}
