//! Main emulator implementation
//!
//! The IMSAI 8080 emulator system: CPU + S-100 bus + front panel.
//!
//! On power-up, the front panel is in STOP mode. RAM is 0xFF (floating
//! bus state). No software, no ROM, no firmware. You use the front panel
//! switches to examine memory, deposit programs, and start execution.
//!
//! This is exactly how a real IMSAI 8080 works: you toggle programs in
//! from the front panel, or boot from disk after keying in a bootstrap
//! loader.

use crate::bus::ImsaiBus;
use crate::cards::{FrontPanel, PanelSwitch, RunState};
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
    /// Memory initialized to 0xFF (floating bus state).
    /// Front panel in STOP mode. No software loaded.
    pub fn new() -> Self {
        Self {
            cpu: Cpu8080::new(),
            bus: ImsaiBus::new(),
            panel: FrontPanel::new(),
        }
    }

    /// Load a program binary into memory at the given address.
    ///
    /// In real hardware, this is done via front panel DEPOSIT switches.
    /// This method is a convenience for testing and scripted setups.
    pub fn load_program(&mut self, start: u16, data: &[u8]) {
        self.bus.load(start, data);
    }

    /// Execute a single CPU instruction.
    ///
    /// Should only be called when the front panel is in RUN mode
    /// or for single-step operations.
    pub fn step(&mut self) -> u32 {
        // Clear transient LEDs before stepping
        self.panel.clear_transient_leds();

        let pc_before = self.cpu.pc;
        let op_byte = self.bus.mem_read(pc_before);
        let cycles = self.cpu.step(&mut self.bus);

        // Update the front panel with what happened. The data bus shows the
        // byte at the (new) PC -- the opcode the next M1 cycle will fetch.
        let data_bus = self.bus.mem_read(self.cpu.pc);
        self.panel.update_run_leds(&self.cpu, data_bus);

        // Check if this was an I/O instruction and log it
        if op_byte == 0xD3 {
            // OUT port,A
            let port = self.bus.mem_read(pc_before + 1);
            self.panel.log_io_write(self.cpu.cycles, port, self.cpu.a);
        } else if op_byte == 0xDB {
            // IN A,port
            let port = self.bus.mem_read(pc_before + 1);
            self.panel.log_io_read(self.cpu.cycles, port, self.cpu.a);
        }

        // Check if this was a memory instruction and log appropriately
        // (simplified: just track reads/writes at the instruction level)
        let is_write = self.is_memory_write_instruction(op_byte);
        if is_write {
            // For writes, we log the target address
            // The actual data is hard to get without deep CPU hooks,
            // so we just note that a write happened
        }

        cycles
    }

    /// Run the CPU for a batch of instructions.
    ///
    /// Stops when the front panel is in STOP mode, the CPU halts,
    /// or the instruction count is reached.
    pub fn run_batch(&mut self, max_instructions: u64) -> u64 {
        let mut count: u64 = 0;
        loop {
            if self.panel.is_stopped() || self.cpu.halted {
                break;
            }
            self.step();
            count += 1;
            if count >= max_instructions {
                break;
            }
        }
        count
    }

    /// Process all pending front panel actions.
    ///
    /// Call this after reading user input (keyboard events) to apply
    /// switch presses. Returns true if the CPU should start running.
    pub fn process_panel(&mut self) -> bool {
        self.panel.process_actions(&mut self.bus, &mut self.cpu)
    }

    /// Execute a single step from the front panel.
    ///
    /// Sets the CPU running for one instruction, then stops.
    /// Updates LEDs to show the new PC and the byte at that address.
    pub fn single_step(&mut self) {
        self.panel.do_single_step(&mut self.bus, &mut self.cpu);
    }

    /// Determine if an instruction writes to memory (simplified check).
    ///
    /// This is used for front panel LED updates and I/O logging.
    /// Not all cases are covered, but the common ones are.
    fn is_memory_write_instruction(&self, op: u8) -> bool {
        matches!(op,
            0x36 | // MVI M
            0x22 | // SHLD
            0x32 | // STA
            0x02 | // STAX B
            0x12    // STAX D
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_on_state() {
        let emu = Imsai8080::new();
        assert!(emu.panel.is_stopped());
        assert!(!emu.panel.is_running());
        assert!(emu.panel.leds().power);
        assert!(emu.panel.leds().wait);
        assert!(!emu.panel.leds().run);
    }

    #[test]
    fn test_front_panel_examine() {
        let mut emu = Imsai8080::new();

        // Write a byte to memory
        emu.bus.mem_write(0x1234, 0xAB);

        // Examine it
        emu.panel.set_address_switches(0x1234);
        emu.panel.press_switch(PanelSwitch::Examine);
        emu.process_panel();

        assert_eq!(bool_array_to_u8(emu.panel.leds().data), 0xAB);
    }

    #[test]
    fn test_front_panel_deposit() {
        let mut emu = Imsai8080::new();

        // Deposit a byte
        emu.panel.set_address_switches(0x0100);
        emu.panel.set_data_switches(0x3C);
        emu.panel.press_switch(PanelSwitch::Deposit);
        emu.process_panel();

        // Verify via bus
        assert_eq!(emu.bus.mem_read(0x0100), 0x3C);
    }

    #[test]
    fn test_front_panel_single_step() {
        let mut emu = Imsai8080::new();

        // Deposit a NOP at 0x0000 (0x00)
        emu.panel.set_address_switches(0x0000);
        emu.panel.set_data_switches(0x00); // NOP
        emu.panel.press_switch(PanelSwitch::Deposit);
        emu.process_panel();

        // Now set up for single step from address 0
        emu.panel.set_address_switches(0x0000);
        emu.single_step();

        // After stepping, PC should have advanced past the NOP
        assert!(emu.cpu.pc > 0x0000);
    }

    #[test]
    fn test_front_panel_run_and_stop() {
        let mut emu = Imsai8080::new();

        // Deposit a simple program: JMP 0x0000 (infinite loop)
        // 0xC3 0x00 0x00
        emu.panel.set_address_switches(0x0000);
        emu.panel.set_data_switches(0xC3);
        emu.panel.press_switch(PanelSwitch::Deposit);
        emu.process_panel();
        assert_eq!(emu.bus.mem_read(0x0000), 0xC3);

        // Deposit next bytes: 0x00 at 0x0001, 0x00 at 0x0002
        emu.panel.set_data_switches(0x00);
        emu.panel.press_switch(PanelSwitch::DepositNext);
        emu.process_panel();
        assert_eq!(emu.bus.mem_read(0x0001), 0x00);

        emu.panel.press_switch(PanelSwitch::DepositNext);
        emu.process_panel();
        assert_eq!(emu.bus.mem_read(0x0002), 0x00);

        // Start running from address 0
        emu.panel.set_address_switches(0x0000);
        emu.panel.press_switch(PanelSwitch::RunStop);
        emu.process_panel();

        assert!(emu.panel.is_running());

        // Run for a few instructions
        let count = emu.run_batch(100);
        assert!(count > 0);

        // Stop
        emu.panel.press_switch(PanelSwitch::RunStop);
        emu.process_panel();

        assert!(emu.panel.is_stopped());
    }

    #[test]
    fn test_memory_initializes_to_ff() {
        let mut emu = Imsai8080::new();

        // All memory should be 0xFF on power-up (floating bus)
        emu.panel.set_address_switches(0x0000);
        emu.panel.press_switch(PanelSwitch::Examine);
        emu.process_panel();
        assert_eq!(bool_array_to_u8(emu.panel.leds().data), 0xFF);

        emu.panel.set_address_switches(0xFFFF);
        emu.panel.press_switch(PanelSwitch::Examine);
        emu.process_panel();
        assert_eq!(bool_array_to_u8(emu.panel.leds().data), 0xFF);
    }

    /// Helper: convert bool array to u8 for test assertions.
    fn bool_array_to_u8(arr: [bool; 8]) -> u8 {
        let mut val: u8 = 0;
        for i in 0..8 {
            if arr[i] {
                val |= 1 << (7 - i);
            }
        }
        val
    }
}