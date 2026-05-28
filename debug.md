# Debug Progress Notes

## Status: BIOS Fixed, CP/M boots but no console output yet

### ✅ Fixed: Jump table at 0x0000 and 0x0005
- 0x0000 = JMP 0xFA03 (BIOS WBOOT entry)
- 0x0005 = JMP 0xEC03 (BDOS function entry)
- IOBYTE and current drive properly initialized by BIOS

### ✅ Fixed: BIOS jump table at 0xFA00
- Full 17-entry CP/M 2.2 BIOS with proper routines
- Console I/O (ports 0x00/0x01), Tarbell disk I/O (0x48-0x4B)
- DPB and skew table installed at 0xF9D0

### ✅ Fixed: WBOOT initializes A=0 and zero page before jumping to CCP

### Current Issue: CPU executes CCP/BDOS code but produces no I/O

Observations from step trace:
1. CPU boots correctly: 0x0000 → WBOOT → LXI SP → JMP CCP (0xE400)
2. CCP starts executing: DCR A (A=0→0xFF), JNZ 0xE500 (cold start)
3. Cold start code at 0xE500 runs, then jumps through CCP initialization
4. CCP has substantial BSS (zero-filled) sections — the CPU slides through
   ~1K of NOPs before reaching more CCP code
5. After 50K instructions, CPU is still in CCP/BDOS area (~0xE600-0xEB00)
6. ZERO I/O instructions executed — no IN/OUT at all in 50K steps

### Root cause analysis (still investigating):

The CCP initialization should call BDOS function 13 (disk reset) and then
function 14 (select drive) to read the directory. This requires the BIOS
READ function to work with the Tarbell controller. Two possibilities:

1. **BDOS calls are not reaching our BIOS** — the relocated BDOS may have
   internal call chains that don't properly chain to the BIOS at 0xFA00.
   Need to verify that CALL 5 properly reaches BDOS function dispatcher,
   which then calls BIOS via the jump table at 0xFA00.

2. **BIOS READ is failing** — the Tarbell controller may not respond
   properly to the READ command sequence (issue RESTORE, then READ with
   status polling). The HOME/SELDSK/SETTRK/SETSEC/READ sequence may
   have a bug.

3. **Sector skew mismatch** — the BIOS SECTRAN routine uses a skew table
   to translate logical-to-physical sectors, but the disk image may store
   data in logical order already. If SECTRAN skews twice, the wrong
   sectors will be read.

4. **CCP is stuck in a loop** — the CCP may be looping waiting for
   keyboard input or disk I/O that never completes.

### Next steps:
- Add targeted debug logging to BIOS CONOUT and READ routines
- Verify CALL 5 → BDOS → BIOS chain works
- Check if BDOS function 13 (disk reset) is called
- Consider whether SECTRAN should return the sector number unchanged
  (if the image is already in logical order)