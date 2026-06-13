# Programming the IMSAI 8080 Emulator

This guide explains how to write, load, and run programs on the IMSAI 8080
emulator. Programs are Intel 8080 machine code that talks directly to the
hardware through I/O ports.

For the instruction set, see the [8080 opcode table](https://pastraiser.com/cpu/i8080/i8080_opcodes.html) or the official [Intel 8080 Programmer's Manual (PDF)](https://altairclone.com/downloads/manuals/8080%20Programmers%20Manual.pdf).

## Hardware You Can Program

| Component                        | I/O Ports                          | Purpose                   |
| -------------------------------- | ---------------------------------- | ------------------------- |
| Console UART (8251A)             | 0x00 (data), 0x01 (status/command) | Terminal input and output |
| Tarbell disk controller (FD1771) | 0x48-0x4B                          | Floppy disk read/write    |
| Front panel                      | None (direct bus)                  | Toggle switches and LEDs  |

## Console I/O

The console is an Intel 8251A UART at ports 0x00 and 0x01.

### Port 0x00: Data Register

Read to receive a character. Write to send a character.

### Port 0x01: Status/Command Register

Read for status bits:

| Bit | Value | Meaning                                           |
| --- | ----- | ------------------------------------------------- |
| 0   | 0x01  | TxRDY: transmitter is ready to accept a character |
| 1   | 0x02  | RxRDY: a character is available to read           |

Important: TxRDY is bit 0 (0x01), not bit 1. Using `ANI 0x02` when you
mean `ANI 0x01` will check the wrong bit and hang until a key is pressed.

Write commands to configure and control the UART:

| Value | Command                                            |
| ----- | -------------------------------------------------- |
| 0x4E  | Mode: 8 data bits, no parity, 1 stop bit, 16x baud |
| 0x05  | Enable transmitter and receiver                    |

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
      ANI 0x01        ; E6 01 - check TX Ready bit (bit 0)
      JZ  SEND        ; CA xx xx - loop until ready
      MOV A, C        ; 79 - move character to A
      OUT 0x00        ; D3 00 - send it
      RET             ; C9
```

Note: TxRDY is **bit 0** (0x01) of the status register. A common mistake
is to use `ANI 0x02`, which checks RxRDY instead and will hang until a key
is pressed.

### Receiving a Character (Polling RX Ready)

```
RECV: IN  0x01        ; DB 01 - read UART status
      ANI 0x02        ; E6 02 - check RX Ready bit (bit 1)
      JZ  RECV        ; CA xx xx - loop until character available
      IN  0x00        ; DB 00 - read character
      ANI 0x7F        ; E6 7F - mask to 7 bits (ASCII)
      RET             ; C9
```

## Writing Programs

You have two options.

### Option 1: Front Panel JSON Programs

Create a `.json` file in the `programs/` directory. The emulator loads and
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

| Action         | Fields            | Effect                                                         |
| -------------- | ----------------- | -------------------------------------------------------------- |
| `deposit`      | `address`, `data` | Set address and data switches, press DEPOSIT                   |
| `deposit_next` | `data`            | Set data switches, press DEPOSIT NEXT (auto-advances address)  |
| `examine`      | `address`         | Set address switches, press EXAMINE                            |
| `examine_next` | none              | Press EXAMINE NEXT                                             |
| `run`          | `address`         | Set address switches, press RUN/STOP                           |
| `load`         | `address`, `data` | Write hex bytes directly into memory (fast, bypasses switches) |

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
# CLI: load binary at address 0x0000 (default)
cargo run --bin imsai-cli -- --load myprogram.bin

# CLI: load binary at a specific address
cargo run --bin imsai-cli -- --load myprogram.bin 0x100

# GUI: load binary at a specific address
cargo run --bin imsai-gui -- --load myprogram.bin 0x100
```

The `--load` flag takes a file path and an optional hex address (default 0x0000).

## Saving Programs

### From the GUI (F3)

Press F3 to save the contents of memory starting at the current program
counter. This dumps 256 bytes into a JSON program file in the `programs/`
directory named `dump_XXXX.json` where XXXX is the PC address.

### From Code

Use the `memory_to_program` function to create a program file from any
memory region. The file is saved in the JSON format described above and can
be loaded again with F2.

## Loading Programs

### From the GUI

| Key | Action                                                                        |
| --- | ----------------------------------------------------------------------------- |
| F2  | Cycle through .json programs in the programs/ directory and load the next one |
| F3  | Save current memory as a program file                                         |
| F5  | Start/stop the CPU                                                            |
| R   | Reset to UART test program                                                    |

### From the CLI

```bash
# Interactive terminal with a program
./target/release/imsai-cli --program programs/hello-world.json

# Interactive terminal with a raw binary
./target/release/imsai-cli --load myprogram.bin 0x100

# Batch mode (50M instructions, no TTY needed)
./target/release/imsai-cli --program programs/hello-world.json --batch

# Scripted test with pre-loaded keyboard input
./target/release/imsai-cli --program programs/hello-world.json --script --cmd "DIR\r"
```

## Memory Map

For bare-metal programs (no OS), you have the full 64K address space.
Start your program at 0x0000 or 0x0100 and use the console UART directly.

| Address Range | Contents                                     |
| ------------- | -------------------------------------------- |
| 0x0000-0xFFFF | 64K RAM (0xFF on power-up, floating bus)   |

The emulator initializes memory to 0xFF, matching the real IMSAI's floating
bus state. Programs should not assume memory is zeroed.

## Complete Example: Hello World

This program initializes the UART, prints "HELLO, WORLD!" with a trailing
newline, then halts the CPU. It polls TX Ready before each character:

```json
{
  "name": "Hello, World!",
  "description": "Initializes the UART and prints 'HELLO, WORLD!' with a newline, then halts. Demonstrates null-terminated strings, TX-ready polling, and HLT.",
  "steps": [
    {
      "action": "load",
      "address": "0000",
      "data": "3E 4E D3 01 3E 05 D3 01 21 35 00 DB 01 E6 01 CA 0B 00 7E FE 00 CA 1E 00 D3 00 23 C3 0B 00 DB 01 E6 01 CA 1E 00 3E 0D D3 00 DB 01 E6 01 CA 29 00 3E 0A D3 00 76 48 45 4C 4C 4F 2C 20 57 4F 52 4C 44 21 00"
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
0008  21 35 00   LXI H, MSG    ; HL = string pointer
000B  DB 01      IN 0x01        ; read UART status
000D  E6 01      ANI 0x01       ; mask TxRDY bit (bit 0)
000F  CA 0B 00   JZ 0x000B      ; loop until transmitter ready
0012  7E         MOV A,M        ; load next character
0013  FE 00      CPI 0x00       ; null terminator?
0015  CA 1E 00   JZ DONE        ; done, print newline and halt
0018  D3 00      OUT 0x00       ; send character
001A  23         INX H           ; advance string pointer
001B  C3 0B 00   JMP 0x000B      ; next character
001E  DB 01 DONE: IN 0x01       ; wait for TxRDY (CR)
0020  E6 01      ANI 0x01
0022  CA 1E 00   JZ 0x001E
0025  3E 0D      MVI A, 0x0D    ; CR
0027  D3 00      OUT 0x00
0029  DB 01      IN 0x01        ; wait for TxRDY (LF)
002B  E6 01      ANI 0x01
002D  CA 29 00   JZ 0x0029
0030  3E 0A      MVI A, 0x0A    ; LF
0032  D3 00      OUT 0x00
0034  76         HLT             ; stop CPU
0035             MSG: "HELLO, WORLD!" + NUL
```

The HLT instruction (0x76) stops the CPU. On the front panel, you will see the
RUN LED turn off and the WAIT LED may change state. This is the correct
behavior; the program has finished its work.

## Complete Example: Echo

This program reads characters from the console and echoes them back.
It waits for RxRDY (bit 1) to receive, then waits for TxRDY (bit 0) to send:

```
0000  3E 4E      MVI A, 0x4E    ; UART mode
0002  D3 01      OUT 0x01
0004  3E 05      MVI A, 0x05    ; UART command: TX+RX enable
0006  D3 01      OUT 0x01

0008  DB 01      RECV: IN 0x01  ; read status
000A  E6 02      ANI 0x02       ; RxRDY? (bit 1)
000C  CA 08 00   JZ RECV        ; loop until character received
000F  DB 00      IN 0x00        ; read character
0011  E6 7F      ANI 0x7F       ; mask to 7 bits
0013  47         MOV B, A        ; save character in B
0014  DB 01      SEND: IN 0x01  ; read status
0016  E6 01      ANI 0x01       ; TxRDY? (bit 0)
0018  CA 14 00   JZ SEND        ; loop until transmitter ready
001B  78         MOV A, B        ; restore character
001C  D3 00      OUT 0x00       ; echo it back
001E  C3 08 00   JMP RECV       ; loop forever
```

JSON version:

```json
{
  "name": "Echo",
  "description": "Reads characters from console and echoes them back. Waits for both RxRDY and TxRDY before each operation.",
  "steps": [
    {
      "action": "load",
      "address": "0000",
      "data": "3E 4E D3 01 3E 05 D3 01 DB 01 E6 02 CA 08 00 DB 00 E6 7F 47 DB 01 E6 01 CA 14 00 78 D3 00 C3 08 00"
    },
    { "action": "run", "address": "0000" }
  ]
}
```

## Disk I/O

The Tarbell floppy controller (FD1771) is accessed through ports 0x48-0x4B:

| Port  | Function                                        |
| ----- | ------------------------------------------------ |
| 0x48  | Command/status register                          |
| 0x49  | Track register                                   |
| 0x4A  | Sector register                                  |
| 0x4B  | Data register                                    |

Disk images are 256,256 bytes (77 tracks x 26 sectors x 128 bytes/sector,
IBM 3740 single-density 8" format).

## Instruction Set Quick Reference

The IMSAI 8080 uses the Intel 8080 instruction set. Common instructions:

| Hex      | Assembly     | Description                            |
| -------- | ------------ | -------------------------------------- |
| 00       | NOP          | No operation                           |
| 76       | HLT          | Halt the CPU (stop execution)          |
| 3E nn    | MVI A, nn    | Load immediate value into A            |
| 06 nn    | MVI B, nn    | Load immediate value into B            |
| 0E nn    | MVI C, nn    | Load immediate value into C            |
| 01 nn nn | LXI B, nnnn  | Load 16-bit immediate into BC          |
| 11 nn nn | LXI D, nnnn  | Load 16-bit immediate into DE          |
| 21 nn nn | LXI H, nnnn  | Load 16-bit immediate into HL          |
| 31 nn nn | LXI SP, nnnn | Load 16-bit immediate into SP          |
| 79       | MOV A, C     | Copy C to A                            |
| 7E       | MOV A, M     | Copy memory(HL) to A                   |
| 77       | MOV M, A     | Copy A to memory(HL)                   |
| 23       | INX H        | Increment HL                           |
| 13       | INX D        | Increment DE                           |
| 05       | DCR B        | Decrement B                            |
| C3 nn nn | JMP nnnn     | Unconditional jump                     |
| CA nn nn | JZ nnnn      | Jump if zero                           |
| C2 nn nn | JNZ nnnn     | Jump if not zero                       |
| CD nn nn | CALL nnnn    | Call subroutine                        |
| C9       | RET          | Return from subroutine                 |
| D3 nn    | OUT nn       | Output A to port                       |
| DB nn    | IN nn        | Input from port to A                   |
| E6 nn    | ANI nn       | AND immediate with A                   |
| FE nn    | CPI nn       | Compare immediate with A               |
| 32 nn nn | STA nnnn     | Store A to memory address              |
| 3A nn nn | LDA nnnn     | Load A from memory address             |
| 2A nn nn | LHLD nnnn    | Load HL from memory address            |
| 22 nn nn | SHLD nnnn    | Store HL to memory address             |
| F6 nn    | ORI nn       | OR immediate with A                    |
| B7       | ORA A        | OR A with A (clears carry, sets flags) |

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