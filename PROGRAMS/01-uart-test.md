# IMSAI 8080 Front Panel Test Program

## Purpose

Verify the complete hardware chain: CPU → bus → SerialCard → 8251A UART → console output.

The program initializes the console UART, then writes the letter `A` (0x41) to port 0x00 in an infinite loop. Expected output: a continuous stream of `A` characters on the terminal.

## Program Listing

| Address | Hex | Binary | Description |
|---------|-----|--------|-------------|
| 0x0000 | 3E | 00111110 | Load accumulator |
| 0x0001 | 4E | 01001110 | ...with 0x4E (8 data bits, no parity, 1 stop bit, 16x baud) |
| 0x0002 | D3 | 11010011 | Output accumulator |
| 0x0003 | 01 | 00000001 | ...to port 0x01 (UART command/status) |
| 0x0004 | 3E | 00111110 | Load accumulator |
| 0x0005 | 05 | 00000101 | ...with 0x05 (TX enable, RX enable) |
| 0x0006 | D3 | 11010011 | Output accumulator |
| 0x0007 | 01 | 00000001 | ...to port 0x01 (UART command/status) |
| 0x0008 | 3E | 00111110 | Load accumulator |
| 0x0009 | 41 | 01000001 | ...with 0x41 (ASCII 'A') |
| 0x000A | D3 | 11010011 | Output accumulator |
| 0x000B | 00 | 00000000 | ...to port 0x00 (UART data) |
| 0x000C | C3 | 11000011 | Jump |
| 0x000D | 0A | 00001010 | ...to address 0x000A (low byte) |
| 0x000E | 00 | 00000000 | ...high byte 0x00 |

15 bytes total (0x0000 through 0x000E).

## Front Panel Deposit Procedure

Power-on state: all LEDs off except POWER and WAIT. Address and data switches at 0.

### 1. Deposit byte 0x3E at address 0x0000

```
Set address switches: 0000 0000 0000 0000  (0x0000)
Set data switches:    0011 1110            (0x3E)
Press: DEPOSIT
Verify: data LEDs show 0011 1110
```

### 2. Deposit byte 0x4E at address 0x0001

```
Set data switches:    0100 1110            (0x4E)
Press: DEPOSIT NEXT
Verify: address LEDs show 0000 0000 0000 0001, data LEDs show 0100 1110
```

### 3. Continue depositing the remaining bytes

```
Data 0xD3, press DEPOSIT NEXT  →  address 0x0002
Data 0x01, press DEPOSIT NEXT  →  address 0x0003
Data 0x3E, press DEPOSIT NEXT  →  address 0x0004
Data 0x05, press DEPOSIT NEXT  →  address 0x0005
Data 0xD3, press DEPOSIT NEXT  →  address 0x0006
Data 0x01, press DEPOSIT NEXT  →  address 0x0007
Data 0x3E, press DEPOSIT NEXT  →  address 0x0008
Data 0x41, press DEPOSIT NEXT  →  address 0x0009
Data 0xD3, press DEPOSIT NEXT  →  address 0x000A
Data 0x00, press DEPOSIT NEXT  →  address 0x000B
Data 0xC3, press DEPOSIT NEXT  →  address 0x000C
Data 0x0A, press DEPOSIT NEXT  →  address 0x000D
Data 0x00, press DEPOSIT NEXT  →  address 0x000E
```

### 4. Verify the program

```
Set address switches: 0000 0000 0000 0000  (0x0000)
Press: EXAMINE
  Data LEDs should show: 0011 1110  (0x3E)

Press: EXAMINE NEXT
  Data LEDs should show: 0100 1110  (0x4E)

Press: EXAMINE NEXT
  Data LEDs should show: 1101 0011  (0xD3)

Press: EXAMINE NEXT
  Data LEDs should show: 0000 0001  (0x01)

Continue examining through 0x000E to verify all 15 bytes.
```

### 5. Run the program

```
Set address switches: 0000 0000 0000 0000  (0x0000)
Press: RUN/STOP
```

The console should display a continuous stream of the letter `A`.

### 6. Stop the program

```
Press: RUN/STOP
```

The CPU halts. Address LEDs show the current program counter.

## What This Proves

| Step | Hardware Tested |
|------|----------------|
| DEPOSIT/EXAMINE | Front panel bus access, MemoryCard read/write |
| UART init (0x4E to port 0x01) | Bus I/O dispatch to SerialCard, 8251A MODE instruction |
| UART init (0x05 to port 0x01) | 8251A COMMAND instruction, TX/RX enable |
| OUT 0x00 (letter 'A') | 8251A TX data register, SerialCard port decode |
| JMP loop | CPU instruction execution, program counter, bus memory reads |

If you see `A` printing on the console, the entire hardware stack works.