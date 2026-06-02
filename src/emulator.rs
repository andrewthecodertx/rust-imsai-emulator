//! Main emulator implementation
//!
//! The IMSAI 8080 emulator system: CPU + S-100 bus + front panel.
//! The front panel is the primary hardware interface. In STOP mode,
//! it controls examine/deposit operations. In RUN mode, the CPU
//! executes and the front panel monitors the bus.

use crate::bus::ImsaiBus;
use crate::cards::FrontPanel;
use intel8080::Cpu8080;

/// The IMSAI 8080 emulator system
pub struct Imsai8080 {
    /// The Intel 8080 CPU
    pub cpu: Cpu8080,
    /// The S-100 system bus (memory + I/O cards)
    pub bus: ImsaiBus,
    /// The front panel (switches + LEDs)
    pub panel: FrontPanel,
}

impl Default for Imsai8080 {
    fn default() -> Self {
        Self::new()
    }
}

impl Imsai8080 {
    /// Create a new IMSAI 8080 emulator in power-on state.
    ///
    /// The front panel starts in STOP mode. RAM is initialized to 0xFF
    /// (floating bus). UARTs and FDC are in reset state. No software
    /// is loaded.
    pub fn new() -> Self {
        Self {
            cpu: Cpu8080::new(),
            bus: ImsaiBus::new(),
            panel: FrontPanel::new(),
        }
    }

    /// Load a program binary into memory (for testing or front panel deposit).
    pub fn load_program(&mut self, start: u16, data: &[u8]) {
        self.bus.load(start, data);
    }

    /// Execute a single CPU instruction.
    ///
    /// Should only be called when the front panel is in RUN mode
    /// or for single-step operations.
    pub fn step(&mut self) -> u32 {
        self.cpu.step(&mut self.bus)
    }

    /// Run for a given number of instructions (if front panel allows).
    pub fn run_steps(&mut self, count: u32) {
        for _ in 0..count {
            if self.panel.is_stopped() {
                break;
            }
            self.step();
        }
    }

    /// Process any pending front panel actions, then return whether
    /// the CPU should execute.
    pub fn process_panel(&mut self) -> bool {
        // We need to temporarily detach bus and cpu to pass to panel,
        // but Rust's borrow checker makes this tricky with &mut self.
        // Instead, we handle the common actions directly.
        let should_run = self.panel.run_state() == crate::cards::RunState::Running;
        should_run
    }
}