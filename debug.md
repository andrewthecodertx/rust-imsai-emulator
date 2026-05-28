# Debug Progress Notes

## Status: Root cause identified — loading disk image incorrectly

### What the PC trace reveals

After 5M instructions, the CPU is stuck in an infinite reboot loop through
empty memory (NOP sled). The trace shows:

1. WBOOT (0xFA03): initializes, JMPs to CCP at 0xE400 ✓
2. CCP (0xE400): DCR A (0→0xFF), JNZ 0xE500 (cold start) ✓
3. Cold start (0xE500): INX DE, INX HL, JMP 0xE5F8 ✓
4. 0xE5F8: NOP sled (~1K zeros in CCP BSS area)
5. 0xEA00: `JM 0xFD00` — conditional jump taken because sign flag is set
6. 0xFD00: More NOPs (uninitialized RAM) → wraps to 0x0000 → WBOOT → loop

### Root cause

**We are loading the raw disk image as a CPM.CPM relocating file, but the
image contains mixed CCP + CMI5619 BIOS data on the system tracks.**

The `CpmBios::load_and_relocate()` function copies `buf[0x100..0x900]` as "CCP"
and `buf[0x900..]` as "BDOS", applying per-segment relocation biases. But:

- The raw disk image has CCP code at offsets 0x100-0x2A0 (sectors 3-6)
- Then CMI5619 BIOS code at 0x700-0x7FF (sectors 15-16) which is NOT CCP
- Then BDOS at 0x900+ (sector 19 onward)

When we relocate 0x100-0x900 as "CCP with ORG 0x0100 and BIAS 0xE300", we
incorrectly relocate the CMI5619 BIOS code that happens to be in that range.
The bytes `FA 00 1A` at offset 0x700 (which is CMI5619 code `JM 0x1A00`)
get relocated to `JM 0xFD00`, sending the CPU into uninitialized memory.

### The fix (two options)

**Option A: Load system tracks sequentially into CPMB, like the real boot**
Read sectors 2-78 from the system tracks and write them directly to memory
starting at CPMB (0xE400), exactly as the CMI5619 boot loader does. Then
skip the CpmBios relocation entirely — the CPM.CPM system on this disk was
already configured for a 64K system with the correct base addresses. The
boot sector's cold start code handles CCP/BDOS setup.

**Option B: Properly parse the CPM.CPM format**
Only load and relocate the actual CCP (0x100-0x900) and BDOS (0x900+)
from the CPM.CPM data, but skip any non-CCP/BDOS data in between. This
requires knowing the exact boundaries, which are system-specific.

Option A is simpler and more faithful to how the real hardware works.
The disk image IS a 64K CMI5619 system — the CCP/BDOS on it are already
assembled for the correct memory layout. We just need to load the sectors
to the right addresses and let the existing cold start code handle setup.

### Additional issue: Zero page needs BDOS vector set by cold start

If we go with Option A, the cold start code at the jump target (0x012C
from the boot sector) will set up the vectors at 0x0000 and 0x0005.
But our custom BIOS at 0xFA00 needs to be in place before the CCP runs.
So the sequence would be:
1. Load system tracks to CPMB
2. Install our custom BIOS at 0xFA00
3. Set up initial WBOOT vector at 0x0000 (so we can reboot)
4. Run the cold start code which sets up BDOS vector at 0x0005
5. CCP starts and uses our BIOS

### Still to verify

- Is the CPM.CPM on this disk actually already relocated for 64K?
  (If so, no relocation is needed at all — just copy to memory.)
- Does the cold start code use ports compatible with our emulator?
  (If it uses CMI5619 ports, we need to intercept or replace it.)