# IMSAI 8080 Emulator

A Rust emulator for the IMSAI 8080 with a Tarbell FD1771 floppy controller and raylib front panel GUI (toggle switches, LEDs, console display).

![IMSAI 8080 front panel](docs/imsai-screenshot.png)

## Downloads

Pre-built binaries for Linux, macOS, and Windows are available on the [releases page](https://github.com/andrewthecodertx/rust-imsai-emulator/releases/latest). Each archive contains both `imsai-cli` (terminal mode) and `imsai-gui` (front panel).

## What It Does

Emulates the IMSAI 8080 hardware: Intel 8080 CPU, 64KB RAM, Tarbell FD1771 floppy disk controller, and IMSAI SIO-2 dual serial board. Supports interactive terminal mode (CLI) and a visual front panel GUI.

**Current status**: Boots and runs programs loaded via `--load` or `--program`. Terminal mode provides interactive keyboard input and console output. Disk images can be mounted and the FD1771 controller is modeled, but there is no disk boot loader yet.

## Hardware Emulated

- Intel 8080 CPU ([rust-intel8080-emulator](https://github.com/andrewthecodertx/rust-intel8080-emulator), [instruction set reference](https://www.paulf.demon.nl/8080/))
- 64KB RAM (0xFF on power-up, matching floating bus behavior)
- Tarbell 1011 floppy disk controller (FD1771, 8" SSSD, ports 0x48-0x4B)
- IMSAI SIO-2 dual serial board (2x Intel 8251A UART, ports 0x00-0x03)
- 80x24 CRT video display (Channel A UART output)
- IMSAI 8080 front panel (toggle switches, LEDs, function buttons)
- S-100 bus connecting all components

## Hardware Reference

### Memory Map

The full 64K address space is RAM. There is no ROM, no BIOS, no firmware.

| Address Range | Contents |
| ------------- | -------- |
| 0x0000-0xFFFF | 64K RAM (0xFF on power-up, floating bus) |

Memory initializes to 0xFF, matching the real IMSAI's floating bus. Programs should not assume memory is zeroed.

### I/O Port Map

The S-100 bus dispatches I/O by port number. Unclaimed ports return 0xFF on read and ignore writes.

| Port(s) | Card | Register |
| ------- | ---- | -------- |
| 0x00 | SIO-2 | Channel A data (console in/out) |
| 0x01 | SIO-2 | Channel A status/command |
| 0x02 | SIO-2 | Channel B data |
| 0x03 | SIO-2 | Channel B command/status |
| 0x48 | Tarbell | FD1771 status (read) / command (write) |
| 0x49 | Tarbell | FD1771 track register |
| 0x4A | Tarbell | FD1771 sector register |
| 0x4B | Tarbell | FD1771 data register |
| 0x79 | SIO-2 | Channel A status alias |
| 0x7B | SIO-2 | Channel A data alias |
| 0xF8 | Tarbell | FD1771 status/command alias |
| 0xF9 | Tarbell | FD1771 track register alias |
| 0xFA | Tarbell | FD1771 sector register alias |
| 0xFB | Tarbell | FD1771 data register alias |
| 0xFC | Tarbell | DRQ/wait status (bit 7) |
| 0xFD | Tarbell | Fixed 0x00 |
| 0xFF | Tarbell | Fixed 0x03 |

### Console UART (Intel 8251A)

The console is an Intel 8251A UART at ports 0x00 and 0x01. It must be initialized before use.

**Port 0x00 — Data**: Read to receive a character, write to send one.

**Port 0x01 — Status/Command**: Read for status, write for mode/command bytes.

Status bits:

| Bit | Value | Meaning |
| --- | ----- | ------- |
| 0 | 0x01 | TxRDY — transmitter ready to accept a character |
| 1 | 0x02 | RxRDY — a character is available to read |

**TxRDY is bit 0 (0x01), not bit 1.** Using `ANI 0x02` when you mean `ANI 0x01` checks the wrong flag and hangs.

Initialization sequence:

```
MVI A, 0x4E    ; mode: 8 data bits, no parity, 1 stop bit, 16x baud
OUT  0x01      ;
MVI A, 0x05    ; command: enable TX and RX
OUT  0x01      ;
```

### Video Display (80x24 CRT Terminal)

Characters written to UART Channel A (OUT port 0x00) appear on the video display. The display is an 80-column by 24-row CRT terminal — it is the output side of the console UART, not a separate I/O device.

| Feature | Value |
| ------- | ----- |
| Columns | 80 |
| Rows | 24 |
| Input | Channel A TX output (port 0x00 writes) |
| Control characters | CR (0x0D), LF (0x0A), BS (0x08) |
| Scrolling | Automatic when cursor passes the last row |

The display supports CR (carriage return), LF (line feed), and BS (backspace). Lines wrap at column 80 and scroll the screen up when the cursor passes row 24. No escape sequences or cursor addressing are supported.

### Floppy Controller (WD FD1771)

The Tarbell 1011 board wraps the FD1771 at ports 0x48-0x4B (aliases at 0xF8-0xFB). Some disk BIOS versions use the aliases.

| Offset | Read | Write |
| ------ | ---- | ----- |
| +0 (0x48) | Status | Command |
| +1 (0x49) | Track register | Track register |
| +2 (0x4A) | Sector register | Sector register |
| +3 (0x4B) | Data register | Data register |

Disk format: 8" SSSD, 77 tracks, 26 sectors per track, 128 bytes per sector (256,256 bytes total). Sectors are numbered 1-26 (physical). Tracks 0-1 are reserved for the system.

Port 0xFC bit 7 is DRQ (data request). Port 0xFD always reads 0x00. Port 0xFF always reads 0x03.

### Front Panel

The front panel is **not on the I/O bus**. It directly accesses the address bus, data bus, and CPU control lines — it works even with no software loaded.

**LEDs:**

| LED | Meaning |
| --- | ------- |
| ADDRESS (16) | Current address bus value |
| DATA (8) | Current data bus value |
| PROGRAMMED OUTPUT (8) | Latched from data bus on every OUT instruction (active-low on real hardware, inverted in emulator so 1 = LED on) |
| RUN | CPU is executing |
| WAIT | CPU is in wait state |
| M1 | Instruction fetch cycle |
| HLTA | CPU halted (HLT instruction) |
| INT | Interrupt pending |
| HLDA | Hold acknowledge |
| MEMR | Memory read active |
| WO (!MWRT) | Write-output (lit when NOT writing) |
| IOR | I/O read active |
| IOW | I/O write active |
| POWER | System powered on |

**Switches:**

| Switch | Function |
| ------ | -------- |
| ADDRESS (16) | Set address for examine/deposit/RUN |
| DATA (8) | Set byte for deposit |
| RUN/STOP | Toggle CPU execution |
| SINGLE STEP | Execute one instruction then halt |
| EXAMINE | Read byte at address switches into data LEDs |
| DEPOSIT | Write data switches into memory at address switches |
| EX NXT | Increment address then examine |
| DEP NXT | Increment address then deposit |

## Front Panel GUI

The `imsai-gui` binary provides a visual raylib front panel. It needs the `gui`
feature (raylib is an optional dependency, so the terminal CLI builds without
the raylib C library):

```bash
cargo run --features gui --bin imsai-gui
```

Defaults to empty memory (all addresses = 0xFF), matching a powered-on IMSAI with no software. Use the program loader (F2) or command-line flags to load software before pressing F5 to run.

Memory state is automatically saved to `imsai_memory.json` on exit and restored on the next launch. Press R to cold-reset (clears memory and deletes the saved state).

| Flag                       | Description                                      |
| -------------------------- | ------------------------------------------------ |
| (none)                     | Start with empty memory, STOPPED                 |
| `--load <file> [0xADDR]`  | Load raw binary file at address (default 0x0000) |
| `--disk <file>`            | Mount disk image in drive A                      |
| `--program <file>`         | Load and execute a front panel program (.json)   |

### Front Panel Programs

Programs are JSON files describing sequences of switch positions and button presses, just like operating the real front panel. Create them in the `programs/` directory.

```json
{
  "name": "Counter",
  "description": "counts forever on the front panel",
  "steps": [
    {
      "action": "load",
      "address": "0000",
      "data": "3e 01 0f 3e fe d3 ff 11 01 00 21 00 00 19 d2 0d 00 07 c3 05 00"
    }
  ]
}
```

Step types:

| Action         | Fields            | Effect                                                   |
| -------------- | ----------------- | -------------------------------------------------------- |
| `load`         | `address`, `data` | Load hex bytes directly into memory (recommended)        |
| `deposit`      | `address`, `data` | Set address and data switches, press DEPOSIT             |
| `deposit_next` | `data`            | Set data switches, press DEPOSIT NEXT (auto-advances)    |
| `examine`      | `address`         | Set address switches, press EXAMINE                      |
| `examine_next` | (none)            | Press EXAMINE NEXT (auto-advances)                       |
| `run`          | `address`         | Set address switches, press RUN/STOP                     |

The `load` action writes bytes directly into memory without toggling switches — use it for everything. The other actions operate the front panel interface exactly as a human would, which is useful for testing hardware-level interactions.

```bash
# Run the UART test program
cargo run --features gui --bin imsai-gui -- --program programs/uart-test.json

# Run the Hello World program
cargo run --features gui --bin imsai-gui -- --program programs/hello-world.json
```

### Front Panel Controls

| Control                         | Action                                                |
| ------------------------------- | ----------------------------------------------------- |
| Mouse click on address switches | Toggle address bits (A15-A0)                          |
| Mouse click on data switches    | Toggle data bits (D7-D0)                              |
| RUN/STOP button or F5           | Start/stop CPU execution                              |
| STEP button                     | Execute one instruction                               |
| EXAMINE button                  | Read byte at address switches into data LEDs          |
| DEPOSIT button                  | Write data switches into memory at address switches   |
| EX NXT / DEP NXT                | Increment address then examine/deposit                |
| F2                              | Open program loader (pick a `.json` from `programs/`) |
| F3                              | Save current memory region as a front panel program   |
| F4                              | Mount a disk image in drive A                         |
| R key                           | Cold reset (clear RAM, delete `imsai_memory.json`)    |
| Keyboard (when running)         | Send characters to console UART                       |

### Terminal Mode (CLI)

```bash
# From a release binary
./imsai-cli --program programs/hello-world.json

# From source
cargo run --bin imsai-cli -- --program programs/hello-world.json
```

Interactive terminal mode with keyboard input and live console output. Memory state is persisted between sessions via `imsai_memory.json`.

```
imsai-cli [OPTIONS]

Mode (choose one):
  --load <file> [addr]       Load raw binary at address (default 0x0000)
  --program <file.json>      Load a front panel program (.json)
  (no arguments)             Start with saved memory (or empty if first run)

Options:
  --disk <file>              Mount disk image in drive A
  --batch, -b                Batch mode (non-interactive, 50M instructions)
  --trace, -t                Trace every instruction
  --vtrace, -v               Verbose trace (with I/O logging)
  --diag, -d                 Diagnostic mode (I/O log + region tracking)
  --step, -s                 Step trace (first 500 instructions)
  --pctrace, -p              PC ring-buffer trace (last 8K instructions)
  --script                   Scripted mode (captures console output)
  --cmd "text"               Pre-load keyboard input for scripted testing
  --speed <mhz>              Throttle the TUI to a target clock (default: host speed)
  --help, -h                 Show this help
```

## Terminal Controls

| Key       | Action                                                         |
| --------- | -------------------------------------------------------------- |
| Letters   | Sent as uppercase                                              |
| Enter     | Sends CR (0x0D)                                                |
| Backspace | Sends DEL (0x7F)                                               |
| Tab       | Sends TAB (0x09)                                               |
| Escape    | Sends ESC (0x1B)                                               |
| Ctrl+key  | Sends control character (Ctrl+C = 0x03)                        |
| F5        | Start/stop CPU execution                                       |
| Ctrl+K    | Command mode (load, program, mount, go/run, reset, quit, help) |
| Ctrl+D    | Exit emulator                                                  |

## Building from Source

Requires Rust 1.80+ and the raylib development libraries (for the GUI).

| Platform       | Install                                            |
| -------------- | -------------------------------------------------- |
| Ubuntu/Debian  | `sudo apt install libraylib-dev`                   |
| Arch Linux     | `sudo pacman -S raylib`                            |
| macOS          | `brew install raylib`                              |

## Known Limitations

- Only 8" SSSD floppy format (77 tracks, 26 sectors, 128 bytes/sector)
- No cycle-accurate timing
- Serial I/O polling only (no interrupt-driven input)
- No disk boot loader yet (disks mount and the FD1771 is modeled, but the machine cannot boot from disk)

## License

MIT, see [LICENSE](LICENSE).

## Contributing

PRs welcome. Please open an issue first for major changes.