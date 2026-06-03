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

See [INTERNALS.md](INTERNALS.md) for bus architecture, chip behavior, and
implementation details.

## Front Panel (the defining IMSAI feature)

The IMSAI front panel has:
- 16 address switches (set address to examine/deposit)
- 8 data switches (set byte to deposit)
- RUN/STOP toggle (starts/stops CPU execution)
- SINGLE STEP button (execute one instruction)
- EXAMINE button (read byte at address switches into data LEDs)
- DEPOSIT button (write data switches into memory at address)
- EXAMINE NEXT / DEPOSIT NEXT (increment address, then examine/deposit)
- 16 address LEDs (show current address bus)
- 8 data LEDs (show current data bus)
- Status LEDs: RUN, STOPPED, M1, WAIT, INT, HLDA

Front panel behavior:
- STOP: CPU is halted. Address/data LEDs show the bus state frozen.
  Switches control examine/deposit operations directly.
- RUN: CPU executes. LEDs show live address/data bus activity.
  Switches have no effect on examine/deposit while running.
- SINGLE STEP: CPU executes one M1 cycle, then stops. Address
  LEDs show the new PC, data LEDs show the instruction byte.

## Chip Models

### Intel 8251A UART

See [INTERNALS.md](INTERNALS.md#intel-8251a-uart-chipsuart8251rs) for the full
state machine, TX/RX data flow, and behavioral notes.

Key detail: **TxRDY is bit 0 (0x01), not bit 1 (0x02).** Checking the wrong
bit tests RxRDY instead, causing infinite loops.

### WD FD1771 FDC

See [INTERNALS.md](INTERNALS.md#wd-fd1771-floppy-controller-chipsfd1771rs) for
command types, state machine details, and implementation notes.

## Card Models

### MemoryCard (`cards/memory.rs`)

64K RAM, initialized to 0xFF (floating bus state). Accesses go directly to the
RAM array with no dispatch overhead.

### SerialCard (`cards/serial.rs`)

IMSAI SIO-2: two 8251A UART channels plus board-level address decoding.
See [INTERNALS.md](INTERNALS.md#serial-card) for RX/TX data flow.

### TarbellCard (`cards/tarbell.rs`)

FD1771 + board-level port decoding and auxiliary ports.
See [INTERNALS.md](INTERNALS.md#tarbell-board-port-aliases) for the full port
map including aliases and fixed-value ports.

### FrontPanel (`cards/front_panel.rs`)

Not an S-100 card. Directly accesses the bus for examine/deposit and monitors
address/data lines during RUN. See
[INTERNALS.md](INTERNALS.md#front-panel-cardsfront_panelrs) for bus monitoring.

## Bus (`bus.rs`)

See [INTERNALS.md](INTERNALS.md#bus-architecture) for the current architecture.
Cards are stored inline with direct field access for memory and match-based
dispatch for I/O ports.

## Disk Image Format (`disk.rs`, `dpb.rs`)

See [INTERNALS.md](INTERNALS.md#disk-image-format-diskrs-dpbrs) for the format
specification, skew table, and sector numbering conventions.