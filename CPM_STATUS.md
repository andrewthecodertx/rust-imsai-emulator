# IMSAI 8080 CP/M 2.2 Emulator Status

## Summary

The IMSAI 8080 emulator runs CP/M 2.2 and displays the `A>` prompt with
interactive terminal mode. The CP/M system (CCP + BDOS) is loaded from a
disk image, and our custom BIOS translates Tarbell controller I/O.

**Current status**: CP/M boots and can list a directory (DIR shows one file
name), but executing .COM programs fails with "Bdos Err On A: Bad Sector".
Investigation revealed the z80pack BDOS binary passes invalid sector numbers
(72, 232) to the BIOS SETSEC routine, indicating incompatible assumptions
about disk I/O between the z80pack BDOS and our Tarbell controller emulation.
This has been identified as a fundamental incompatibility with the z80pack
BDOS binary.

**Decision**: The z80pack CP/M 2.2 binary has been removed. The project needs
either a custom CCP+BDOS built from DRI source code, or a CP/M 2.2 system
image assembled for the Tarbell controller's I/O ports.

## What Works

- CPU emulation: Full Intel 8080 instruction set
- Memory: 64K address space with bus architecture
- Console I/O: Port 0x00 (data) and 0x01 (status) for terminal output
- Tarbell disk controller: Ports 0x48-0x4B for 8" floppy disk I/O
- Boot sequence: Loads system tracks from disk, installs custom BIOS
- BIOS: Complete 17-entry CP/M 2.2 BIOS with sector translation
- BDOS integration: CCP starts and displays the A> prompt
- Interactive terminal mode: Real-time keyboard input via crossterm
- Disk image management: Read/write for 8" SSSD format (77 tracks, 26 sectors)

## Architecture

```
Memory Map (64K):
  0x0000-0x00FF  Zero page (vectors, BDOS entry at 0x0005)
  0x0100-0xE3FF  TPA (Transient Program Area)
  0xE400-0xEFFF  CCP (Command Control Program) - loaded from disk
  0xF000-0xF9FF  BDOS - loaded from disk
  0xFA00-0xFB2F  Custom BIOS (jump table + routines)
  0xFB30-0xFB3F   DPH (Disk Parameter Header)
  0xFB40-0xFBBF   DIRBUF (directory buffer, 128 bytes)
  0xFBC0-0xFBDF   CSV (directory check vector, 32 bytes)
  0xFBE0-0xFC0F   ALV (allocation vector, 48 bytes)
  0xFC10-0xFFFF   Available

BIOS Routines:
  0xFA00: BOOT (debug marker, falls through to WBOOT)
  0xFA03: WBOOT (warm boot - reset vectors, jump to CCP)
  0xFA06: CONST (console status)
  0xFA09: CONIN (console input - blocking)
  0xFA0C: CONOUT (console output)
  0xFA0F: LIST (list output - no-op)
  0xFA12: PUNCH (punch output - same as console)
  0xFA15: READER (reader input - returns EOF)
  0xFA18: HOME (seek track 0)
  0xFA1B: SELDSK (select disk, return DPH address)
  0xFA1E: SETTRK (set track number)
  0xFA21: SETSEC (set sector number)
  0xFA24: SETDMA (set DMA address)
  0xFA27: READ (read sector into DMA buffer)
  0xFA2A: WRITE (write sector from DMA buffer)
  0xFA2D: LISTST (list status - always ready)
  0xFA30: SECTRAN (sector translation with skew)
```

## Bugs Fixed

1. **DPH scratch area overlap**: The z80pack BDOS uses 6 bytes of scratch
   space at DPH offsets 2-7. DIRBUF was at offset 6, which the BDOS
   overwrote. Fixed by moving DIRBUF to offset 8.

2. **DPH address moved from 0xFB20 to 0xFB30**: Adding DRQ fix instructions
   to the READ and WRITE BIOS routines increased code size by 4 bytes,
   causing overlap with the DPH at 0xFB20. Moved DPH and all data buffers
   to 0xFB30+.

3. **Skew table / scratch variable overlap**: CUR_TRACK overlapped
   the SECTRAN skew table. Fixed by moving scratch variables after the
   skew table.

4. **Tarbell DRQ handling**: The FD1771 status register now properly signals
   DRQ when data is available and BUSY during active operations.

5. **Sector 0 handling**: The disk image reader treats sector 0 as
   sector 1 for compatibility with BDOS implementations that pass logical
   sector 0 instead of physical sector 1.

6. **SECTRAN instruction fix**: MVI A,0x00 was being emitted where
   MVI H,0x00 was needed. Applied a runtime patch to fix this.

7. **BIOS READ/WRITE DRQ bug**: After ANI 0x02 to check DRQ, the code
   fell through to ANI 0x01 to check BUSY, but this used the already-ANDed
   accumulator value (DRQ mask instead of full status). Fixed by adding
   an explicit re-read of the status register (IN TARB_STAT) before the
   BUSY check in both READ and WRITE routines.

## Open Issues

- Need a CP/M 2.2 system image (CCP+BDOS) assembled for Tarbell controller
  I/O ports (0x48-0x4B), or need to build one from DRI source code
- The z80pack CP/M 2.2 disk image has been removed due to incompatible
  sector numbering assumptions in its BDOS
- Only drive A is functional; drives B-D are not implemented
- No CP/M program execution testing yet
- Writing to disk is implemented but untested
- No cycle-accurate timing

## Commands for Running

```bash
# Build
cargo build --release

# Interactive terminal mode (default, requires TTY):
./target/release/rust-imsai-emulator <disk_image.img>

# Scripted test (no TTY needed, captures console output):
./target/release/rust-imsai-emulator <disk_image.img> --script --cmd "DIR\r"

# Batch mode (non-interactive, 50M instructions):
./target/release/rust-imsai-emulator <disk_image.img> --batch

# Diagnostic modes:
./target/release/rust-imsai-emulator <disk_image.img> --pctrace
./target/release/rust-imsai-emulator <disk_image.img> --diag
./target/release/rust-imsai-emulator <disk_image.img> --step
```

## Terminal Mode Controls

- Keyboard input: Characters are sent to CP/M CONIN (uppercase conversion)
- Enter: Sends CR (0x0D) to CP/M
- Backspace/Delete: Sends DEL (0x7F)
- Escape: Sends ESC (0x1B)
- Ctrl+key: Sends control characters (Ctrl+C = 0x03)
- Ctrl+]: Exit the emulator