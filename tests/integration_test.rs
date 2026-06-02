use rust_imsai_emulator::Imsai8080;
use rust_imsai_emulator::cards::PanelSwitch;

#[test]
fn test_front_panel_deposit_two_bytes() {
    let mut emu = Imsai8080::new();

    // Deposit 0xC3 at 0x0000, then 0x00 at 0x0001 via DepositNext
    emu.panel.set_address_switches(0x0000);
    emu.panel.set_data_switches(0xC3);
    emu.panel.press_switch(PanelSwitch::Deposit);
    emu.process_panel();

    // Verify first byte
    assert_eq!(emu.bus.mem_read(0x0000), 0xC3, "Byte at 0000 should be 0xC3");

    // Deposit 0x00 at 0x0001
    emu.panel.set_data_switches(0x00);
    emu.panel.press_switch(PanelSwitch::DepositNext);
    emu.process_panel();

    // Verify second byte
    assert_eq!(emu.bus.mem_read(0x0001), 0x00, "Byte at 0001 should be 0x00");
    assert_eq!(emu.panel.address_switches(), 0x0001, "Address should be 0x0001 after DepositNext");
}

#[test]
fn test_uart_program_via_load_program() {
    let mut emu = Imsai8080::new();

    // The UART test program
    let program: [u8; 15] = [
        0x3E, 0x4E, 0xD3, 0x01,  // MVI A,0x4E; OUT 0x01  (UART MODE)
        0x3E, 0x05, 0xD3, 0x01,  // MVI A,0x05; OUT 0x01  (UART CMD)
        0x3E, 0x41, 0xD3, 0x00,  // MVI A,0x41; OUT 0x00  (TX 'A')
        0xC3, 0x0A, 0x00,        // JMP 0x000A
    ];

    // Load via bus (bypassing front panel for now)
    emu.load_program(0x0000, &program);

    // Verify all bytes are in memory
    for (i, &expected) in program.iter().enumerate() {
        let actual = emu.bus.mem_read(i as u16);
        assert_eq!(actual, expected,
            "Byte at {:04X}: expected {:02X}, got {:02X}", i, expected, actual);
    }

    // Run the program by setting PC and starting via front panel
    emu.panel.set_address_switches(0x0000);
    emu.panel.press_switch(PanelSwitch::RunStop);
    emu.process_panel();

    assert!(emu.panel.is_running(), "Panel should be in RUN state");

    let count = emu.run_batch(10000);
    assert!(count > 0, "Should have executed instructions");

    // Stop
    emu.panel.press_switch(PanelSwitch::RunStop);
    emu.process_panel();

    // Check UART output - the 8251A needs to be programmed first via
    // OUT 0x01 (MODE then COMMAND), then characters output via OUT 0x00.
    // The program does this, so we should get 'A' characters.
    emu.bus.serial().channel_a_mut().drain_tx();
    emu.bus.serial().channel_a_mut().update_tx();
    let output = emu.bus.serial().channel_a_mut().take_output();
    assert!(!output.is_empty(), "UART should have produced output after CPU execution");
    assert!(output.iter().any(|&b| b == b'A'),
        "UART output should contain 'A', got: {:?}",
        output.iter().take(20).map(|&b| b as char).collect::<String>());
}

#[test]
fn test_front_panel_memory_initializes_to_ff() {
    let emu = Imsai8080::new();
    // All memory should be 0xFF on power-up (uninitialized bus state)
    for addr in [0x0000u16, 0x0100, 0x8000, 0xFFFF] {
        assert_eq!(emu.bus.mem_read(addr), 0xFF,
            "Memory at {:04X} should be 0xFF (floating bus)", addr);
    }
}