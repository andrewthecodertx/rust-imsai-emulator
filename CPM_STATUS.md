# CP/M 2.2 Emulator Status

## Summary

The **IMSAI 8080 emulator successfully boots CP/M 2.2** and displays the `A>` prompt.

## What Works

- **CPU emulation**: Full Intel 8080 instruction set
- **Memory**: 64K address space with bus architecture
- **Console I/O**: Port 0x00 (data) and 0x01 (status) for terminal output
- **Tarbell disk controller**: Ports 0x48-0x4B for 8" floppy disk I/O
- **CP/M 2.2 boot**: Loads z80pack CP/M 2.2 system tracks, installs custom BIOS
- **BIOS**: Complete 17-entry CP/M 2.2 BIOS with:
  - Console input/output (CONIN, CONOUT, CONST)
  - Disk I/O (HOME, SELDSK, SETTRK, SETSEC, SETDMA, READ, WRITE)
  - Sector translation with 6:1 interleave skew table
  - Disk Parameter Header (DPH) with proper layout for BDOS compatibility
- **BDOS integration**: CP/M BDOS successfully reads the directory, writes to console

## Architecture

```
Memory Map (64K):
  0x0000-0x00FF  Zero page (vectors, BDOS entry at 0x0005)
  0x0100-0xE3FF  TPA (Transient Program Area)
  0xE400-0xEAFF  CCP (Command Control Program)
  0xEB00-0xEBFF   BDOS data
  0xEC00-0xEFFF   BDOS code
  0xF000-0xF9FF   BDOS data/buffers
  0xFA00-0xFB1F   Custom BIOS (jump table + routines)
  0xFB20-0xFB2F   DPH (Disk Parameter Header)
  0xFB30-0xFBAF   DIRBUF (directory buffer, 128 bytes)
  0xFBB0-0xFBCF   CSV (directory check vector, 32 bytes)
  0xFBD0-0xFBFF   ALV (allocation vector, 48 bytes)
  0xFC00-0xFFFF   Available
```

## Key Bugs Fixed

1. **DPH scratch area overlap**: The z80pack BDOS uses 6 bytes of scratch
   space at DPH offsets 2-7. Our original layout had DIRBUF at offset 6,
   which the BDOS overwrote, corrupting the directory buffer pointer.
   Fixed by moving DIRBUF to offset 8.

2. **Skew table / scratch variable overlap**: CUR_TRACK (0xF9E8) overlapped
   the SECTRAN skew table (0xF9DF-0xF9F8), corrupting sector translation.
   Fixed by moving scratch variables after the skew table.

3. **Tarbell DRQ handling**: The FD1771 status register now properly signals
   DRQ (Data Request) when sector data is available, and BUSY during active
   read/write operations.

4. **Sector 0 handling**: The disk image reader now treats sector 0 as
   sector 1 for compatibility with BDOS implementations that pass logical
   sector 0 instead of physical sector 1.

5. **SECTRAN instruction fix**: MVI A,0x00 was being emitted where MVI H,0x00
   was needed. Applied a runtime patch to fix this.

## Commands for Running

```bash
# Normal run (shows A> prompt after ~5M instructions):
./target/release/rust-imsai-emulator disk_images/cpm22-z80pack.dsk

# With diagnostics:
./target/release/rust-imsai-emulator disk_images/cpm22-z80pack.dsk --pctrace
./target/release/rust-imsai-emulator disk_images/cpm22-z80pack.dsk --diag
./target/release/rust-imsai-emulator disk_images/cpm22-z80pack.dsk --step

# Different disk images:
./target/release/rust-imsai-emulator disk_images/cpm22-boot.img
```

## Current Limitations

- No keyboard input yet ( CONST always returns "character ready")
- No TIM/interactive mode for typing commands
- The emulator runs a fixed number of instructions and stops
- Only the first disk drive (A:) is functional

## Files to Review

- **`src/bios.rs`** - CP/M 2.2 BIOS implementation (jump table + routines)
- **`src/cpm_bios.rs`** - DRI relocating-image support (for CMI5619 images)
- **`src/main.rs`** - Emulator main loop and test modes
- **`src/io/tarbell.rs`** - Tarbell disk controller with FD1771 emulation
- **`src/disk.rs`** - Disk image management (8" SSSD format)
- **`src/dpb.rs`** - Disk Parameter Block definitions
