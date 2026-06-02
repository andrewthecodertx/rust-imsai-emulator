# IMSAI 8080 Emulator

A Rust emulator for the IMSAI 8080 that boots CP/M 2.2 with a custom BIOS and Tarbell floppy controller. Includes a raylib front panel GUI with toggle switches, LEDs, and a console display.

## What It Does

Emulates the IMSAI 8080 hardware (CPU, memory, Tarbell disk controller, console I/O) and runs a CP/M 2.2 operating system from a disk image. The custom BIOS provides the 17 standard CP/M entry points, and the Tarbell FD1771 controller handles disk reads and writes.

**Current status**: Boots CP/M 2.2 to the `A>` prompt. Directory listing partially works. Program execution (.COM files) hit "Bad Sector" errors due to incompatibility between the z80pack BDOS binary and the Tarbell controller's sector numbering. A CP/M 2.2 system image assembled for the Tarbell controller is needed.

## Hardware Emulated

- Intel 8080 CPU ([rust-intel8080-emulator](https://github.com/andrewthecodertx/rust-intel8080-emulator))
- 64KB RAM
- Tarbell 1011 floppy disk controller (FD1771, 8" SSSD, ports 0x48-0x4B)
- IMSAI SIO-2 dual serial board (2x Intel 8251A UART, ports 0x00-0x03)
- IMSAI 8080 front panel (toggle switches, LEDs, function buttons)
- S-100 bus connecting all components

## Front Panel GUI

The `imsai-panel` binary provides a visual raylib front panel:

```bash
cargo run --bin imsai-panel
```

Defaults to running the UART test program (prints "A" continuously on
the console display). The program is preloaded into memory starting at
address `0x0000` before the panel comes up, so address 0 holds `0x3E`
(the `MVI A,...` opcode) rather than the floating-bus `0xFF` you'd see
on a freshly powered-on IMSAI with no software. Use `--bare` to start
with empty memory.

| Flag                     | Description                                      |
| ------------------------ | ------------------------------------------------ |
| (none)                   | Start with UART test program loaded, STOPPED    |
| `--bare`                 | Start with empty memory (all addresses = 0xFF)  |
| `--load <file> [0xADDR]` | Load raw binary file at address                  |
| `--disk <file>`          | Load disk image and boot CP/M 2.2                |
| `--program <file>`       | Load and execute a front panel program (JSON)    |

### Front Panel Programs

Programs are JSON files describing sequences of switch positions and button presses, just like operating the real front panel. Create them in the `programs/` directory.

```json
{
  "name": "UART Test",
  "description": "Prints 'A' continuously to the UART",
  "steps": [
    { "action": "deposit", "address": "0000", "data": "3E" },
    { "action": "deposit_next", "data": "4E" },
    { "action": "deposit_next", "data": "D3" },
    { "action": "deposit_next", "data": "01" },
    { "action": "run", "address": "0000" }
  ]
}
```

Step types:

| Action         | Fields            | Effect                                                   |
| -------------- | ----------------- | -------------------------------------------------------- |
| `deposit`      | `address`, `data` | Set address and data switches, press DEPOSIT             |
| `deposit_next` | `data`            | Set data switches, press DEPOSIT NEXT (auto-advances)    |
| `examine`      | `address`         | Set address switches, press EXAMINE                      |
| `examine_next` | (none)            | Press EXAMINE NEXT (auto-advances)                       |
| `run`          | `address`         | Set address switches, press RUN/STOP                     |
| `load`         | `address`, `data` | Load hex bytes directly into memory (no switch toggling) |

The `load` action is a shortcut that writes bytes via `load_program()` instead of toggling each byte through the front panel. Use it for longer programs. The other actions operate the front panel interface exactly as a human would.

```bash
# Run the UART test program (toggles each byte in via front panel)
cargo run --bin imsai-panel -- --program programs/uart-test.json

# Same program using fast load (loads all bytes at once)
cargo run --bin imsai-panel -- --program programs/uart-test-fast.json
```

### Front Panel Controls

| Control                         | Action                                              |
| ------------------------------- | --------------------------------------------------- |
| Mouse click on address switches | Toggle address bits (A15-A0)                        |
| Mouse click on data switches    | Toggle data bits (D7-D0)                            |
| RUN/STOP button or F5           | Start/stop CPU execution                            |
| STEP button                     | Execute one instruction                             |
| EXAMINE button                  | Read byte at address switches into data LEDs        |
| DEPOSIT button                  | Write data switches into memory at address switches |
| EX NXT / DEP NXT                | Increment address then examine/deposit              |
| R key                           | Reset to UART test program                          |
| Keyboard (when running)         | Send characters to console UART                     |

### Terminal Mode (CLI)

```bash
cargo run --release -- <disk_image.img>
```

Requires a CP/M 2.2 disk image (256,256 bytes, 77 tracks x 26 sectors x 128 bytes). The system tracks must contain CCP+BDOS assembled for addresses 0xE400/0xEC06 with the Tarbell controller ports.

```
rust-imsai-emulator <disk_image.img> [OPTIONS]

Options:
  (default)         Interactive terminal mode with keyboard input
  --batch, -b        Batch mode (non-interactive, 50M instructions)
  --trace, -t        Trace every instruction
  --vtrace, -v       Verbose trace (with I/O logging)
  --diag, -d         Diagnostic mode (I/O log + region tracking)
  --step, -s         Step trace (first 500 instructions)
  --pctrace, -p      PC ring-buffer trace (last 8K instructions)
  --script           Scripted mode (captures console output)
  --cmd "text"       Pre-load keyboard input for scripted testing
```

## CP/M 2.2 Memory Map

| Address | Contents                          |
| ------- | --------------------------------- |
| 0x0000  | Vectors (JMP WBOOT, JMP BDOS)     |
| 0x0100  | TPA (Transient Program Area)      |
| 0xE400  | CCP (loaded from disk image)      |
| 0xEC00  | BDOS (loaded from disk image)     |
| 0xF000  | BDOS data area                    |
| 0xFA00  | Custom BIOS (17-entry jump table) |
| 0xFB30  | DPH + DIRBUF + CSV + ALV          |

## Terminal Mode Controls

| Key       | Action                                  |
| --------- | --------------------------------------- |
| Letters   | Sent as uppercase to CP/M CONIN         |
| Enter     | Sends CR (0x0D)                         |
| Backspace | Sends DEL (0x7F)                        |
| Escape    | Sends ESC (0x1B)                        |
| Ctrl+key  | Sends control character (Ctrl+C = 0x03) |
| Ctrl+]    | Exit emulator                           |

## BIOS

The custom BIOS at 0xFA00 provides all 17 CP/M 2.2 entry points using Tarbell controller ports 0x48-0x4B and console ports 0x00-0x01. Disk parameter block matches the 8" SSSD format: 26 sectors/track, 1024-byte allocation blocks, 2 reserved tracks.

## Known Limitations

- Only drive A: is functional
- .COM program execution fails with "Bad Sector" due to BDOS sector numbering incompatibility
- Write operations to disk are implemented but untested
- Only 8" SSSD floppy format (77 tracks, 26 sectors, 128 bytes/sector)
- No cycle-accurate timing

## License

MIT, see [LICENSE](LICENSE).

## Contributing

PRs welcome. Please open an issue first for major changes.

