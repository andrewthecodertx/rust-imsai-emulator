# IMSAI 8080 Emulator

A Rust-based emulator for the IMSAI 8080 microcomputer system, one of the earliest commercially successful personal computers based on the Intel 8080 CPU.

## Overview

This emulator uses the [rust-intel8080-emulator](https://github.com/andrewthecodertx/rust-intel8080-emulator) crate as its CPU core and implements the necessary hardware components to simulate the IMSAI 8080 system.

## Features

- Complete Intel 8080 CPU emulation
- 64KB RAM memory space
- Modular design for easy extension
- Written in safe, efficient Rust

## Architecture

The emulator consists of several modules:

- `emulator`: Main system coordinator
- `memory`: RAM memory system (64KB)
- `io`: Input/output controller
- `system`: System configuration and components

## Building

```bash
cargo build
```

## Running

```bash
cargo run
```

## Dependencies

- [intel8080](https://github.com/andrewthecodertx/rust-intel8080-emulator) - Intel 8080 CPU emulator

## Current Status

This is a work in progress. The basic structure is in place but full emulation of IMSAI hardware is still being developed.

## License

MIT License - see LICENSE file for details.