# IMSAI 8080 Hardware Documentation

## Goal

A hardware-accurate IMSAI 8080 emulator. Every card and chip models real
silicon. The machine responds exactly as a real IMSAI would with no software
loaded: front panel switches, LEDs, and raw bus state.

## Real IMSAI 8080 Hardware

A bare IMSAI 8080 with no software is just:
- CPU card (Intel 8080A @ 2MHz)
- Memory card (RAM, initially 0xFF)
- SIO-2 serial card (2x Intel 8251A UART, unprogrammed at power-on)
- Tarbell disk controller (FD1771, no disk spinning)
- Front panel (switches + LEDs, your only interface)

There is no ROM, no BIOS, no firmware. You toggle programs in from the front
panel or boot from disk after manually keying in a bootstrap.

## Front Panel (the defining IMSAI feature)

The IMSAI front panel has:
- 16 address switches (set address to examine/deposit)
- 8 data switches (set byte to deposit)
- RUN/STOP toggle (starts/stops CPU execution)
- SINGLE STEP button (execute one instruction)
- EXAMINE button (read byte at address switches into data LEDs)
- DEPOSIT button (write data switches into RAM at address)
- EXAMINE NEXT / DEPOSIT NEXT (increment address, then examine/deposit)
- 16 address LEDs (show current address bus)
- 8 data LEDs (show current data bus)
- Status LEDs: RUN, STOPPED, M1 (machine cycle 1), WAIT, INT, HLDA

Front panel behavior:
- STOP: CPU is halted. Address/data LEDs show the bus state frozen.
  Switches control examine/deposit operations directly.
- RUN: CPU executes. LEDs show live address/data bus activity.
  Switches have no effect on examine/deposit while running.
- SINGLE STEP: CPU executes one M1 cycle, then stops. Address
  LEDs show the new PC, data LEDs show the instruction byte.

## Chip Models (`src/chips/`)

### Intel 8251A UART (`chips/uart8251.rs`)
- Full mode/command/status register model
- TX/RX data buffers with flow control
- Error detection: parity, overrun, framing
- 15 tests

### WD FD1771 FDC (`chips/fd1771.rs`)
- All 4 command types (I-IV) with state machine
- DRQ/INTRQ signaling
- Per-drive head position tracking
- 26 tests

## Card Models (`src/cards/`)

### MemoryCard (`cards/memory.rs`)
- 64K RAM, initialized to 0xFF (floating bus state)
- 5 tests

### SerialCard (`cards/serial.rs`)
- IMSAI SIO-2: two 8251A UART channels
- Ports 0x00-0x03 + aliases 0x79/0x7B
- 10 tests

### TarbellCard (`cards/tarbell.rs`)
- FD1771 + board-level port decoding
- Ports 0x48-0x4B + aliases 0xF8-0xFF
- 8 tests

### FrontPanel (`cards/front_panel.rs`)
- 16 address switches + 8 data switches
- RUN/STOP, SINGLE STEP, EXAMINE, DEPOSIT, EXAMINE NEXT, DEPOSIT NEXT
- 16 address LEDs + 8 data LEDs + status LEDs
- Direct memory access for examine/deposit (bypasses CPU)
- Single step: run CPU for one M1 cycle, then freeze
- 22 tests

## Bus (`bus.rs`)

- I/O dispatch by card port ownership
- Memory dispatch by card address ownership
- Front panel has direct bus access (not a Card trait implementor)
- Convenience methods for console/tarbell card access

## Disk Image (`disk.rs`)

- CP/M 2.2 filesystem support for IBM 3740 single-density 8" format
- 77 tracks, 26 sectors, 128 bytes/sector (256,256 bytes total)
- 6:1 interleave sector skew table (canonical, defined in `dpb.rs`)
- Read/write physical and logical sectors