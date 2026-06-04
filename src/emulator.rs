
use crate::bus::ImsaiBus;
use crate::cards::FrontPanel;
#[cfg(test)]
use crate::cards::PanelSwitch;
use intel8080::Cpu8080;

pub struct Imsai8080 {
    pub cpu: Cpu8080,
    pub bus: ImsaiBus,
    pub panel: FrontPanel,
}

impl Default for Imsai8080 {
    fn default() -> Self {
        Self::new()
    }
}

impl Imsai8080 {
    /// Power-on state: RAM=0xFF, panel STOPPED, no software loaded.
    pub fn new() -> Self {
        Self {
            cpu: Cpu8080::new(),
            bus: ImsaiBus::new(),
            panel: FrontPanel::new(),
        }
    }

    pub fn load_program(&mut self, start: u16, data: &[u8]) {
        self.bus.load(start, data);
    }

    /// Execute one CPU instruction. Call only when panel is in RUN mode.
    pub fn step(&mut self) -> u32 {
        self.panel.clear_transient_leds();

        let pc_before = self.cpu.pc;
        let op_byte = self.bus.mem_read(pc_before);
        let cycles = self.cpu.step(&mut self.bus);

        let data_bus = self.bus.mem_read(self.cpu.pc);
        self.panel.update_run_leds(&self.cpu, data_bus);

        if op_byte == 0xD3 {
            let port = self.bus.mem_read(pc_before.wrapping_add(1));
            self.panel.log_io_write(self.cpu.cycles, port, self.cpu.a);
        } else if op_byte == 0xDB {
            let port = self.bus.mem_read(pc_before.wrapping_add(1));
            self.panel.log_io_read(self.cpu.cycles, port, self.cpu.a);
        }

        cycles
    }

    /// Run up to `max_instructions`. Stops if panel is STOPPED or CPU halts.
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

    pub fn process_panel(&mut self) -> bool {
        self.panel.process_actions(&mut self.bus, &mut self.cpu)
    }

    pub fn single_step(&mut self) {
        self.panel.do_single_step(&mut self.bus, &mut self.cpu);
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
        emu.bus.mem_write(0x1234, 0xAB);
        emu.panel.set_address_switches(0x1234);
        emu.panel.press_switch(PanelSwitch::Examine);
        emu.process_panel();
        assert_eq!(bool_array_to_u8(emu.panel.leds().data), 0xAB);
    }

    #[test]
    fn test_front_panel_deposit() {
        let mut emu = Imsai8080::new();
        emu.panel.set_address_switches(0x0100);
        emu.panel.set_data_switches(0x3C);
        emu.panel.press_switch(PanelSwitch::Deposit);
        emu.process_panel();
        assert_eq!(emu.bus.mem_read(0x0100), 0x3C);
    }

    #[test]
    fn test_front_panel_single_step() {
        let mut emu = Imsai8080::new();
        emu.panel.set_address_switches(0x0000);
        emu.panel.set_data_switches(0x00);
        emu.panel.press_switch(PanelSwitch::Deposit);
        emu.process_panel();
        emu.panel.set_address_switches(0x0000);
        emu.single_step();
        assert!(emu.cpu.pc > 0x0000);
    }

    #[test]
    fn test_front_panel_run_and_stop() {
        let mut emu = Imsai8080::new();
        emu.panel.set_address_switches(0x0000);
        emu.panel.set_data_switches(0xC3);
        emu.panel.press_switch(PanelSwitch::Deposit);
        emu.process_panel();
        emu.panel.set_data_switches(0x00);
        emu.panel.press_switch(PanelSwitch::DepositNext);
        emu.process_panel();
        emu.panel.press_switch(PanelSwitch::DepositNext);
        emu.process_panel();
        emu.panel.set_address_switches(0x0000);
        emu.panel.press_switch(PanelSwitch::RunStop);
        emu.process_panel();
        assert!(emu.panel.is_running());
        let count = emu.run_batch(100);
        assert!(count > 0);
        emu.panel.press_switch(PanelSwitch::RunStop);
        emu.process_panel();
        assert!(emu.panel.is_stopped());
    }

    #[test]
    fn test_memory_initializes_to_ff() {
        let mut emu = Imsai8080::new();
        emu.panel.set_address_switches(0x0000);
        emu.panel.press_switch(PanelSwitch::Examine);
        emu.process_panel();
        assert_eq!(bool_array_to_u8(emu.panel.leds().data), 0xFF);
        emu.panel.set_address_switches(0xFFFF);
        emu.panel.press_switch(PanelSwitch::Examine);
        emu.process_panel();
        assert_eq!(bool_array_to_u8(emu.panel.leds().data), 0xFF);
    }

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