# IMSAI 8080 Emulator

A Rust emulator for the IMSAI 8080 that boots and runs CP/M 2.2 with interactive terminal support.

## What It Does

Loads a CP/M 2.2 disk image, installs a custom BIOS, and runs the operating system with console output, disk I/O, keyboard input, and the `A>` prompt.

## Hardware Emulated

- Intel 8080 CPU ([rust-intel8080-emulator](https://github.com/andrewthecodertx/rust-intel8080-emulator))
- 64KB RAM
- Tarbell 1011 floppy disk controller (FD1771, 8" SSSD, ports 0x48-0x4B)
- IMSAI VIO video board (80x24 character display, ports 0x00-0x01)
- S-100 bus connecting all components

## CP/M 2.2 Memory Map

| Address  | Contents                              |
|----------|----------------------------------------|
| 0x0000   | Vectors (JMP WBOOT, JMP BDOS)         |
| 0x0100   | TPA (Transient Program Area)           |
| 0xE400   | CCP (from z80pack disk image)          |
| 0xEC00   | BDOS (from z80pack disk image)         |
| 0xF000   | BDOS data area                         |
| 0xFA00   | Custom BIOS (17-entry jump table)     |
| 0xFB20   | DPH + DIRBUF + CSV + ALV              |

## Quick Start

```bash
cargo build --release
./target/release/rust-imsai-emulator disk_images/cpm22-z80pack.dsk
```

This boots CP/M 2.2 in interactive terminal mode. You can type commands at the `A>` prompt.

## Command-Line Options

```
rust-imsai-emulator [DISK_IMAGE] [OPTIONS]

Options:
  (default)         Interactive terminal mode with keyboard input
  --batch, -b        Batch mode (non-interactive, 50M instructions)
  --trace, -t        Trace every instruction
  --vtrace, -v       Verbose trace (with I/O logging)
  --diag, -d         Diagnostic mode (I/O log + region tracking)
  --step, -s         Step trace (first 500 instructions)
  --pctrace, -p      PC ring-buffer trace (last 8K instructions)
  --hybrid           Full-speed run with periodic display flush
```

## Terminal Mode Controls

| Key          | Action                                    |
|-------------|-------------------------------------------|
| Letters      | Sent as uppercase to CP/M CONIN           |
| Enter        | Sends CR (0x0D)                           |
| Backspace    | Sends DEL (0x7F)                          |
| Escape       | Sends ESC (0x1B)                          |
| Ctrl+key     | Sends control character (Ctrl+C = 0x03)   |
| Ctrl+]       | Exit emulator                              |

## Disk Images

Use any 256,256-byte z80pack-format CP/M 2.2 8" SSSD image (77 tracks x 26 sectors x 128 bytes). The `disk_images/` directory contains test images.

## BIOS

The custom BIOS at 0xFA00 provides all 17 CP/M 2.2 entry points:

| #  | Entry    | Function                         |
|----|----------|----------------------------------|
| 0  | BOOT     | Cold start (outputs debug 0xFE) |
| 1  | WBOOT     | Warm start (reinitializes, jumps to CCP) |
| 2  | CONST    | Console status (checks port 0x01) |
| 3  | CONIN    | Console input (blocking, reads port 0x00) |
| 4  | CONOUT   | Console output (port 0x00)       |
| 5  | LIST     | List device (no-op)              |
| 6  | PUNCH    | Punch device (console)           |
| 7  | READER   | Reader (returns EOF)              |
| 8  | HOME     | Seek track 0                     |
| 9  | SELDSK   | Select disk, return DPH pointer   |
| 10 | SETTRK   | Set track number                 |
| 11 | SETSEC   | Set sector number                |
| 12 | SETDMA   | Set DMA address                  |
| 13 | READ     | Read sector into DMA buffer      |
| 14 | WRITE    | Write sector from DMA buffer      |
| 15 | LISTST   | List status (always ready)       |
| 16 | SECTRAN  | Sector translation with skew     |

## Known Limitations

- Only drive A: is functional
- Write operations to disk are implemented but untested
- Only 8" SSSD floppy format (77 tracks, 26 sectors, 128 bytes/sector)
- Emulator runs at full speed without cycle-accurate timing

## License

MIT, see [LICENSE](LICENSE).

## Contributing

PRs welcome. Please open an issue first for major changes.