//! Moving one game instruction that writes memory out of line: its length,
//! and the code that runs it elsewhere and then calls back into this DLL.
//! `resources` runs the game's writes to the watched page this way.

/// Length of the instruction at the start of `code` (x64 when `wide`) when
/// it writes through a memory operand and runs unchanged at another address;
/// `None` for anything else, including x64 RIP-relative operands, whose
/// address depends on where the instruction is.
pub(crate) fn relocatable_length(code: &[u8], wide: bool) -> Option<usize> {
    let byte = |at: usize| code.get(at).copied();
    let mut at = 0;
    let mut operand16 = false;
    loop {
        match byte(at)? {
            0x66 => operand16 = true,
            0xf0 | 0xf2 | 0xf3 | 0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 => {}
            // 16-bit addressing (x86) or 32-bit (x64): another ModRM table.
            0x67 => return None,
            _ => break,
        }
        at += 1;
    }
    if wide && byte(at)? & 0xf0 == 0x40 {
        at += 1;
    }
    let opcode = byte(at)?;
    at += 1;
    let full = if operand16 { 2 } else { 4 };
    // The immediate after the ModRM operand; `None` for no ModRM operand.
    let immediate = match opcode {
        0x0f => {
            let second = byte(at)?;
            at += 1;
            match second {
                0x38 => {
                    at += 1;
                    0
                }
                0x3a => {
                    at += 1;
                    1
                }
                0x70..=0x73 | 0xa4 | 0xac | 0xba | 0xc2 | 0xc4..=0xc6 => 1,
                0x05..=0x09 | 0x0b | 0x0e | 0x30..=0x37 | 0x77 | 0x80..=0x8f | 0xa0..=0xa2 | 0xa8..=0xaa | 0xc8..=0xcf => {
                    return None
                }
                _ => 0,
            }
        }
        // add, or, adc, sbb, and, sub, xor, cmp with a ModRM operand.
        0x00..=0x3f if opcode & 7 < 4 => 0,
        0x63 | 0x84..=0x8f | 0xd0..=0xd3 | 0xd8..=0xdf | 0xfe | 0xff => 0,
        0x6b | 0x80 | 0x82 | 0x83 | 0xc0 | 0xc1 | 0xc6 => 1,
        0x69 | 0x81 | 0xc7 => full,
        // test r/m, imm has the immediate; not, neg, mul, div do not.
        0xf6 | 0xf7 => match byte(at)? >> 3 & 7 {
            0 | 1 if opcode == 0xf6 => 1,
            0 | 1 => full,
            _ => 0,
        },
        // mov between the accumulator and an absolute address.
        0xa0..=0xa3 => return Some(at + if wide { 8 } else { 4 }).filter(|&end| end <= code.len()),
        // movs and stos, with or without rep.
        0xa4 | 0xa5 | 0xaa | 0xab => return Some(at),
        _ => return None,
    };
    let modrm = byte(at)?;
    at += 1;
    let (mode, rm) = (modrm >> 6, modrm & 7);
    if mode == 3 {
        return None;
    }
    if rm == 4 {
        let sib = byte(at)?;
        at += 1;
        if mode == 0 && sib & 7 == 5 {
            at += 4;
        }
    } else if mode == 0 && rm == 5 {
        if wide {
            return None;
        }
        at += 4;
    }
    at += match mode {
        1 => 1,
        2 => 4,
        _ => 0,
    };
    at += immediate;
    (at <= code.len()).then_some(at)
}

/// The shared exit every relocated instruction calls: saves the flags, the
/// registers the calling convention lets `after` change and the x87/SSE
/// state, calls `after()` (no arguments) on a 16-byte aligned stack with the
/// direction flag clear, restores everything and returns.
pub(crate) fn exit_code(after: usize, wide: bool) -> Vec<u8> {
    let mut code = Vec::with_capacity(80);
    if wide {
        // pushfq; push rax, rcx, rdx, r8-r11; push rbp; cld; mov rbp, rsp;
        // and rsp, -16; sub rsp, 0x220; fxsave [rsp+0x20]; mov rax, after
        code.extend_from_slice(&[0x9c, 0x50, 0x51, 0x52, 0x41, 0x50, 0x41, 0x51, 0x41, 0x52, 0x41, 0x53, 0x55, 0xfc]);
        code.extend_from_slice(&[0x48, 0x8b, 0xec, 0x48, 0x83, 0xe4, 0xf0, 0x48, 0x81, 0xec, 0x20, 0x02, 0x00, 0x00]);
        code.extend_from_slice(&[0x0f, 0xae, 0x44, 0x24, 0x20, 0x48, 0xb8]);
        code.extend_from_slice(&(after as u64).to_le_bytes());
        // call rax; fxrstor [rsp+0x20]; mov rsp, rbp; pop rbp;
        // pop r11-r8, rdx, rcx, rax; popfq; ret
        code.extend_from_slice(&[0xff, 0xd0, 0x0f, 0xae, 0x4c, 0x24, 0x20, 0x48, 0x8b, 0xe5, 0x5d]);
        code.extend_from_slice(&[0x41, 0x5b, 0x41, 0x5a, 0x41, 0x59, 0x41, 0x58, 0x5a, 0x59, 0x58, 0x9d, 0xc3]);
    } else {
        // pushfd; pushad; cld; mov ebp, esp; and esp, -16; sub esp, 0x200;
        // fxsave [esp]; mov eax, after
        code.extend_from_slice(&[0x9c, 0x60, 0xfc, 0x8b, 0xec, 0x83, 0xe4, 0xf0, 0x81, 0xec, 0x00, 0x02, 0x00, 0x00]);
        code.extend_from_slice(&[0x0f, 0xae, 0x04, 0x24, 0xb8]);
        code.extend_from_slice(&(after as u32).to_le_bytes());
        // call eax; fxrstor [esp]; mov esp, ebp; popad; popfd; ret
        code.extend_from_slice(&[0xff, 0xd0, 0x0f, 0xae, 0x0c, 0x24, 0x8b, 0xe5, 0x61, 0x9d, 0xc3]);
    }
    code
}

/// Code placed at `at` that runs `instruction` (relocatable), calls the
/// shared exit at `exit` and continues the game at `resume`. `exit` lies
/// within ±2 GiB of `at`.
pub(crate) fn relocated_code(instruction: &[u8], at: usize, exit: usize, resume: usize, wide: bool) -> Vec<u8> {
    let mut code = instruction.to_vec();
    let call_end = at + code.len() + 5;
    code.push(0xe8);
    code.extend_from_slice(&(exit.wrapping_sub(call_end) as u32).to_le_bytes());
    if wide {
        // jmp [rip]; the 8-byte target follows.
        code.extend_from_slice(&[0xff, 0x25, 0, 0, 0, 0]);
        code.extend_from_slice(&(resume as u64).to_le_bytes());
    } else {
        let jump_end = call_end + 5;
        code.push(0xe9);
        code.extend_from_slice(&(resume.wrapping_sub(jump_end) as u32).to_le_bytes());
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_of_the_game_writes_to_the_resource_page() {
        // x86: frame counter, the bytes and words near it, repair and refund sites.
        for (code, length) in [
            (&[0x89, 0x0a][..], 2),
            (&[0x88, 0x81, 0xf1, 0, 0, 0], 6),
            (&[0x66, 0x89, 0x88, 0xea, 0, 0, 0], 7),
            (&[0x29, 0x14, 0x81], 3),
            (&[0x29, 0x74, 0x88, 0x30], 4),
            (&[0x01, 0x0c, 0x90], 3),
            (&[0x01, 0x44, 0xb1, 0x30], 4),
            (&[0xc7, 0x05, 1, 2, 3, 4, 5, 6, 7, 8], 10),
            (&[0x66, 0xc7, 0x40, 0x10, 1, 2], 6),
            (&[0x83, 0x40, 0x30, 0x05], 4),
            (&[0xf7, 0x58, 0x30], 3),
            (&[0xf6, 0x40, 0x30, 0x01], 4),
            (&[0xa3, 1, 2, 3, 4], 5),
            (&[0xf3, 0xab], 2),
            (&[0xf0, 0x0f, 0xc1, 0x08], 4),
            (&[0x0f, 0x11, 0x84, 0x24, 1, 2, 3, 4], 8),
        ] {
            assert_eq!(relocatable_length(code, false), Some(length), "{code:02x?}");
        }
        // x64 repair and refund sites.
        for (code, length) in [
            (&[0x29, 0x3c, 0x88][..], 3),
            (&[0x29, 0x5c, 0x88, 0x30], 4),
            (&[0x01, 0x01], 2),
            (&[0x01, 0x41, 0x30], 3),
            (&[0x44, 0x01, 0x0c, 0x98], 4),
            (&[0x48, 0xc7, 0x41, 0x08, 1, 2, 3, 4], 8),
            (&[0x48, 0xa3, 1, 2, 3, 4, 5, 6, 7, 8], 10),
        ] {
            assert_eq!(relocatable_length(code, true), Some(length), "{code:02x?}");
        }
    }

    #[test]
    fn refuses_what_cannot_run_elsewhere() {
        // x64 RIP-relative, a register operand, a branch, cut short.
        assert_eq!(relocatable_length(&[0x89, 0x05, 1, 2, 3, 4], true), None);
        assert_eq!(relocatable_length(&[0x89, 0xc8], false), None);
        assert_eq!(relocatable_length(&[0x0f, 0x84, 1, 2, 3, 4], false), None);
        assert_eq!(relocatable_length(&[0x88, 0x81, 0xf1], false), None);
        assert_eq!(relocatable_length(&[0xe8, 1, 2, 3, 4], false), None);
    }

    #[test]
    fn relocated_code_calls_the_exit_and_jumps_back() {
        let code = relocated_code(&[0x89, 0x0a], 0x1000, 0x900, 0x40_1234, false);
        assert_eq!(code[2], 0xe8);
        assert_eq!(0x1000 + 7 + i32::from_le_bytes(code[3..7].try_into().unwrap()) as isize, 0x900);
        assert_eq!(code[7], 0xe9);
        assert_eq!(0x1000 + 12 + i32::from_le_bytes(code[8..12].try_into().unwrap()) as isize, 0x40_1234);
        let code = relocated_code(&[0x01, 0x01], 0x7ff0_1000, 0x7ff0_0000, 0x7ff6_0000_1234, true);
        assert_eq!(0x7ff0_1000 + 7 + i32::from_le_bytes(code[3..7].try_into().unwrap()) as i64, 0x7ff0_0000);
        assert_eq!(&code[7..13], &[0xff, 0x25, 0, 0, 0, 0]);
        assert_eq!(u64::from_le_bytes(code[13..21].try_into().unwrap()), 0x7ff6_0000_1234);
    }
}
