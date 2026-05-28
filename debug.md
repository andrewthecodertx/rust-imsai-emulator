# Debug Progress Notes

## Status: Partial Fix Applied

The three key issues from the original debug.md have been addressed:

### ✅ Fixed: Jump table at 0x0000 and 0x0005
- 0x0000 now correctly points to JMP 0xFA03 (BIOS WBOOT entry, jump table entry 1)
- 0x0005 now correctly points to JMP 0xEC03 (BDOS function entry, BDOS+3)
- IOBYTE (0x0003) = 0x00, Current drive (0x0004) = 0x00

### ✅ Fixed: BIOS jump table at 0xFA00
- Full 17-entry CP/M 2.2 BIOS jump table installed at 0xFA00
- Each entry is a JMP to the corresponding routine implementation
- Routines include proper CONST, CONIN, CONOUT, SELDSK, SETTRK, SETSEC, SETDMA, READ, WRITE, SECTRAN
- Console I/O uses ports 0x00/0x01
- Disk I/O uses Tarbell controller ports 0x48-0x4B
- DPB and skew table installed at 0xF9D0/0xF9DF
- Scratch RAM variables at 0xF9E8-0xF9EC

### ✅ Fixed: WBOOT now initializes zero page
- A=0 (warm boot indicator)
- SP=0x0000
- IOBYTE=0, current drive=0
- Then jumps to CCP at 0xE400

## Remaining Issue: No Console Output

After boot, the CPU executes CCP/BDOS code but produces no I/O operations.

### Observed behavior:
- CPU starts at 0x0000 → JMP 0xFA03 (BIOS WBOOT) → sets A=0, SP=0 → JMP 0xE400 (CCP)
- CCP begins executing at 0xE400 (first byte is 0x3D = DCR A)
- After 200K instructions, CPU is at ~0xE748 (in BDOS/CCP area)
- No OUT 0x00 instructions executed (no console output)

### Possible causes:
1. **BDOS relocation may be incorrect** - The `CpmBios::load_and_relocate()` relocates addresses by scanning for JMP/CALL opcodes and adding bias. If it incorrectly relocates or misses some addresses, BDOS function calls may go to wrong locations.

2. **CCP expects different zero-page setup** - The CCP may need more than just IOBYTE and current drive at 0x0003/0x0004. It may depend on the BDOS drive/user code at 0x0004.

3. **Disk I/O not completing** - The BDOS calls BIOS READ to access the directory, but our READ routine may not properly interface with the Tarbell controller. If the first BDOS call (to read the directory) fails, the CCP may hang silently.

4. **CCP cold start code** - With A=0 (warm boot), the CCP skips initialization and goes straight to reading the directory. If the directory read fails, it may loop silently.

### Next steps:
- Run a step trace to see the first few hundred instructions and trace where execution goes
- Verify BDOS relocation is correct by checking specific addresses
- Check if BDOS CALL 5 dispatches correctly (does CALL 5 reach the BDOS function dispatcher?)
- Verify that the Tarbell controller properly responds to READ commands
- Consider adding more diagnostic output to the BIOS routines (e.g., marking when READ is called)