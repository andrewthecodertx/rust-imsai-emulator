# IMSAI 8080 Hardware Accuracy Plan

## Goal

Build a hardware-accurate, cycle-accurate IMSAI 8080 emulator where every
S-100 card models real silicon. Each card and chip gets its own test suite.

## Real IMSAI 8080 Configuration

A typical CP/M-ready IMSAI 8080:
- CPU: Intel 8080A @ 2MHz
- RAM: IMSAI 64K RAM board (or multiple 8K/16K boards)
- Serial: IMSAI SIO-2 board (2x Intel 8251A UART)
- Disk: Tarbell 1011 board (WD FD1771 FDC, 8" SS/SD floppy)
- Optional: IMSAI VIO video board (memory-mapped, ports 0x04-0x07)

## Chip Models (new, `src/chips/`)

### Intel 8251A UART (`chips/uart8251.rs`)
- Full register model: mode instruction, command instruction, status
- TX/RX data buffers with proper flow control
- Baud rate divisor: maps CPU cycles to serial timing
- Status bits: TxRDY, RxRDY, TxEMPTY, PE, OE, FE, SYNDET, DSR
- Error reset, internal reset, break character support
- Tests: reset state, mode/command programming, TX/RX flow, errors

### WD FD1771 FDC (`chips/fd1771.rs`)
- Register model: command, track, sector, data, status
- All 11 command types with proper state machine
- Status bits: NOT READY, WRITE PROTECT, RNF, CRC ERROR, SEEK ERROR,
  DRQ, BUSY, INDEX, LOST DATA, TRACK 00
- DRQ and INTRQ pin modeling (software polls status register)
- Seek/restore timing (not instant - modeled per track)
- Rotational delay for read/write (not instant)
- Tests: register read/write, each command type, DRQ/INTRQ signaling,
  error conditions, multi-drive select

## Card Models (`src/cards/`)

### MemoryCard (`cards/memory.rs`)
- Currently: monolithic 64K, initialized to 0xFF
- Target: configurable address ranges, proper S-100 bus decode
- Unpopulated addresses read 0xFF (floating bus)
- Tests: boundary reads, address decode, writes, uninitialized reads

### SerialCard (`cards/serial.rs`) - replaces ConsoleCard
- IMSAI SIO-2: two 8251A UART channels
- Channel A (console): ports 0x00 (data), 0x01 (cmd/status)
- Channel B (aux/list): ports 0x02 (data), 0x03 (cmd/status)
- Connected to host terminal via keyboard/video interfaces
- Tests: port decode, UART programming, TX/RX flow, status bits

### TarbellCard (`cards/tarbell.rs`) - replaces Card+io/tarbell
- Tarbell 1011: FD1771 + drive select logic + wait state generator
- Ports 0x48-0x4B: FD1771 registers (status/cmd, track, sector, data)
- Drive select via port writes
- Tests: port decode, FD1771 delegation, drive select, disk insert/eject

## Bus (`bus.rs`)

- Track total CPU cycles elapsed
- Cards can query cycle count for timing
- I/O wait state support (S-100 boards can insert wait states)
- Memory dispatch by card priority
- Tests: cycle counting, I/O dispatch, wait states

## Cycle Accuracy

CPU runs at 2MHz. One cycle = 500ns.
- Memory read/write: 3 cycles (T1-T3)
- I/O instruction: 10 cycles minimum (OUT = 10, IN = 10)
- Plus wait states from slow peripherals
- FD1771 seek: ~6ms per track = ~12,000 cycles
- FD1771 sector read: one revolution at 360 RPM = 16.67ms = ~33,333 cycles
- 8251A at 9600 baud: one char = ~10 bits = ~1.04ms = ~2,083 cycles

For now, we model DRQ/INTRQ correctly but allow instant completion
when the host polls fast enough. True cycle-accurate delays can be
added later via the cycle counter.

## Implementation Order

1. Chip models: 8251A UART, FD1771 FDC (with tests)
2. Card restructure: move cards to `src/cards/`, use chip models
3. Bus update: cycle tracking, proper dispatch
4. Integration: update main.rs, verify CP/M still boots
5. Remove CP/M software concerns from main loop (separate module)