//! Typed operand model for ARMv7 Thumb.
//!
//! Bootstrap iteration: registers, error types, and a stub
//! `DecodedOperand` enum that grows as the opcode table covers
//! more operand kinds. The shape mirrors the AArch64 operand
//! module so future cross-referencing is easy.

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum RegisterClass {
    /// 32-bit general-purpose register r0..r15. r13 = sp,
    /// r14 = lr, r15 = pc by ARM convention.
    R,
    /// "Low" registers r0..r7 — the only ones encodable in
    /// most Thumb-1 16-bit instructions.
    Low,
    /// Single-precision VFP register s0..s31.
    S,
    /// Double-precision VFP register d0..d31.
    D,
    /// Quad register q0..q15 (Advanced SIMD).
    Q,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Register {
    pub class: RegisterClass,
    /// Encoded register number. For `Low` the valid range is
    /// 0..=7; for `R` 0..=15; for `S` 0..=31; for `D` 0..=31;
    /// for `Q` 0..=15.
    pub index: u8,
}

/// One Thumb operand. Grows as the opcode table needs more
/// kinds. Each variant captures the runtime-decoded value;
/// raw bit-positions stay private to the table/decoder.
///
/// Variants ahead of the current decoder coverage
/// (`PcRelative`, `Condition`) are present so callers can
/// pattern-match exhaustively without worrying about churn
/// when the table picks up more shapes.
#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DecodedOperand {
    /// A general-purpose register operand.
    Register(Register),
    /// An immediate operand, sign-extended to i64 if the
    /// instruction's encoding treats the field as signed.
    /// Decoders pick i64 vs u64 based on the operand kind.
    Immediate(i64),
    /// PC-relative branch target — the absolute address the
    /// branch resolves to (decoder adds the current PC and
    /// the encoded offset).
    BranchTarget(u64),
    /// PC-relative literal-pool reference (ldr Rt, =const,
    /// `ldr Rt, [pc, #offset]`). The absolute address of the
    /// referenced word.
    PcRelative(u64),
    /// Register list (push/pop, ldm/stm). Bit N set means
    /// register N is in the list.
    RegisterList(u16),
    /// Encoded condition code for `b<cond>` and IT-block
    /// instructions (0..=15; 14 is "always", 15 is reserved
    /// in pre-v8 Thumb).
    Condition(u8),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DecodeError {
    /// The input bytes ran out before a complete Thumb
    /// instruction could be read.
    TruncatedInput {
        offset: usize,
        available: usize,
        wanted: usize,
    },
    /// No opcode in the table matched the input word.
    NoMatchingOpcode { word: u32, width_bytes: usize },
    /// An operand-specific decode failed (out-of-range
    /// immediate, unsupported encoding, etc.).
    OperandDecode { reason: &'static str },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TruncatedInput {
                offset,
                available,
                wanted,
            } => write!(
                f,
                "Thumb decoder: truncated input at offset {offset} \
                 (have {available} bytes, need {wanted})",
            ),
            Self::NoMatchingOpcode { word, width_bytes } => write!(
                f,
                "Thumb decoder: no opcode matches word 0x{word:08x} ({width_bytes}-byte instruction)",
            ),
            Self::OperandDecode { reason } => write!(f, "Thumb operand decode: {reason}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EncodeError {
    UnknownMnemonic { mnemonic: &'static str },
    NoMatchingForm { mnemonic: &'static str },
    InvalidOperand { kind: &'static str },
    Unimplemented { kind: &'static str },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMnemonic { mnemonic } => {
                write!(f, "Thumb encoder: unknown mnemonic {mnemonic:?}")
            }
            Self::NoMatchingForm { mnemonic } => {
                write!(f, "Thumb encoder: no matching form for {mnemonic:?}")
            }
            Self::InvalidOperand { kind } => {
                write!(f, "Thumb encoder: invalid operand kind {kind}")
            }
            Self::Unimplemented { kind } => {
                write!(f, "Thumb encoder: unimplemented operand kind {kind}")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_class_low_subset_of_r() {
        // Sanity: Low and R are distinct enum variants but
        // semantically Low ⊂ R. Tests that depend on
        // distinguishing them downstream rely on this.
        assert_ne!(RegisterClass::Low, RegisterClass::R);
    }

    #[test]
    fn decode_error_display_includes_offset() {
        let err = DecodeError::TruncatedInput {
            offset: 0x10,
            available: 1,
            wanted: 2,
        };
        let msg = format!("{err}");
        assert!(msg.contains("offset 16"));
        assert!(msg.contains("have 1"));
    }
}
