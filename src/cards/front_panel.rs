use std::collections::VecDeque;

/// Front panel function switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSwitch {
    RunStop,
    SingleStep,
    Examine,
    Deposit,
    ExamineNext,
    DepositNext,
}

/// Front panel status LEDs.
#[derive(Debug, Clone, Default)]
pub struct PanelLeds {
    pub address: [bool; 16],
    pub data: [bool; 8],
    pub run: bool,
    pub m1: bool,
    pub hlta: bool,
    pub wait: bool,
    pub int: bool,
    pub hlda: bool,
    pub power: bool,
    pub memr: bool,
    /// Memory write active. True during write cycles. The front panel LED is
    /// !WO (active-low), so the GUI renders `!mwrt` — lit when NOT writing.
    pub mwrt: bool,
    pub ior: bool,
    pub iow: bool,
    /// Programmed output: latched from the data bus on every OUT instruction.
    /// On real IMSAI hardware, the front panel has an 8-bit latch that captures
    /// whatever byte the CPU writes during any OUT instruction.
    pub programmed_output: [bool; 8],
}

/// CPU run state controlled by the front panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// CPU halted, front panel drives address/data bus
    Halted,
    /// CPU executing, front panel monitors bus
    Running,
}

/// I/O event logged by the bus-monitoring front panel.
///
/// When the panel is in RUN mode, it watches all bus transactions.
/// This lets you see what the CPU is doing: which addresses it reads,
/// which I/O ports it accesses, etc.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// Cycle count when this event happened
    pub cycle: u64,
    /// Address bus value
    pub address: u16,
    /// Data bus value (what was read or written)
    pub data: u8,
    /// Whether this was an I/O operation (vs memory)
    pub is_io: bool,
    /// Whether this was a write (vs read)
    pub is_write: bool,
    /// Whether this was an output to a port (OUT instruction)
    pub is_output: bool,
}

/// IMSAI 8080 front panel.
///
/// The front panel directly controls and monitors the S-100 bus.
/// In STOP mode, it drives the address/data lines for examine/deposit.
/// In RUN mode, it monitors the bus passively, updating LEDs to show
/// CPU activity.
///
/// The panel also logs I/O events, making it a hardware debugger
/// for the S-100 cards. You can see exactly which ports the CPU
/// reads/writes, and which memory addresses it accesses.
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
    /// Circular buffer of recent I/O events for monitoring
    io_log: VecDeque<IoEvent>,
    /// Maximum number of I/O events to keep
    io_log_capacity: usize,
    /// Total bus cycles seen while running
    cycle_count: u64,
    /// Address seen on last bus transaction (for LED monitoring)
    last_address: u16,
    /// Data seen on last bus transaction (for LED monitoring)
    last_data: u8,
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
            run_state: RunState::Halted,
            pending_actions: Vec::new(),
            io_log: VecDeque::new(),
            io_log_capacity: 256,
            cycle_count: 0,
            last_address: 0,
            last_data: 0,
        };
        panel.leds.power = true;
        panel.leds.wait = true; // CPU is in WAIT state on power-on
        panel
    }

    /// Set the 16 address toggle switches. Address LEDs update immediately.
    pub fn set_address_switches(&mut self, addr: u16) {
        self.address_switches = addr;
        if self.run_state == RunState::Halted {
            self.leds.address = u16_to_bool_array(addr);
        }
    }

    /// Set the 8 data toggle switches. Does NOT drive the data LEDs: on a real
    /// IMSAI, data lamps show the data bus (EXAMINE/DEPOSIT result or live CPU
    /// activity), not the switch positions.
    pub fn set_data_switches(&mut self, data: u8) {
        self.data_switches = data;
    }

    /// Get the current address switch value.
    pub fn address_switches(&self) -> u16 {
        self.address_switches
    }

    /// Get the current data switch value.
    pub fn data_switches(&self) -> u8 {
        self.data_switches
    }

    /// Press a function switch. Queued and processed on next `process_actions()`.
    pub fn press_switch(&mut self, switch: PanelSwitch) {
        self.pending_actions.push(switch);
    }

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
        self.run_state == RunState::Halted
    }

    /// Get recent I/O events logged while the CPU was running.
    pub fn io_log(&self) -> Vec<IoEvent> {
        self.io_log.iter().copied().collect()
    }

    /// Clear the I/O event log.
    pub fn clear_io_log(&mut self) {
        self.io_log.clear();
    }

    /// Get total cycle count since last reset.
    pub fn cycle_count(&self) -> u64 {
        self.cycle_count
    }

    /// Log an I/O read event (IN instruction).
    /// Call this from the bus when the CPU reads an I/O port.
    pub fn log_io_read(&mut self, cycle: u64, port: u8, value: u8) {
        self.cycle_count = cycle;
        self.last_address = port as u16;
        self.last_data = value;
        self.leds.ior = true;
        self.leds.iow = false;

        if self.io_log.len() >= self.io_log_capacity {
            self.io_log.pop_front();
        }
        self.io_log.push_back(IoEvent {
            cycle,
            address: port as u16,
            data: value,
            is_io: true,
            is_write: false,
            is_output: false,
        });
    }

    /// Log an I/O write event (OUT instruction).
    /// Call this from the bus when the CPU writes to an I/O port.
    pub fn log_io_write(&mut self, cycle: u64, port: u8, value: u8) {
        self.cycle_count = cycle;
        self.last_address = port as u16;
        self.last_data = value;
        self.leds.iow = true;
        self.leds.ior = false;
        self.leds.mwrt = false;
        // Latch the data bus value into the programmed output LEDs.
        // On real IMSAI hardware, the programmed output LEDs are active-low:
        // a 0 bit lights the LED, a 1 bit extinguishes it. We invert here so
        // that `programmed_output[j] = true` always means "LED on", consistent
        // with all other LEDs in PanelLeds.
        let inverted = !value;
        self.leds.programmed_output = u8_to_bool_array(inverted);

        if self.io_log.len() >= self.io_log_capacity {
            self.io_log.pop_front();
        }
        self.io_log.push_back(IoEvent {
            cycle,
            address: port as u16,
            data: value,
            is_io: true,
            is_write: true,
            is_output: true,
        });
    }

    /// Log a memory read event.
    pub fn log_mem_read(&mut self, cycle: u64, addr: u16, value: u8) {
        self.cycle_count = cycle;
        self.last_address = addr;
        self.last_data = value;
        self.leds.memr = true;
        self.leds.mwrt = false;
    }

    /// Log a memory write event.
    pub fn log_mem_write(&mut self, cycle: u64, addr: u16, value: u8) {
        self.cycle_count = cycle;
        self.last_address = addr;
        self.last_data = value;
        self.leds.mwrt = true;
        self.leds.memr = false;
    }

    /// Update LEDs to reflect current bus state during RUN mode.
    ///
    /// Call this after each CPU step to show live bus activity. `data_bus` is
    /// the byte currently on the data bus (the opcode about to be fetched at
    /// the new PC), which the address/data LEDs latch.
    ///
    /// If the CPU has executed HLT it is now halted: the RUN light goes out,
    /// and the WAIT and HLTA (halt acknowledge) lights come on, exactly as on
    /// a real IMSAI where HLT parks the processor in a wait state.
    pub fn update_run_leds(&mut self, cpu: &intel8080::Cpu8080, data_bus: u8) {
        if self.run_state == RunState::Running {
            self.leds.address = u16_to_bool_array(cpu.pc);
            self.leds.data = u8_to_bool_array(data_bus);
            if cpu.halted {
                self.leds.run = false;
                self.leds.wait = true;
                self.leds.hlta = true;
                self.leds.m1 = false;
                self.leds.memr = false;
            } else {
                self.leds.run = true;
                self.leds.wait = false;
                self.leds.hlta = false;
                // Every instruction begins with an M1 (opcode fetch) memory read.
                self.leds.m1 = true;
                self.leds.memr = true;
                // !WO (write-output-bar) is active-low on real hardware: it is
                // lit when the CPU is NOT writing. Most instructions end with
                // a read cycle, so the default state is lit.
                // mwrt=false means "not writing" → WO-bar is ON in the GUI.
                self.leds.mwrt = false;
            }
        }
    }

    /// Clear transient status LEDs (M1, IOR, IOW, MEMR, MWRT).
    /// These pulse on during bus transactions and should be cleared
    /// between cycles in a real panel. For our model, clear them
    /// before each CPU step so they reflect only the current instruction.
    pub fn clear_transient_leds(&mut self) {
        self.leds.m1 = false;
        self.leds.ior = false;
        self.leds.iow = false;
        self.leds.memr = false;
        self.leds.mwrt = false;
    }

    // Action processing (called each emulator tick)

    /// Process all pending switch actions.
    ///
    /// Takes a mutable reference to the bus (for examine/deposit) and
    /// the CPU (for run/stop/single-step control).
    ///
    /// Returns true if the CPU should execute (RUN mode active).
    /// Returns false if the CPU should stay halted (STOP mode).
    pub fn process_actions(
        &mut self,
        bus: &mut crate::bus::ImsaiBus,
        cpu: &mut intel8080::Cpu8080,
    ) -> bool {
        let mut should_run = self.run_state == RunState::Running;
        let actions: Vec<PanelSwitch> = self.pending_actions.drain(..).collect();

        for action in actions {
            match action {
                PanelSwitch::RunStop => {
                    if self.run_state == RunState::Halted {
                        self.run_state = RunState::Running;
                        should_run = true;
                        // In a real IMSAI, pressing RUN starts the CPU
                        // from whatever address is on the bus. The CPU
                        // register PC may have been set by previous
                        // examine/deposit operations. We load the
                        // address switches into PC as the start address.
                        cpu.pc = self.address_switches;
                        // Releasing the CPU also brings it out of any prior
                        // HLT state so it actually fetches and runs again.
                        cpu.halted = false;
                        self.leds.wait = false;
                        self.leds.hlta = false;
                    } else {
                        self.run_state = RunState::Halted;
                        should_run = false;
                        self.leds.wait = true;
                    }
                }
                PanelSwitch::SingleStep => {
                    // Single step: advance one instruction, then stop.
                    // The caller handles the actual CPU step.
                    should_run = false;
                    self.run_state = RunState::Halted;
                    self.leds.wait = true;
                }
                PanelSwitch::Examine => {
                    // EXAMINE loads the address switches into the program
                    // counter (as on a real IMSAI), so RUN / SINGLE STEP then
                    // start executing from the examined address.
                    cpu.pc = self.address_switches;
                    self.examine(bus);
                }
                PanelSwitch::Deposit => {
                    self.deposit(bus);
                }
                PanelSwitch::ExamineNext => {
                    self.address_switches = self.address_switches.wrapping_add(1);
                    cpu.pc = self.address_switches;
                    self.leds.address = u16_to_bool_array(self.address_switches);
                    self.examine(bus);
                }
                PanelSwitch::DepositNext => {
                    self.address_switches = self.address_switches.wrapping_add(1);
                    self.leds.address = u16_to_bool_array(self.address_switches);
                    self.deposit(bus);
                }
            }
        }

        self.leds.run = self.run_state == RunState::Running;
        self.leds.power = true;
        should_run
    }

    /// Execute a single CPU step and update the front panel.
    ///
    /// This is the single-step operation: run the CPU for one instruction,
    /// then stop. The address and data LEDs show the new PC and the
    /// byte at that address. Status LEDs show what happened.
    pub fn do_single_step(&mut self, bus: &mut crate::bus::ImsaiBus, cpu: &mut intel8080::Cpu8080) {
        let pc_before = cpu.pc;
        cpu.step(bus);

        // Update LEDs to show the new CPU state
        self.leds.address = u16_to_bool_array(cpu.pc);
        self.leds.data = u8_to_bool_array(bus.mem_read(cpu.pc));
        self.leds.m1 = true;
        self.leds.hlta = cpu.halted; // HLT reached?
        self.leds.run = false;
        self.leds.wait = true;
        self.address_switches = cpu.pc;

        self.io_log.push_back(IoEvent {
            cycle: cpu.cycles,
            address: pc_before,
            data: bus.mem_read(pc_before),
            is_io: false,
            is_write: false,
            is_output: false,
        });
    }

    /// Examine: put the address switches on the address bus, assert
    /// MEMR, and latch the data bus into the data LEDs.
    fn examine(&mut self, bus: &crate::bus::ImsaiBus) {
        let addr = self.address_switches;
        let data = bus.mem_read(addr);

        self.leds.address = u16_to_bool_array(addr);
        self.leds.data = u8_to_bool_array(data);
        self.leds.memr = true;
        self.leds.mwrt = false;
    }

    /// Deposit: put the address switches on the address bus, put the
    /// data switches on the data bus, and assert MWRT.
    fn deposit(&mut self, bus: &mut crate::bus::ImsaiBus) {
        let addr = self.address_switches;
        let data = self.data_switches;

        bus.mem_write(addr, data);

        self.leds.address = u16_to_bool_array(addr);
        self.leds.data = u8_to_bool_array(data);
        self.leds.mwrt = true;
        self.leds.memr = false;
    }
}

// Utility: convert integers to bool arrays for LED display

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

#[allow(dead_code)]
fn bool_array_to_u16(arr: [bool; 16]) -> u16 {
    let mut val: u16 = 0;
    for i in 0..16 {
        if arr[i] {
            val |= 1 << (15 - i);
        }
    }
    val
}

#[allow(dead_code)]
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
        assert_eq!(panel.run_state(), RunState::Halted);
        assert!(!panel.is_running());
        assert!(panel.is_stopped());
        assert_eq!(panel.address_switches(), 0);
        assert_eq!(panel.data_switches(), 0);
        assert!(panel.leds().power);
        assert!(!panel.leds().run);
        assert!(panel.leds().wait);
    }

    #[test]
    fn test_set_address_switches() {
        let mut panel = FrontPanel::new();
        panel.set_address_switches(0x1234);
        assert_eq!(panel.address_switches(), 0x1234);

        assert_eq!(bool_array_to_u16(panel.leds().address), 0x1234);
    }

    #[test]
    fn test_set_data_switches() {
        let mut panel = FrontPanel::new();
        panel.set_data_switches(0xAB);
        assert_eq!(panel.data_switches(), 0xAB);

        // data bus (EXAMINE/DEPOSIT result), so they stay clear here.
        assert_eq!(bool_array_to_u8(panel.leds().data), 0x00);
    }

    #[test]
    fn test_examine_reads_memory() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Write a byte to memory
        bus.mem_write(0x00FF, 0x42);

        // Examine it
        panel.set_address_switches(0x00FF);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bool_array_to_u8(panel.leds().data), 0x42);

        assert_eq!(bool_array_to_u16(panel.leds().address), 0x00FF);

        assert!(panel.leds().memr);
    }

    #[test]
    fn test_deposit_writes_memory() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Deposit 0x3C at address 0x0100
        panel.set_address_switches(0x0100);
        panel.set_data_switches(0x3C);
        panel.press_switch(PanelSwitch::Deposit);
        panel.process_actions(&mut bus, &mut cpu);

        assert!(panel.leds().mwrt);

        panel.set_data_switches(0x00);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bool_array_to_u8(panel.leds().data), 0x3C);
        // Also verify via bus directly
        assert_eq!(bus.mem_read(0x0100), 0x3C);
    }

    #[test]
    fn test_examine_next_auto_increments() {
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
    fn test_deposit_next_auto_increments() {
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
        assert!(panel.leds().wait);

        // Press RUN/STOP to enter RUN
        panel.set_address_switches(0x0100);
        panel.press_switch(PanelSwitch::RunStop);
        let should_run = panel.process_actions(&mut bus, &mut cpu);

        assert!(panel.is_running());
        assert!(should_run);
        assert_eq!(cpu.pc, 0x0100); // PC set to address switches
        assert!(!panel.leds().wait); // WAIT LED off when running

        // Press RUN/STOP again to enter STOP
        panel.press_switch(PanelSwitch::RunStop);
        let should_run = panel.process_actions(&mut bus, &mut cpu);

        assert!(panel.is_stopped());
        assert!(!should_run);
        assert!(panel.leds().wait); // WAIT LED on when stopped
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
    fn test_address_leds_mirror_switches_in_stop_mode() {
        let mut panel = FrontPanel::new();
        panel.set_address_switches(0xBEEF);
        panel.set_data_switches(0xDE);

        // The address LEDs mirror the address switches in STOP mode (the panel
        // drives the address bus), but the data LEDs show the data bus, not the
        // data switches -- so they remain clear until an EXAMINE/DEPOSIT.
        assert_eq!(bool_array_to_u16(panel.leds().address), 0xBEEF);
        assert_eq!(bool_array_to_u8(panel.leds().data), 0x00);
    }

    #[test]
    fn test_deposit_writes_latched_address_while_data_changes() {
        // Mimics the GUI: EXAMINE latches the address; afterwards only the data
        // switches are pushed each frame (the low 8 sense switches double as
        // data). Dialing a data byte must NOT move the deposit target.
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Latch address 0x0100 via EXAMINE.
        panel.set_address_switches(0x0100);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);

        // Several "frames" pushing the data switches (0x3E) -- address holds.
        for _ in 0..3 {
            panel.set_data_switches(0x3E);
            panel.process_actions(&mut bus, &mut cpu);
        }
        assert_eq!(
            panel.address_switches(),
            0x0100,
            "address must stay latched"
        );

        // DEPOSIT writes the data byte at the latched address (not 0x013E).
        panel.press_switch(PanelSwitch::Deposit);
        panel.process_actions(&mut bus, &mut cpu);
        assert_eq!(bus.mem_read(0x0100), 0x3E);

        // DEPOSIT NEXT advances the latched address and writes again.
        panel.set_data_switches(0xC9);
        panel.press_switch(PanelSwitch::DepositNext);
        panel.process_actions(&mut bus, &mut cpu);
        assert_eq!(bus.mem_read(0x0101), 0xC9);
        assert_eq!(panel.address_switches(), 0x0101);
    }

    #[test]
    fn test_examine_sets_pc_then_single_step_advances() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Two NOPs at 0x0100.
        bus.mem_write(0x0100, 0x00);
        bus.mem_write(0x0101, 0x00);

        // EXAMINE the start address: this loads the PC.
        panel.set_address_switches(0x0100);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);
        assert_eq!(
            cpu.pc, 0x0100,
            "EXAMINE should load PC from the address switches"
        );

        // SINGLE STEP executes the NOP and advances; the panel address (and
        // address LEDs) must follow the new PC.
        panel.do_single_step(&mut bus, &mut cpu);
        assert_eq!(cpu.pc, 0x0101);
        assert_eq!(
            panel.address_switches(),
            0x0101,
            "address should follow the new PC"
        );
        assert_eq!(bool_array_to_u16(panel.leds().address), 0x0101);
    }

    #[test]
    fn test_examined_value_survives_repeated_switch_refresh() {
        // Regression: the GUI re-pushes the switch state every frame via
        // set_address_switches/set_data_switches. An EXAMINE result must
        // remain on the data LEDs across those refreshes, not get clobbered.
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        bus.mem_write(0x0040, 0x3E);

        // "Frame" 1: dial in the address, press EXAMINE.
        panel.set_address_switches(0x0040);
        panel.set_data_switches(0x40); // low byte doubles as data switches
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);
        assert_eq!(bool_array_to_u8(panel.leds().data), 0x3E);

        // Subsequent "frames" with no button press must NOT overwrite it.
        for _ in 0..5 {
            panel.set_address_switches(0x0040);
            panel.set_data_switches(0x40);
            panel.process_actions(&mut bus, &mut cpu);
            assert_eq!(bool_array_to_u8(panel.leds().data), 0x3E);
        }
    }

    #[test]
    fn test_leds_show_examined_data_not_switches() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Set data switches to 0x00, but memory has 0xFF (uninitialized)
        panel.set_data_switches(0x00);
        panel.set_address_switches(0x1234);
        panel.press_switch(PanelSwitch::Examine);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bool_array_to_u8(panel.leds().data), 0xFF);
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
    fn test_sequential_deposit_and_examine() {
        let mut panel = FrontPanel::new();
        let mut bus = crate::bus::ImsaiBus::new();
        let mut cpu = intel8080::Cpu8080::new();

        // Deposit at 0x0100, then deposit next at 0x0101
        panel.set_address_switches(0x0100);
        panel.set_data_switches(0x11);
        panel.press_switch(PanelSwitch::Deposit);
        panel.process_actions(&mut bus, &mut cpu);

        panel.set_data_switches(0x22);
        panel.press_switch(PanelSwitch::DepositNext);
        panel.process_actions(&mut bus, &mut cpu);

        assert_eq!(bus.mem_read(0x0100), 0x11);
        assert_eq!(bus.mem_read(0x0101), 0x22);
    }

    #[test]
    fn test_io_log_tracking() {
        let mut panel = FrontPanel::new();

        // Log an I/O read event
        panel.log_io_read(100, 0x01, 0x42);
        assert_eq!(panel.io_log().len(), 1);
        assert_eq!(panel.io_log()[0].address, 0x0001);
        assert_eq!(panel.io_log()[0].data, 0x42);
        assert!(panel.io_log()[0].is_io);
        assert!(!panel.io_log()[0].is_write);

        // Log an I/O write event
        panel.log_io_write(200, 0x00, 0x48);
        assert_eq!(panel.io_log().len(), 2);
        assert_eq!(panel.io_log()[1].address, 0x0000);
        assert_eq!(panel.io_log()[1].data, 0x48);
        assert!(panel.io_log()[1].is_io);
        assert!(panel.io_log()[1].is_write);
    }

    #[test]
    fn test_cycle_count() {
        let mut panel = FrontPanel::new();
        assert_eq!(panel.cycle_count(), 0);

        panel.log_io_read(500, 0x01, 0x00);
        assert_eq!(panel.cycle_count(), 500);
    }

    #[test]
    fn test_programmed_output_latches_on_io_write() {
        let mut panel = FrontPanel::new();
        // Programmed output LEDs start all-off at power-on.
        assert_eq!(bool_array_to_u8(panel.leds().programmed_output), 0x00);

        // OUT with value 0xFE (1111_1110): bit 0 is 0 (active-low = ON),
        // so programmed_output bit 0 should be lit (true) and the rest off.
        // Inverted: !0xFE = 0x01 = 0000_0001.
        panel.log_io_write(100, 0xFF, 0xFE);
        assert_eq!(bool_array_to_u8(panel.leds().programmed_output), 0x01);

        // OUT with value 0x3E (0011_1110): inverted = 0xC1.
        panel.log_io_write(200, 0x00, 0x3E);
        assert_eq!(bool_array_to_u8(panel.leds().programmed_output), 0xC1);

        // An I/O read should NOT update programmed output.
        panel.log_io_read(300, 0x01, 0xAB);
        assert_eq!(bool_array_to_u8(panel.leds().programmed_output), 0xC1);
    }

    #[test]
    fn test_programmed_output_counter_program() {
        // Load the counter program and verify programmed output LEDs update.
        // Counter program: MVI A,01 / RRC / MVI A,FE / OUT FF / ...
        // OUT FF outputs 0xFE = 1111_1110. Active-low inversion: !0xFE = 0x01.
        // So only bit 0 (D0) LED should be lit.
        let mut emu = crate::emulator::Imsai8080::new();
        let program: Vec<u8> = vec![
            0x3E, 0x01, // MVI A,01
            0x0F,       // RRC  -> A=0x80
            0x3E, 0xFE, // MVI A,FE
            0xD3, 0xFF, // OUT FF  -> outputs A=0xFE
        ];
        emu.load_program(0x0000, &program);
        emu.panel.set_address_switches(0x0000);
        emu.panel.press_switch(crate::cards::PanelSwitch::RunStop);
        emu.process_panel();

        // Run enough instructions to reach the OUT FF.
        emu.run_batch(10);
        // After OUT FF with value 0xFE, inverted = 0x01.
        assert_eq!(
            bool_array_to_u8(emu.panel.leds().programmed_output),
            0x01,
            "programmed output should show !0xFE = 0x01 after OUT FF"
        );
    }
}
