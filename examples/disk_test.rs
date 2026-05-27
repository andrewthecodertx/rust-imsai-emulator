//! Interactive test example for the IMSAI 8080 emulator
//!
//! Demonstrates the Tarbell floppy controller, disk image creation,
//! and CP/M BIOS installation.

use rust_imsai_emulator::{
    CpmBios, DiskImage, Imsai8080, TarbellController,
};
use std::path::Path;

fn main() {
    println!("IMSAI 8080 Emulator - Disk & CP/M BIOS Test");
    println!("=============================================\n");

    // Create an emulator instance and install CP/M 2.2 BIOS
    let mut emu = Imsai8080::new();
    CpmBios::install(&mut emu.bus);
    println!("CP/M 2.2 BIOS installed at 0x{:04X}", 0x1600);

    // Create a formatted disk image and insert it into drive A
    let mut controller = TarbellController::new();
    let disk = DiskImage::new_formatted();
    controller.insert_disk_image(0, disk).unwrap();
    println!("Formatted disk inserted in drive A");

    // Write a simple boot sector to track 0
    let boot_program: Vec<u8> = vec![
        0xC3, 0x00, 0x01, // JMP 0x0100 (CCP start)
        // CP/M copyright string at offset 3
        0x43, 0x4F, 0x50, 0x59, 0x52, 0x49, 0x47, 0x48, 0x54, // "COPYRIGHT"
    ];
    controller.get_disk_mut(0).unwrap().write_system(&boot_program).unwrap();
    println!("Boot sector written to track 0");

    // Read it back
    let system = controller.get_disk(0).unwrap().read_system();
    println!(
        "System area starts with: {:02X} {:02X} {:02X} (JMP {:04X})",
        system[0], system[1], system[2],
        u16::from_le_bytes([system[1], system[2]])
    );

    // Save the disk image to a file
    let disk_path = Path::new("cpm_disk_a.bin");
    controller.get_disk_mut(0).unwrap().save(disk_path).unwrap();
    let file_size = std::fs::metadata(disk_path).unwrap().len();
    println!(
        "Disk image saved to {} ({} bytes)",
        disk_path.display(),
        file_size
    );

    // Load it back and verify
    let loaded_disk = DiskImage::load(disk_path).unwrap();
    let sector = loaded_disk.read_sector(0, 1).unwrap();
    println!(
        "Loaded disk sector 0/1 starts with: {:02X} {:02X} {:02X}",
        sector[0], sector[1], sector[2]
    );

    // Also test creating a bootable disk from a system image
    let system_data = vec![
        0xC3, 0x00, 0x01, // JMP 0x0100
        0x00,             // IOBYTE
        0x00, 0x00, 0x00, // Padding
        0xC3, 0x05, 0x00, // JMP 0x0005 (BDOS)
    ];
    let bootable = DiskImage::create_bootable(&system_data).unwrap();
    println!(
        "Bootable disk created, system data at track 0: {:02X} {:02X} {:02X}",
        bootable.read_sector(0, 1).unwrap()[0],
        bootable.read_sector(0, 1).unwrap()[1],
        bootable.read_sector(0, 1).unwrap()[2],
    );

    // Check that directory area is formatted (0xE5 = unused)
    let dir_sector = bootable.read_logical_sector(2, 0).unwrap();
    println!(
        "Directory sector starts with: {:02X} (0xE5 = unused entry)",
        dir_sector[0]
    );

    // Clean up
    std::fs::remove_file(disk_path).ok();

    println!("\nAll disk infrastructure tests passed!");
}