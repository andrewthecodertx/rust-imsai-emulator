# Debug Progress Notes

## Status: CP/M 2.2 boots to A> prompt with interactive terminal

The emulator now successfully boots CP/M 2.2 and displays the A> prompt.
Interactive terminal mode with keyboard input is implemented via crossterm.

### Key issues resolved

1. **DPH scratch area overlap** (fixed): BDOS overwrites DPH offsets 2-7 as
   scratch space. DIRBUF was at offset 6, got corrupted. Moved to offset 8.

2. **Skew table / scratch variable overlap** (fixed): CUR_TRACK overlapped
   the SECTRAN skew table, corrupting sector translation. Moved variables
   after the skew table.

3. **Tarbell DRQ handling** (fixed): FD1771 status register now properly
   signals DRQ when data is available and BUSY during active operations.

4. **SECTRAN instruction fix** (fixed): MVI A,0x00 was emitted where
   MVI H,0x00 was needed. Patched at runtime.

5. **CMI5619 relocating image** (abandoned): The original CMI5619 CP/M disk
   image uses DRI's relocating format with incompatible hardware ports.
   Switched to z80pack's pre-assembled 64K system image.

### Current architecture

- z80pack CP/M 2.2 system tracks loaded verbatim into 0xE400-0xF9FF
- Custom BIOS at 0xFA00 with Tarbell controller ports (0x48-0x4B)
- Interactive terminal mode uses crossterm for raw TTY input
- Console output is intercepted from OUT 0x00/0x7B instructions
- Keyboard input is queued via the Keyboard buffer, polled by CP/M CONIN