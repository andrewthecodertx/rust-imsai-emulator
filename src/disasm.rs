
/// Disassemble a single 8080 instruction from up to 4 bytes at the given PC.
///
/// Returns a human-readable instruction string. Unknown opcodes fall back
/// to the generic MOV r,r decode (01dddsss) or "0xNN" for truly unknown ones.
pub fn disassemble_8080(bytes: [u8; 4]) -> String {
    let op = bytes[0];
    let lo = bytes[1];
    let hi = bytes[2];
    let addr = lo as u16 | (hi as u16) << 8;

    match op {
        0x00 => "NOP".into(),
        0x01 => format!("LXI BC,0x{:04X}", addr),
        0x11 => format!("LXI DE,0x{:04X}", addr),
        0x21 => format!("LXI HL,0x{:04X}", addr),
        0x31 => format!("LXI SP,0x{:04X}", addr),
        0xC3 => format!("JMP 0x{:04X}", addr),
        0xC2 => format!("JNZ 0x{:04X}", addr),
        0xCA => format!("JZ 0x{:04X}", addr),
        0xD2 => format!("JNC 0x{:04X}", addr),
        0xDA => format!("JC 0x{:04X}", addr),
        0xE2 => format!("JPO 0x{:04X}", addr),
        0xEA => format!("JPE 0x{:04X}", addr),
        0xF2 => format!("JP 0x{:04X}", addr),
        0xFA => format!("JM 0x{:04X}", addr),
        0xCD => format!("CALL 0x{:04X}", addr),
        0xC4 => format!("CNZ 0x{:04X}", addr),
        0xCC => format!("CZ 0x{:04X}", addr),
        0xD4 => format!("CNC 0x{:04X}", addr),
        0xDC => format!("CC 0x{:04X}", addr),
        0xC9 => "RET".into(),
        0xC0 => "RNZ".into(),
        0xC8 => "RZ".into(),
        0xD0 => "RNC".into(),
        0xD8 => "RC".into(),
        0x3E => format!("MVI A,0x{:02X}", lo),
        0x06 => format!("MVI B,0x{:02X}", lo),
        0x0E => format!("MVI C,0x{:02X}", lo),
        0x16 => format!("MVI D,0x{:02X}", lo),
        0x1E => format!("MVI E,0x{:02X}", lo),
        0x26 => format!("MVI H,0x{:02X}", lo),
        0x2E => format!("MVI L,0x{:02X}", lo),
        0x36 => format!("MVI M,0x{:02X}", lo),
        0xDB => format!("IN 0x{:02X}", lo),
        0xD3 => format!("OUT 0x{:02X}", lo),
        0x7F => "MOV A,A".into(),
        0x78 => "MOV A,B".into(),
        0x79 => "MOV A,C".into(),
        0x7A => "MOV A,D".into(),
        0x7B => "MOV A,E".into(),
        0x7C => "MOV A,H".into(),
        0x7D => "MOV A,L".into(),
        0x7E => "MOV A,M".into(),
        0x47 => "MOV B,A".into(),
        0x40 => "MOV B,B".into(),
        0x41 => "MOV B,C".into(),
        0x42 => "MOV B,D".into(),
        0x43 => "MOV B,E".into(),
        0x44 => "MOV B,H".into(),
        0x45 => "MOV B,L".into(),
        0x46 => "MOV B,M".into(),
        0x4F => "MOV C,A".into(),
        0x48 => "MOV C,B".into(),
        0x49 => "MOV C,C".into(),
        0x4A => "MOV C,D".into(),
        0x4B => "MOV C,E".into(),
        0x4C => "MOV C,H".into(),
        0x4D => "MOV C,L".into(),
        0x4E => "MOV C,M".into(),
        0x57 => "MOV D,A".into(),
        0x5F => "MOV E,A".into(),
        0x67 => "MOV H,A".into(),
        0x6F => "MOV L,A".into(),
        0x77 => "MOV M,A".into(),
        0x70 => "MOV M,B".into(),
        0x71 => "MOV M,C".into(),
        0x80..=0x8F => {
            let names = ["ADD", "ADC", "SUB", "SBB", "ANA", "XRA", "ORA", "CMP"];
            let reg = op & 0x07;
            let op_name = names[((op >> 3) & 7) as usize];
            let reg_name = ["B", "C", "D", "E", "H", "L", "M", "A"][reg as usize];
            format!("{} {}", op_name, reg_name)
        }
        0x04 => "INR B".into(),
        0x0C => "INR C".into(),
        0x14 => "INR D".into(),
        0x1C => "INR E".into(),
        0x24 => "INR H".into(),
        0x2C => "INR L".into(),
        0x34 => "INR M".into(),
        0x3C => "INR A".into(),
        0x05 => "DCR B".into(),
        0x0D => "DCR C".into(),
        0x15 => "DCR D".into(),
        0x1D => "DCR E".into(),
        0x25 => "DCR H".into(),
        0x2D => "DCR L".into(),
        0x35 => "DCR M".into(),
        0x3D => "DCR A".into(),
        0x03 => "INX BC".into(),
        0x13 => "INX DE".into(),
        0x23 => "INX HL".into(),
        0x33 => "INX SP".into(),
        0x0B => "DCX BC".into(),
        0x1B => "DCX DE".into(),
        0x2B => "DCX HL".into(),
        0x3B => "DCX SP".into(),
        0x09 => "DAD BC".into(),
        0x19 => "DAD DE".into(),
        0x29 => "DAD HL".into(),
        0x39 => "DAD SP".into(),
        0xC5 => "PUSH BC".into(),
        0xD5 => "PUSH DE".into(),
        0xE5 => "PUSH HL".into(),
        0xF5 => "PUSH PSW".into(),
        0xC1 => "POP BC".into(),
        0xD1 => "POP DE".into(),
        0xE1 => "POP HL".into(),
        0xF1 => "POP PSW".into(),
        0x32 => format!("STA 0x{:04X}", addr),
        0x3A => format!("LDA 0x{:04X}", addr),
        0x22 => format!("SHLD 0x{:04X}", addr),
        0x2A => format!("LHLD 0x{:04X}", addr),
        0xEB => "XCHG".into(),
        0xE3 => "XTHL".into(),
        0xF9 => "SPHL".into(),
        0xC6 => format!("ADI 0x{:02X}", lo),
        0xCE => format!("ACI 0x{:02X}", lo),
        0xD6 => format!("SUI 0x{:02X}", lo),
        0xDE => format!("SBI 0x{:02X}", lo),
        0xE6 => format!("ANI 0x{:02X}", lo),
        0xEE => format!("XRI 0x{:02X}", lo),
        0xF6 => format!("ORI 0x{:02X}", lo),
        0xFE => format!("CPI 0x{:02X}", lo),
        0x76 => "HLT".into(),
        0xF3 => "DI".into(),
        0xFB => "EI".into(),
        0xE9 => "PCHL".into(),
        0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
            format!("RST {}", (op >> 3) & 7)
        }
        _ => {
            if op >> 6 == 0b01 {
                let dst = (op >> 3) & 7;
                let src = op & 7;
                let reg_names = ["B", "C", "D", "E", "H", "L", "M", "A"];
                format!("MOV {},{}", reg_names[dst as usize], reg_names[src as usize])
            } else {
                format!("0x{:02X}", op)
            }
        }
    }
}

/// Return a human-readable name for a known I/O port, or empty string.
pub fn port_name(port: u8) -> &'static str {
    match port {
        0x00 => "CON_DATA",
        0x01 => "CON_STAT",
        0x48 => "TARB_STAT",
        0x49 => "TARB_TRK",
        0x4A => "TARB_SEC",
        0x4B => "TARB_DATA",
        0xFE => "DEBUG",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disassemble_nop() {
        assert_eq!(disassemble_8080([0x00, 0, 0, 0]), "NOP");
    }

    #[test]
    fn test_disassemble_jmp() {
        assert_eq!(disassemble_8080([0xC3, 0x00, 0x01, 0]), "JMP 0x0100");
    }

    #[test]
    fn test_disassemble_mvi_a() {
        assert_eq!(disassemble_8080([0x3E, 0x42, 0, 0]), "MVI A,0x42");
    }

    #[test]
    fn test_disassemble_out() {
        assert_eq!(disassemble_8080([0xD3, 0x01, 0, 0]), "OUT 0x01");
    }

    #[test]
    fn test_disassemble_mov_a_b() {
        assert_eq!(disassemble_8080([0x78, 0, 0, 0]), "MOV A,B");
    }

    #[test]
    fn test_disassemble_generic_mov() {
        assert_eq!(disassemble_8080([0x51, 0, 0, 0]), "MOV D,C");
    }

    #[test]
    fn test_disassemble_unknown_opcode() {
        assert_eq!(disassemble_8080([0xDD, 0, 0, 0]), "0xDD");
    }

    #[test]
    fn test_port_name_known() {
        assert_eq!(port_name(0x00), "CON_DATA");
        assert_eq!(port_name(0x01), "CON_STAT");
        assert_eq!(port_name(0x48), "TARB_STAT");
    }

    #[test]
    fn test_port_name_unknown() {
        assert_eq!(port_name(0x10), "");
    }
}