# Programming the IMSAI 8080 Emulator

This guide explains how to write, load, and run programs on the IMSAI 8080
emulator. Programs are Intel 8080 machine code that talks directly to the
hardware through I/O ports.

## Hardware You Can Program

| Component | I/O Ports | Purpose |
|-----------|-----------|---------|
| Console UART (8251A) | 0x00 (data), 0x01 (status/command) | Terminal input and output |
| Tarbell disk controller (FD1771) | 0x48-0x4B | Floppy disk read/write |
| Front panel | None (direct bus) | Toggle switches and LEDs |

## Console I/O

The console is an Intel 8251A UART at ports 0x00 and 0x01.

### Port 0x00: Data Register

Read to receive a character. Write to send a character.

### Port 0x01: Status/Command Register

Read for status bits:

| Bit | Meaning |
|-----|---------|
| 0 | RX Ready: a character is available to read |
| 1 | TX Ready: the UART is ready to accept a character |

Write commands to configure and control the UART:

| Value | Command |
|-------|---------|
| 0x4E | Mode: 8 data bits, no parity, 1 stop bit, 16x baud |
| 0x05 | Enable transmitter and receiver |

### Initializing the Console

The UART needs a mode command followed by a command before it will send or
receive data:

```
MVI A, 0x4E    ; 3E 4E - mode: 8N1, 16x baud
OUT  0x01      ; D3 01 - write to command port
MVI A, 0x05    ; 3E 05 - command: TX enable, RX enable
OUT  0x01      ; D3 01 - write to command port
```

After initialization, send characters by writing to port 0x00 and check
status on port 0x01 before each action.

### Sending a Character (Polling TX Ready)

```
; Character to send is in register C
SEND: IN  0x01        ; DB 01 - read UART status
      ANI 0x02        ; E6 02 - check TX Ready bit
      JZ  SEND        ; CA xx xx - loop until ready
      MOV A, C        ; 79 - move character to A
      OUT 0x00        ; D3 00 - send it
      RET             ; C9
```

### Receiving a Character (Polling RX Ready)

```
RECV: IN  0x01        ; DB 01 - read UART status
      ANI 0x01        ; E6 01 - check RX Ready bit
      JZ  RECV        ; CA xx xx - loop until character available
      IN  0x00        ; DB 00 - read character
      ANI 0x7F        ; E6 7F - mask to 7 bits (ASCII)
      RET             ; C9
```

### CP/M Console I/O (Calling BDOS via CALL 5)

If a CP/M system is loaded, you can use BDOS function calls instead of
talking to the UART directly. Put the function number in register C and
any parameter in register DE, then CALL 5:

| Function | C | Parameter | Returns |
|----------|---|-----------|---------|
| Console output | 2 | E = character | Nothing |
| Console input | 1 | Nothing | A = character |
| Print string | 9 | DE = string address (terminated by `$`) | Nothing |
| Read string | 10 | DE = buffer address | Buffer filled |

Example: print "HELLO" using BDOS function 9:

```
        MVI C, 9        ; BDOS function: print string
        LXI D, MSG      ; DE points to the message
        CALL 5           ; call BDOS
        JMP 0           ; warm boot back to CP/M
MSG:    DB 'HELLO$'      ; CP/M strings end with $
```

## Writing Programs

You have three options, from most manual to least.

### Option 1: Front Panel JSON Programs

Create a `.json` file in the `PROGRAMS/` directory. The emulator loads and
executes these through the front panel, exactly as a human would toggle
switches.

Each program has a `name`, a `description`, and a list of `steps`:

```json
{
  "name": "My Program",
  "description": "What it does",
  "steps": [
    { "action": "load", "address": "0000", "data": "3E 41 D3 00 C3 08 00" },
    { "action": "run", "address": "0000" }
  ]
}
```

#### Step Types

| Action | Fields | Effect |
|--------|--------|--------|
| `deposit` | `address`, `data` | Set address and data switches, press DEPOSIT |
| `deposit_next` | `data` | Set data switches, press DEPOSIT NEXT (auto-advances address) |
| `examine` | `address` | Set address switches, press EXAMINE |
| `examine_next` | none | Press EXAMINE NEXT |
| `run` | `address` | Set address switches, press RUN/STOP |
| `load` | `address`, `data` | Write hex bytes directly into memory (fast, bypasses switches) |

The `load` action is a shortcut. Rather than toggling each byte one at a time
through the switches, it writes the bytes straight to memory. Use it for
anything longer than a few bytes. The other actions operate the front panel
exactly as a human would, which is useful for testing the panel hardware itself.

Addresses and data are hex strings without a prefix: `"0000"`, `"3E"`, not
`"0x0000"` or `"0x3E"`.

#### Deposit vs Load

The `deposit` and `deposit_next` steps simulate physical switch toggling.
They are slow (each byte is a separate step) but test the full front panel
path including auto-increment of the address counter.

The `load` step writes bytes directly into memory. It is fast and should be
preferred for real programs. You can mix both in the same program:

```json
{
  "name": "Mixed Example",
  "description": "Uses load for speed, then deposit for a patch",
  "steps": [
    { "action": "load", "address": "0100", "data": "3E 41 D3 00 C3 00 01" },
    { "action": "deposit", "address": "0103", "data": "42" },
    { "action": "run", "address": "0100" }
  ]
}
```

This loads a program at 0x0100, then patches one byte at 0x0103 (changing the
output character from `A` to `B`), then runs it.

### Option 2: Raw Binary Files

Assemble your 8080 code with any assembler (z80asm, asm80, whatever produces
raw binary output). Load the binary directly:

```bash
# CLI mode: load binary at address 0x0000 (default)
cargo run --release -- <disk_image.img>

# GUI mode: load binary at a specific address
cargo run --bin imsai-panel -- --load myprogram.bin 0x100
```

The `--load` flag takes a file path and an optional hex address (default 0x0000).

### Option 3: CP/M .COM Files

If you have a working CP/M disk image, .COM files execute at address 0x0100.
Load the disk image instead:

```bash
cargo run --release -- <disk_image.img>
```

The boot sequence loads CCP and BDOS from the disk image and installs the
custom BIOS at 0xFA00. The CP/M prompt (`A>`) reads commands from the console
UART.

## Saving Programs

### From the GUI (F3)

Press F3 to save the contents of memory starting at the current program
counter. This dumps 256 bytes into a JSON program file in the `PROGRAMS/`
directory named `dump_XXXX.json` where XXXX is the PC address.

### From Code

Use the `memory_to_program` function to create a program file from any
memory region. The file is saved in the JSON format described above and can
be loaded again with F2.

## Loading Programs

### From the GUI

| Key | Action |
|-----|--------|
| F2 | Cycle through .json programs in the PROGRAMS/ directory and load the next one |
| F3 | Save current memory as a program file |
| F5 | Start/stop the CPU |
| R | Reset to UART test program |

### From the CLI

```bash
# Interactive terminal with a disk image
./target/release/rust-imsai-emulator disk.img

# Batch mode (50M instructions, no TTY needed)
./target/release/rust-imsai-emulator disk.img --batch

# Scripted test with pre-loaded keyboard input
./target/release/rust-imsai-emulator disk.img --script --cmd "DIR\r"
```

## Memory Map

| Address Range | Contents |
|---------------|----------|
| 0x0000-0x00FF | Zero page: vectors, BDOS entry at 0x0005 |
| 0x0100-0xE3FF | TPA (Transient Program Area, free for programs) |
| 0xE400-0xEFFF | CCP (loaded from disk) |
| 0xF000-0xF9FF | BDOS (loaded from disk) |
| 0xFA00-0xFB2F | Custom BIOS (jump table plus routines) |
| 0xFB30-0xFC0F | DPH, DIRBUF, CSV, ALV buffers |

For bare-metal programs (no CP/M), you have the full 64K address space.
Start your program at 0x0000 or 0x0100 and use the console UART directly.

## Complete Example: Hello World

This program initializes the UART and prints "HELLO" in a loop:

```json
{
  "name": "Hello World",
  "description": "Initializes UART and prints HELLO in a loop",
  "steps": [
    {
      "action": "load",
      "address": "0000",
      "data": "3E 4E D3 01 3E 05 D3 01 11 10 00 1A E6 7F D3 00 FE 24 CA 0A 00 13 C3 0A 00 48 45 4C 4C 4F 24"
    },
    { "action": "run", "address": "0000" }
  ]
}
```

Assembly listing:

```
0000  3E 4E      MVI A, 0x4E    ; UART mode: 8N1, 16x baud
0002  D3 01      OUT 0x01       ; write mode command
0004  3E 05      MVI A, 0x05    ; UART command: TX+RX enable
0006  D3 01      OUT 0x01       ; write command
0008  11 10 00   LXI D, MSG    ; DE = pointer to string
000B  1A         LDAX D          ; A = next character
000C  E6 7F      ANI 0x7F       ; mask to 7 bits
000E  D3 00      OUT 0x00       ; send to console
0010  FE 24      CPI '$'        ; end of string?
0012  CA 18 00   JZ RESTART     ; if so, loop back
0015  13         INX D           ; next character
0016  C3 0B 00   JMP NEXT       ; continue
0019  C3 08 00   RESTART: JMP LOOP ; restart from beginning

001C  "HELLO$"                  ; message data
```

Wait, that restart jumps to 0x0008 which re-sets DE each time through the
string, so it will print "HELLO" repeatedly. The `CPI '$'` checks for the
terminator. When found, jump back to `LOOP` (0x0008) to start over.

## Complete Example: Echo

This program reads characters from the console and echoes them back:

```
0000  3E 4E      MVI A, 0x4E    ; UART mode
0002  D3 01      OUT 0x01
0004  3E 05      MVI A, 0x05    ; UART command: TX+RX enable
0006  D3 01      OUT 0x01

0008  DB 01      WAIT: IN 0x01  ; read status
000A  E6 01      ANI 0x01       ; RX ready?
000C  CA 08 00   JZ WAIT        ; loop until character received
000F  DB 00      IN 0x00        ; read character
0011  E6 7F      ANI 0x7F       ; mask to 7 bits
0013  D3 00      OUT 0x00       ; echo it back
0015  C3 08 00   JMP WAIT       ; loop forever
```

JSON version:

```json
{
  "name": "Echo",
  "description": "Reads characters from console and echoes them back",
  "steps": [
    {
      "action": "load",
      "address": "0000",
      "data": "3E 4E D3 01 3E 05 D3 01 DB 01 E6 01 CA 08 00 DB 00 E6 7F D3 00 C3 08 00"
    },
    { "action": "run", "address": "0000" }
  ]
}
```

## Instruction Set Quick Reference

The IMSAI 8080 uses the Intel 8080 instruction set. Common instructions:

| Hex | Assembly | Description |
|-----|----------|-------------|
| 00 | NOP | No operation |
| 3E nn | MVI A, nn | Load immediate value into A |
| 06 nn | MVI B, nn | Load immediate value into B |
| 0E nn | MVI C, nn | Load immediate value into C |
| 01 nn nn | LXI B, nnnn | Load 16-bit immediate into BC |
| 11 nn nn | LXI D, nnnn | Load 16-bit immediate into DE |
| 21 nn nn | LXI H, nnnn | Load 16-bit immediate into HL |
| 31 nn nn | LXI SP, nnnn | Load 16-bit immediate into SP |
| 79 | MOV A, C | Copy C to A |
| 7E | MOV A, M | Copy memory(HL) to A |
| 77 | MOV M, A | Copy A to memory(HL) |
| 23 | INX H | Increment HL |
| 13 | INX D | Increment DE |
| 05 | DCR B | Decrement B |
| C3 nn nn | JMP nnnn | Unconditional jump |
| CA nn nn | JZ nnnn | Jump if zero |
| C2 nn nn | JNZ nnnn | Jump if not zero |
| CD nn nn | CALL nnnn | Call subroutine |
| C9 | RET | Return from subroutine |
| D3 nn | OUT nn | Output A to port |
| DB nn | IN nn | Input from port to A |
| E6 nn | ANI nn | AND immediate with A |
| FE nn | CPI nn | Compare immediate with A |
| 32 nn nn | STA nnnn | Store A to memory address |
| 3A nn nn | LDA nnnn | Load A from memory address |
| 2A nn nn | LHLD nnnn | Load HL from memory address |
| 22 nn nn | SHLD nnnn | Store HL to memory address |
| F6 nn | ORI nn | OR immediate with A |
| B7 | ORA A | OR A with A (clears carry, sets flags) |

## Debugging Tips

1. **Use EXAMINE to verify** your program in memory before running. Set the
   address switches and press EXAMINE, then EXAMINE NEXT to walk through
   each byte.

2. **Use F3 to dump** the current memory region to a JSON file you can
   inspect outside the emulator.

3. **Start simple.** The UART test (just printing `A` in a loop) is 15 bytes.
   Get that working before trying more complex programs.

4. **The UART needs initialization.** Programs that skip the 0x4E/0x05
   initialization will not produce output. Always include those four bytes
   at the start of any program that uses console I/O.

5. **Check your jumps.** 8080 jumps are absolute addresses in little-endian
   format: `JMP 0x0100` assembles as `C3 00 01`, not `C3 01 00`.