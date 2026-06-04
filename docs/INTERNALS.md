# IMSAI 8080 Emulator Internals

Architecture, data flow, and hardware behavior notes for contributors.

## Bus Architecture

The S-100 bus (`bus.rs`) is a passive backplane. Cards communicate via address
lines, data lines, and control signals. The emulator's bus stores cards inline
(No `Box<dyn Card>` or dynamic dispatch):

- `MemoryCard`: 64K RAM, public field, accessed directly via `ram[addr]`
- `SerialCard`: IMSAI SIO-2, ports 0x00-0x03 + aliases 0x79/0x7B
- `TarbellCard`: floppy controller, ports 0x48-0x4B + aliases 0xF8-0xFF

Memory reads/writes index the RAM array directly (no trait dispatch, no Option
unwrap). I/O reads/writes dispatch by port number via a match statement.

### I/O Port Map

| Port(s)     | Card       | Device                            |
|-------------|------------|-----------------------------------|
| 0x00        | SerialCard | Channel A data (console in/out)   |
| 0x01        | SerialCard | Channel A command/status          |
| 0x02        | SerialCard | Channel B data                    |
| 0x03        | SerialCard | Channel B command/status          |
| 0x48-0x4B   | TarbellCard| FD1771 registers                 |
| 0x79        | SerialCard | Channel A status alias            |
| 0x7B        | SerialCard | Channel A data alias             |
| 0xF8-0xFB   | TarbellCard| FD1771 register aliases          |
| 0xFC        | TarbellCard| DRQ/wait status (bit 7)          |
| 0xFD        | TarbellCard| Fixed 0x00                        |
| 0xFF        | TarbellCard| Fixed 0x03                        |

Unclaimed ports return 0xFF on read (floating bus) and ignore writes.

## Intel 8251A UART (`chips/uart8251.rs`)

The 8251A is a programmable serial communication interface. It has a
state-machine-driven initialization sequence and separate TX/RX data paths.

### State Machine

The UART has three internal states:

1. **ExpectMode**: After reset, accepts a mode instruction byte. If the mode
   byte indicates sync mode (bits 1-0 != 00), it advances to ExpectSync.
   For async mode (the only mode we support), it advances to ExpectCommand.

2. **ExpectSync**: Not implemented. Sync mode would require one or two sync
   characters. We stay in ExpectMode (the real chip would loop reading sync
   chars, which we don't model).

3. **ExpectCommand**: After mode instruction (and optional command reset),
   accepts command bytes. A command reset (0x40) returns to ExpectMode.
   Otherwise, the command byte is applied and the UART enters Ready state.

### TX Data Flow

```
OUT port 0x00 -> write_data() -> TX buffer -> drain_tx() -> output buffer
                                                         |
                                            poll_rx() drains -> video
```

- `write_data()` stores a byte in the TX holding register if TX is enabled and
  the buffer is empty. If the buffer is full, the byte is silently dropped
  (matches real hardware overrun behavior).
- `drain_tx()` transfers one byte from the TX buffer to the output buffer and
  re-asserts TxRDY. Does NOT clear the output buffer. Use this for custom
  rendering pipelines (raylib panel).
- `poll_rx()` calls `service_uart()` which calls both `drain_tx()` and
  processes keyboard input. Use this in the main CPU execution loop.
- `take_output()` returns and clears the output buffer (for CLI scripted mode).

### RX Data Flow

```
keyboard -> type_text() -> RX buffer -> read_data() -> CPU IN port 0x00
```

- `type_text()` queues characters in the keyboard buffer (not the UART RX).
- When the UART is in Ready state (TX+RX enabled), `poll_keyboard()` transfers
  one character from the keyboard buffer to the UART RX data register.
- `read_data()` reads the RX data register and clears RxRDY. If no data is
  ready, returns 0x00 (the real 8251A would float the bus).

### Key Behavioral Notes

- **TxRDY is bit 0 (0x01), not bit 1 (0x02).** Checking the wrong bit
  (0x02) tests RxRDY instead, which causes infinite loops waiting for TX.
  This is a common serial BIOS bug.
- **Mode instruction must precede command instruction.** The 8251A ignores
  command bytes until a mode instruction sets the character length, parity,
  and stop bits.
- **Command reset (0x40) returns to ExpectMode.** This is how you reconfigure
  the UART after initial setup. The real chip needs two back-to-back 0x00
  bytes first to guarantee sync, then the 0x40 reset byte. Our model accepts
  0x40 at any point in ExpectCommand.
- **Internal reset clears mode and command settings.** After `reset()`, the
  UART returns to ExpectMode with all configuration cleared.

## WD FD1771 Floppy Controller (`chips/fd1771.rs`)

The FD1771 is a state-machine-driven floppy disk controller. It manages seek,
read, write, and track management operations.

### Command Types

The command type is determined by **bit 7** of the command byte:

| Type | Bit 7 | Commands                                      |
|------|-------|------------------------------------------------|
| I    | 0     | RESTORE, SEEK, STEP, STEP IN, STEP OUT        |
| II   | 1     | READ SECTOR, WRITE SECTOR                     |
| III  | 1     | READ ADDRESS, READ TRACK, WRITE TRACK          |
| IV   | 1     | FORCE INTERRUPT                                |

Type I commands span 0x00-0x7F. Type II-IV commands span 0x80-0xFF. The
distinction between Type II, III, and IV is determined by bits 1-4.

### Key Implementation Details

- **Type I step commands use bit 4 as the update flag.** When set, the track
  register is updated during step operations. When clear, the track register
  remains unchanged (used for verify-after-seek).
- **The FD1771 has an internal state machine** with states: Idle, Seeking,
  Reading Data, Writing Data. Commands are only accepted in Idle state. Issuing
  a command while busy is ignored (matches real hardware).
- **DRQ (Data Request) is set when the data register is ready for transfer.**
  For reads, DRQ means a sector byte is available. For writes, DRQ means the
  data register is ready for the next byte.
- **INTRQ is asserted on command completion.** The emulator tracks INTRQ as a
  boolean flag. The Tarbell board uses INTRQ and DRQ together for polling I/O.
- **Seek timing is not modeled.** The real chip delays based on stepping rate
  and track distance. Our model completes seeks instantly.
- **Sector numbering: physical sectors are 1-26, logical 0-25.** The 6:1
  interleave skew table maps logical to physical (see `dpb.rs`). The chip
  works with physical sector numbers.

## Front Panel (`cards/front_panel.rs`)

The IMSAI 8080 front panel is NOT an S-100 card. It directly accesses the
address bus, data bus, and control lines. This is what makes it useful as a
hardware debugger: it works even with no CPU, no firmware, no software.

### Bus Monitoring During RUN

When running, the front panel watches every bus transaction. It does this by
checking the instruction opcode after each CPU step:

- After every step, it reads the bus at the old PC (opcode byte) and new PC
  (next instruction byte) to update the address and data LEDs.
- For OUT (0xD3) and IN (0xDB) instructions, it logs the port and value
  to the I/O event log.

### Single Step Behavior

Single step runs the CPU for one M1 (instruction fetch) cycle. On real hardware,
this means the CPU fetches the opcode byte and then halts with WAIT asserted.
The address LEDs show the new PC (the address of the next instruction), and
the data LEDs show the opcode byte at that address.

### RUN/STOP Behavior

Pressing RUN loads the current address switches into the program counter and
starts execution. This matches real hardware: the CPU starts from whatever
address the switches are set to. Pressing STOP halts the CPU by forcing WAIT.

## Memory Card (`cards/memory.rs`)

64KB static RAM initialized to 0xFF on power-up. This matches the real IMSAI's
floating bus behavior: uninitialized RAM reads return 0xFF because no card is
driving the data lines.

The memory card owns the entire address space. On the bus, memory reads go
directly to `ram[addr]` with no dynamic dispatch.

## Disk Image Format (`disk.rs`, `dpb.rs`)

The disk image format matches the IBM 3740 single-density 8" floppy:

- 77 tracks, 26 sectors per track, 128 bytes per sector (256,256 bytes total)
- Sectors are numbered 1-26 (physical). Logical sector numbering is 0-25.
- The 6:1 interleave skew table maps logical to physical.
- Tracks 0-1 are reserved for the system (boot + OS kernel).
- The entire disk initializes to 0xE5 (standard fill byte for unused sectors).

## Tarbell Board Port Aliases

The Tarbell 1011 board mirrors FD1771 register ports at 0xF8-0xFB. Some
disk BIOS versions use these aliases. The mapping:

| Primary | Alias | Register       |
|---------|-------|----------------|
| 0x48    | 0xF8  | Status/Command |
| 0x49    | 0xF9  | Track          |
| 0x4A    | 0xFA  | Sector         |
| 0x4B    | 0xFB  | Data           |

Auxiliary ports:
- 0xFC: DRQ/wait status (bit 7 = DRQ active)
- 0xFD: Fixed 0x00 (used by some boot ROMs)
- 0xFF: Fixed 0x03 (used by some boot ROMs)