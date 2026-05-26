//! CP/M loader and execution module for the IMSAI 8080 emulator
//!
//! This module provides functionality to load and run CP/M on the
//! emulated IMSAI 8080 system.

use std::fs;
use std::path::Path;

/// CP/M system loader
pub struct CpMLoader;

impl CpMLoader {
    /// Create a new CP/M loader
    pub fn new() -> Self {
        Self
    }

    /// Load CP/M from a ROM file
    pub fn load_cpm_from_rom(&self, rom_path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Check if the ROM file exists
        if !Path::new(rom_path).exists() {
            return Err(format!("ROM file '{}' not found", rom_path).into());
        }

        // Read the ROM file
        let rom_data = fs::read(rom_path)?;
        Ok(rom_data)
    }

    /// Load CP/M from embedded resources (placeholder)
    pub fn load_cpm_embedded(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // In a real implementation, this would load CP/M from embedded resources
        // For now, we'll return an empty vector to indicate CP/M is not available
        println!("CP/M not found in embedded resources. You need to provide a CP/M ROM file.");
        Err("CP/M not available".into())
    }

    /// Install CP/M into the emulator's memory
    pub fn install_cpm(
        &self,
        memory: &mut [u8],
        cpm_data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if we have enough space for CP/M
        if cpm_data.len() > memory.len() {
            return Err("CP/M image is too large for available memory".into());
        }

        // Copy CP/M data to memory starting at address 0
        memory[..cpm_data.len()].copy_from_slice(cpm_data);

        println!("CP/M loaded successfully ({} bytes)", cpm_data.len());
        Ok(())
    }

    /// Initialize CP/M system parameters
    pub fn initialize_cpm_system(&self, memory: &mut [u8]) {
        // Initialize key CP/M system locations:
        // - Set initial stack pointer (SP) location
        // - Set warm boot vector
        // - Initialize IOBYTE at 0x0003
        // - Initialize command line buffer at 0x0080

        // IOBYTE at 0x0003 (console only)
        memory[0x0003] = 0x00; // Console device only

        // Command line buffer at 0x0080
        memory[0x0080] = 0x00; // Length byte = 0 (empty command line)

        // Initialize warm boot vector
        // This would typically point to the BIOS warm boot routine

        println!("CP/M system initialized");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpm_loader_creation() {
        let loader = CpMLoader::new();
        assert!(true); // Just test that we can create the loader
    }

    #[test]
    fn test_load_missing_rom() {
        let loader = CpMLoader::new();
        let result = loader.load_cpm_from_rom("nonexistent.rom");
        assert!(result.is_err());
    }
}
