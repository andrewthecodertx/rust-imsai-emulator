//! IMSAI 8080 front panel (switches and LEDs)
//!
//! The IMSAI 8080 front panel is the defining hardware feature of the machine.
//! With no ROM, no BIOS, and no firmware, the front panel is your only
//! interface to hardware. You use it to:
//!
//! - Examine memory: set 16 address switches, press EXAMINE, read data LEDs
//! - Deposit memory: set address + data switches, press DEPOSIT
//! - Run a program: set PC via examine/deposit, toggle RUN
//! - Single-step: press SINGLE STEP to execute one instruction
//! - Stop: toggle STOP to halt the CPU
//!
//! The front panel connects directly to the S-100 bus. In STOP mode, it
//! controls the bus (address and data lines). In RUN mode, the CPU drives
//! the bus and the panel just monitors (LEDs follow the bus).
//!
//! Switch layout (physical order, left to right):
//! - 16 address toggle switches (A15..A0, up=1, down=0)
//! - 8 data toggle switches (D7..D0, up=1, down=0)
//! - Function switches: RUN/STOP, SINGLE STEP, EXAMINE, DEPOSIT,
//!   EXAMINE NEXT, DEPOSIT NEXT
//!
//! LED layout:
//! - 16 address LEDs (show current address bus)
//! - 8 data LEDs (show current data bus)
//! - Status LEDs: RUN, M1, WAIT, INT, HLDA, POWER

/// Front panel function switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSwitch {
    /// Toggle between RUN and STOP
    RunStop,
    /// Execute one instruction (M1 cycle), then stop
    SingleStep,
    /// Read byte at address switch position into data LEDs
    Examine,
    /// Write data switches into memory at address switch position
    Deposit,
    /// Increment address, then examine
    ExamineNext,
    /// Increment address, then deposit
    DepositNext,
}

/// Front panel status LEDs.
#[derive(Debug, Clone, Default)]
pub struct PanelLeds {
    /// 16 address LEDs (true = ON)
    pub address: [bool; 16],
    /// 8 data LEDs (true = ON)
    pub data: [bool; 8],
    /// CPU is running
    pub run: bool,
    /// Machine cycle 1 active (instruction fetch)
    pub m1: bool,
    /// CPU in wait state
    pub wait: bool,
    /// Interrupt acknowledge
    pub int: bool,
    /// Hold acknowledge (DMA)
    pub hlda: bool,
    /// Power on
    pub power: bool,
}

/// CPU run state controlled by the front panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// CPU halted, front panel controls bus
    Stopped,
    /// CPU executing, front panel monitors bus
    Running,
}

/// IMSAI 8080 front panel.
///
/// Models the physical front panel with toggle switches and LEDs.
/// The panel directly accesses the S-100 bus for examine/deposit
/// operations (no CPU involvement needed).
pub struct FrontPanel {
    /// 16 address toggle switches (bit 15 = A15 = MSB)
    address_switches: u16,
    /// 8 data toggle switches (bit 7 = D7 = MSB)
    data_switches: u8,
    /// Current LED state
    leds: PanelLeds,
    /// Run state (STOPPED or RUNNING)
    run_state: RunState,
    /// Pending switch actions (pressed since last update)
    pending_actions: Vec<PanelSwitch>,
}

impl Default for FrontPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl FrontPanel {
    /// Create a new front panel in power-on state (STOPPED, switches at 0).
    pub fn new() -> Self {
        let mut panel = Self {
            address_switches: 0,
            data_switches: 0,
            leds: PanelLeds::default(),
            run_state: RunState::Stopped,
            pending_actions: Vec::new(),
        };
        panel.leds.power = true;
        panel.update_leds_from_state();
        panel
    }

    // -------------------------------------------------------------------
    // Switch setters (the host terminal/user sets these)
    // -------------------------------------------------------------------

    /// Set the 16 address toggle switches.
    /// Updates address LEDs to match (hardwired in real hardware).
    pub fn set_address_switches(&mut self, addr: u16) {
        self.address_switches = addr;
        self.leds.address = u16_to_bool_array(addr);
    }

    /// Set the 8 data toggle switches.
    /// Updates data LEDs to match (hardwired in real hardware).
    pub fn set_data_switches(&mut self, data: u8) {
        self.data_switches = data;
        self.leds.data = u8_to_bool_array(data);
    }

    /// Get the current address switch value.
    pub fn address_switches(&self) -> u16 {
        self.address_switches
    }

    /// Get the current data switch value.
    pub fn data_switches(&self) -> u8 {
        self.data_switches
    }

    /// Press a function switch. The action is queued and processed
    /// on the next call to `process_actions()`.
    pub fn press_switch(&mut self, switch: PanelSwitch) {
        self.pending_actions.push(switch);
    }

    // -------------------------------------------------------------------
    // LED accessors
    // -------------------------------------------------------------------

    /// Get the current LED state.
    pub fn leds(&self) -> &PanelLeds {
        &self.leds
    }

    /// Get the current run state.
    pub fn run_state(&self) -> RunState {
        self.run_state
    }

    /// Check if the panel is in RUN mode.
    pub fn is_running(&self) -> bool {
        self.run_state == RunState::Running
    }

    /// Check if the panel is in STOP mode.
    pub fn is_stopped(&self) -> bool {
        self.run_state == RunState::Stopped
    }

    // -------------------------------------------------------------------
    // Action processing (called each emulator tick)
    // -------------------------------------------------------------------

    /// Process all pending switch actions.
    ///
    /// Takes a mutable reference to the bus (for examine/deposit) and
    /// the CPU (for run/stop/single-step control).
    ///
    /// Returns true if the CPU should execute (RUN or SINGLE STEP).
    /// Returns false if the CPU should stay halted (STOP).
    pub fn process_actions(
        &mut self,
        bus: &mut crate::bus::ImsaiBus,
        cpu: &mut intel8080::Cpu8080,
    ) -> bool {
        let mut should_run = self.run_state == RunState::Running;

        // Drain pending actions first to avoid double borrow
        let actions: Vec<PanelSwitch> = self.pending_actions.drain(..).collect();

        for action in actions {
            match action {
                PanelSwitch::RunStop => {
                    if self.run_state == RunState::Stopped {
                        self.run_state = RunState::Running;
                        should_run = true;
                        // When entering RUN, set CPU PC to address switches
                        // (this is how a real IMSAI front panel works:
                        //  the RUN switch starts execution from the current
                        //  address on the bus, which is the address switches)
                        cpu.pc = self.address_switches;
                    } else {
                        self.run_state = RunState::Stopped;
                        should_run = false;
                    }
                }
                PanelSwitch::SingleStep => {
                    // Single step: execute one instruction, then stop
                    // The caller should step the CPU once, and we'll
                    // set state back to Stopped after.
                    should_run = false; // We return false; caller handles the step
                    self.run_state = RunState::Stopped;
                }
                PanelSwitch::Examine => {
                    self.examine(bus);
                }
                PanelSwitch::Deposit => {
                    self.deposit(bus);
                }
                PanelSwitch::ExamineNext => {
                    self.address_switches = self.address_switches.wrapping_add(1);
                    self.examine(bus);
                }
                PanelSwitch::DepositNext => {
                    self.address_switches = self.address_switches.wrapping_add(1);
                    self.deposit(bus);
                }
            }
        }

        self.update_leds_from_state();
        should_run
    }

    /// Process a single step: execute one CPU instruction and update LEDs.
    pub fn do_single_step(
        &mut self,
        bus: &mut crate::bus::ImsaiBus,
        cpu: &mut intel8080::Cpu8080,
    ) {
        // Execute one instruction
        cpu.step(bus);

        // Update LEDs to show new CPU state
        self.leds.address = u16_to_bool_array(cpu.pc);
        self.leds.data = u8_to_bool_array(bus.mem_read(cpu.pc));
        self.leds.m1 = true; // We just fetched an instruction
        self.leds.run = false; // Stopped after single step
    }

    // -------------------------------------------------------------------
    // Internal: examine/deposit operations
    // -------------------------------------------------------------------

    /// Examine: read the byte at the address switch position and
    /// display it on the data LEDs.
    fn examine(&mut self, bus: &crate::bus::ImsaiBus) {
        let data = bus.mem_read(self.address_switches);
        self.leds.address = u16_to_bool_array(self.address_switches);
        self.leds.data = u8_to_bool_array(data);
    }

    /// Deposit: write the data switch value into memory at the
    /// address switch position.
    fn deposit(&mut self, bus: &mut crate::bus::ImsaiBus) {
        bus.mem_write(self.address_switches, self.data_switches);
        self.leds.address = u16_to_bool_array(self.address_switches);
        self.leds.data = u8_to_bool_array(self.data_switches);
    }

    /// Update LEDs from current run state.
    ///
    /// When stopped and no examine/deposit was just performed,
    /// LEDs show the switch positions. After examine/deposit,
    /// LEDs show the examined/deposited data.
    fn update_leds_from_state(&mut self) {
        self.leds.run = self.run_state == RunState::Running;
        self.leds.power = true;
        // Note: do NOT overwrite address/data LEDs here.
        // examine() and deposit() set the LEDs directly.
        // Switch-to-LED display is handled in new() and in
        // set_address_switches/set_data_switches if the user
        // wants switch positions shown.
    }
}

// -------------------------------------------------------------------
// Utility: convert integers to bool arrays for LED display
// -------------------------------------------------------------------

fn u16_to_bool_array(val: u16) -> [bool; 16] {
    let mut arr = [false; 16];
    for i in 0..16 {
        arr[i] = val & (1 << (15 - i)) != 0;
    }
    arr
}

fn u8_to_bool_array(val: u8) -> [bool; 8] {
    let mut arr = [false; 8];
    for i in 0..8 {
        arr[i] = val & (1 << (7 - i)) != 0;
    }
    arr
}

fn bool_array_to_u16(arr: [bool; 16]) -> u16 {
    let mut val: u16 = 0;
    for i in 0..16 {
        if arr[i] {
            val |= 1 << (15 - i);
        }
    }
    val
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_on_state() {
        let panel = FrontPanel::new();
        assert_eq!(panel.run_state(), RunState::Stopped);
        assert!(!panel.is_running());
        assert!(panel.is_stopped());
        assert_eq!(panel.address_switches(), 0);
        assert_eq!(panel.data_switches(), 0);
        assert!(panel.leds().power);
        assert!(!panel.leds().run);
    }

    #[test]
    fn test_set_address_switches() {
        let mut panel = FrontPanel::new();
        panel.set_address_switches(0x1234);
        assert_eq!(panel.address_switches(), 0x1234);
    }

    #[test]
    fn test_set_data_switches() {
        let mut panel = FrontPanel::new();
        panel.set_data_switches(0xAB);
        assert_eq!(panel.data_switches(), 0xAB);
    }

    #[test]
    fn test_examine_and_deposit() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        // CPU is not needed for examine/deposit, pass a dummy
        let mut cpu = intel8080::Cpu8080::new();

        // Write a byte to memory
        bus.mem_write(0x00FF, 0x42);

        // Examine it
        panel.set_address_switches(0x00FF);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);

        // Data LEDs should show 0x42
        assert_eq!(bool_array_to_u8(panel.leds().data), 0x42);
        assert_eq!(bool_array_to_u16(panel.leds().address), 0x00FF);
    }

    #[test]
    fn test_deposit() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Deposit 0x3C at address 0x0100
        panel.set_address_switches(0x0100);
        panel.set_data_switches(0x3C);
        panel.press_switch(PanelSwitch::Deposit);
        panel.process_actions(&mut bus, &mut cpu);

        // Read it back via examine
        panel.set_data_switches(0x00); // Clear data switches
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bool_array_to_u8(panel.leds().data), 0x3C);
        // Also verify via bus directly
        assert_eq!(bus.mem_read(0x0100), 0x3C);
    }

    #[test]
    fn test_examine_next() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Write two bytes
        bus.mem_write(0x0200, 0xAA);
        bus.mem_write(0x0201, 0xBB);

        // Examine first byte
        panel.set_address_switches(0x0200);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);
        assert_eq!(bool_array_to_u8(panel.leds().data), 0xAA);

        // Examine next (auto-increments address)
        panel.press_switch(PanelSwitch::ExamineNext);
        panel.process_actions(&mut bus, &mut cpu);
        assert_eq!(bool_array_to_u8(panel.leds().data), 0xBB);
        assert_eq!(panel.address_switches(), 0x0201);
    }

    #[test]
    fn test_deposit_next() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Deposit at 0x0300
        panel.set_address_switches(0x0300);
        panel.set_data_switches(0x11);
        panel.press_switch(PanelSwitch::Deposit);
        panel.process_actions(&mut bus, &mut cpu);

        // Deposit next (auto-increments address)
        panel.set_data_switches(0x22);
        panel.press_switch(PanelSwitch::DepositNext);
        panel.process_actions(&mut bus, &mut cpu);

        // Verify both bytes
        assert_eq!(bus.mem_read(0x0300), 0x11);
        assert_eq!(bus.mem_read(0x0301), 0x22);
        assert_eq!(panel.address_switches(), 0x0301);
    }

    #[test]
    fn test_run_stop_toggle() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Start in STOPPED state
        assert!(panel.is_stopped());

        // Press RUN/STOP to enter RUN
        panel.set_address_switches(0x0100);
        panel.press_switch(PanelSwitch::RunStop);
        let should_run = panel.process_actions(&mut bus, &mut cpu);

        assert!(panel.is_running());
        assert!(should_run);
        assert_eq!(cpu.pc, 0x0100); // PC set to address switches

        // Press RUN/STOP again to enter STOP
        panel.press_switch(PanelSwitch::RunStop);
        let should_run = panel.process_actions(&mut bus, &mut cpu);

        assert!(panel.is_stopped());
        assert!(!should_run);
    }

    #[test]
    fn test_single_step_returns_stopped() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        panel.press_switch(PanelSwitch::SingleStep);
        let should_run = panel.process_actions(&mut bus, &mut cpu);

        assert!(panel.is_stopped());
        assert!(!should_run); // Caller handles the actual step
    }

    #[test]
    fn test_leds_show_switches_when_stopped() {
        let mut panel = FrontPanel::new();
        panel.set_address_switches(0xBEEF);
        panel.set_data_switches(0xDE);

        // LEDs should mirror the switches when stopped
        assert_eq!(bool_array_to_u16(panel.leds().address), 0xBEEF);
        assert_eq!(bool_array_to_u8(panel.leds().data), 0xDE);
    }

    #[test]
    fn test_u16_bool_array_roundtrip() {
        let vals = [0x0000, 0xFFFF, 0x1234, 0xABCD, 0x8001];
        for &v in &vals {
            assert_eq!(bool_array_to_u16(u16_to_bool_array(v)), v);
        }
    }

    #[test]
    fn test_u8_bool_array_roundtrip() {
        let vals = [0x00, 0xFF, 0x42, 0xAB, 0x80];
        for &v in &vals {
            assert_eq!(bool_array_to_u8(u8_to_bool_array(v)), v);
        }
    }

    #[test]
    fn test_address_wrap_on_examine_next() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        panel.set_address_switches(0xFFFF);
        panel.press_switch(PanelSwitch::ExamineNext);
        panel.process_actions(&mut bus, &mut cpu);

        // Address should wrap to 0x0000
        assert_eq!(panel.address_switches(), 0x0000);
    }

    #[test]
    fn test_multiple_actions_queued() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Deposit at 0x0100
        panel.set_address_switches(0x0100);
        panel.set_data_switches(0x11);
        panel.press_switch(PanelSwitch::Deposit);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bus.mem_read(0x0100), 0x11);

        // Then deposit next at 0x0101 (auto-increment address)
        panel.set_data_switches(0x22);
        panel.press_switch(PanelSwitch::DepositNext);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bus.mem_read(0x0101), 0x22);
    }
}