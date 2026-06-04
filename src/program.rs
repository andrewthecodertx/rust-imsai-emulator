#![allow(missing_docs)] // Enum variant fields documented at variant level

use std::fs;
use std::path::Path;

use crate::Imsai8080;
use crate::PanelSwitch;

/// Parse a hex string like "3E" or "0x3E" into a u8.
pub fn parse_hex8(s: &str) -> Result<u8, String> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(s, 16).map_err(|e| format!("Invalid hex byte '{}': {}", s, e))
}

/// Parse a hex string like "0100" or "0x0100" into a u16.
pub fn parse_hex16(s: &str) -> Result<u16, String> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(s, 16).map_err(|e| format!("Invalid hex address '{}': {}", s, e))
}

/// Parse a space-separated hex string like "3E 41 D3 00" into bytes.
pub fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    s.split_whitespace().map(|b| parse_hex8(b)).collect()
}

/// A single front panel step.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action")]
#[serde(rename_all = "snake_case")]
pub enum PanelStep {
    Deposit { address: String, data: String },
    DepositNext { data: String },
    Examine { address: String },
    ExamineNext,
    Run { address: String },
    Load { address: String, data: String },
}

/// A front panel program: named sequence of steps.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PanelProgram {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub steps: Vec<PanelStep>,
}

/// Load a panel program from a JSON file.
pub fn load_program_file(path: &Path) -> Result<PanelProgram, String> {
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Save a panel program to a JSON file.
pub fn save_program_file(prog: &PanelProgram, path: &Path) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(prog).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Execute a panel program on the emulator.
///
/// Returns the first error encountered, with the step index and
/// a description of what went wrong. A malformed program stops
/// execution rather than silently writing garbage to memory.
pub fn execute_panel_program(emu: &mut Imsai8080, prog: &PanelProgram) -> Result<(), String> {
    for (i, step) in prog.steps.iter().enumerate() {
        match step {
            PanelStep::Deposit { address, data } => {
                let addr = parse_hex16(address)
                    .map_err(|e| format!("Step {}: deposit address: {}", i, e))?;
                let byte =
                    parse_hex8(data).map_err(|e| format!("Step {}: deposit data: {}", i, e))?;
                emu.panel.set_address_switches(addr);
                emu.panel.set_data_switches(byte);
                emu.panel.press_switch(PanelSwitch::Deposit);
                emu.process_panel();
            }
            PanelStep::DepositNext { data } => {
                let byte = parse_hex8(data)
                    .map_err(|e| format!("Step {}: deposit_next data: {}", i, e))?;
                emu.panel.set_data_switches(byte);
                emu.panel.press_switch(PanelSwitch::DepositNext);
                emu.process_panel();
            }
            PanelStep::Examine { address } => {
                let addr = parse_hex16(address)
                    .map_err(|e| format!("Step {}: examine address: {}", i, e))?;
                emu.panel.set_address_switches(addr);
                emu.panel.press_switch(PanelSwitch::Examine);
                emu.process_panel();
            }
            PanelStep::ExamineNext => {
                emu.panel.press_switch(PanelSwitch::ExamineNext);
                emu.process_panel();
            }
            PanelStep::Run { address } => {
                let addr =
                    parse_hex16(address).map_err(|e| format!("Step {}: run address: {}", i, e))?;
                emu.panel.set_address_switches(addr);
                emu.panel.press_switch(PanelSwitch::RunStop);
                emu.process_panel();
            }
            PanelStep::Load { address, data } => {
                let addr =
                    parse_hex16(address).map_err(|e| format!("Step {}: load address: {}", i, e))?;
                let bytes =
                    parse_hex_bytes(data).map_err(|e| format!("Step {}: load data: {}", i, e))?;
                emu.load_program(addr, &bytes);
            }
        }
    }
    Ok(())
}

/// Find the start address from a panel program (first "run", "deposit", or "load" step).
pub fn find_program_start(prog: &PanelProgram) -> Option<u16> {
    for step in &prog.steps {
        match step {
            PanelStep::Run { address } => return parse_hex16(address).ok(),
            PanelStep::Deposit { address, .. } => return parse_hex16(address).ok(),
            PanelStep::Load { address, .. } => return parse_hex16(address).ok(),
            _ => {}
        }
    }
    None
}

/// Create a panel program from a region of memory.
pub fn memory_to_program(emu: &Imsai8080, start: u16, len: u16) -> PanelProgram {
    let mut steps = Vec::new();
    let mut hex_bytes = Vec::new();
    let mut offset: u16 = 0;

    while offset < len {
        let remaining = len - offset;
        let chunk_size = remaining.min(16);
        let mut chunk = String::new();
        for i in 0..chunk_size {
            let addr = start.wrapping_add(offset + i);
            let byte = emu.bus.mem_read(addr);
            if i > 0 {
                chunk.push(' ');
            }
            chunk.push_str(&format!("{:02X}", byte));
        }
        hex_bytes.push(chunk);
        offset += chunk_size;
    }

    for (i, chunk) in hex_bytes.iter().enumerate() {
        let addr = start.wrapping_add((i * 16) as u16);
        steps.push(PanelStep::Load {
            address: format!("{:04X}", addr),
            data: chunk.clone(),
        });
    }

    steps.push(PanelStep::Run {
        address: format!("{:04X}", start),
    });

    PanelProgram {
        name: format!("dump_{:04X}", start),
        description: format!("Memory dump from 0x{:04X}, {} bytes", start, len),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_emu() -> Imsai8080 {
        Imsai8080::new()
    }

    #[test]
    fn test_valid_program_executes() {
        let mut emu = make_emu();
        let prog = PanelProgram {
            name: "test".to_string(),
            description: String::new(),
            steps: vec![
                PanelStep::Load {
                    address: "0000".to_string(),
                    data: "3E 41 D3 00 C3 00 00".to_string(),
                },
                PanelStep::Run {
                    address: "0000".to_string(),
                },
            ],
        };
        assert!(execute_panel_program(&mut emu, &prog).is_ok());
        // Byte at 0x0000 should be 0x3E (MVI A)
        assert_eq!(emu.bus.mem_read(0x0000), 0x3E);
    }

    #[test]
    fn test_invalid_address_returns_error() {
        let mut emu = make_emu();
        let prog = PanelProgram {
            name: "bad".to_string(),
            description: String::new(),
            steps: vec![PanelStep::Deposit {
                address: "ZZZZ".to_string(),
                data: "41".to_string(),
            }],
        };
        let result = execute_panel_program(&mut emu, &prog);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deposit address"));
    }

    #[test]
    fn test_invalid_data_returns_error() {
        let mut emu = make_emu();
        let prog = PanelProgram {
            name: "bad".to_string(),
            description: String::new(),
            steps: vec![PanelStep::Deposit {
                address: "0100".to_string(),
                data: "ZZ".to_string(),
            }],
        };
        let result = execute_panel_program(&mut emu, &prog);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deposit data"));
    }

    #[test]
    fn test_invalid_load_hex_returns_error() {
        let mut emu = make_emu();
        let prog = PanelProgram {
            name: "bad".to_string(),
            description: String::new(),
            steps: vec![PanelStep::Load {
                address: "0100".to_string(),
                data: "3E ZZ".to_string(),
            }],
        };
        let result = execute_panel_program(&mut emu, &prog);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("load data"));
    }

    #[test]
    fn test_invalid_run_address_returns_error() {
        let mut emu = make_emu();
        let prog = PanelProgram {
            name: "bad".to_string(),
            description: String::new(),
            steps: vec![PanelStep::Run {
                address: "GGGG".to_string(),
            }],
        };
        let result = execute_panel_program(&mut emu, &prog);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("run address"));
    }

    #[test]
    fn test_error_includes_step_index() {
        let mut emu = make_emu();
        let prog = PanelProgram {
            name: "bad".to_string(),
            description: String::new(),
            steps: vec![
                PanelStep::Deposit {
                    address: "0100".to_string(),
                    data: "41".to_string(),
                },
                PanelStep::Load {
                    address: "XXXX".to_string(),
                    data: "3E".to_string(),
                },
            ],
        };
        let result = execute_panel_program(&mut emu, &prog);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Step 1 (0-indexed) should be the failing one
        assert!(
            err.contains("Step 1"),
            "Expected step index in error: {}",
            err
        );
    }
}
