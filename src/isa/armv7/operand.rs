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
    /// Raw operand bits the decoder couldn't structurally
    /// model. Carries the encoded bits at their original
    /// positions within the instruction word, plus a mask
    /// covering the bits this operand owns. The encoder uses
    /// `(bits & mask) | (word & !mask)` to splice the bits
    /// back into a fresh word at encode time, guaranteeing
    /// bit-exact round-trip even for addressing-mode and
    /// shifted-register forms whose full operand grammar
    /// the typed model doesn't yet cover.
    ///
    /// Use this when an instruction's operand mixes several
    /// orthogonal pieces (P/U/W bits, shift type+amount,
    /// scaled offset variants) that the rewriter doesn't
    /// need to introspect — only preserve.
    OpaqueBits { bits: u32, mask: u32 },
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
    /// The supplied mnemonic doesn't appear in the table.
    UnknownMnemonic { mnemonic: &'static str },
    /// No row matches the given (mnemonic, operand-shape).
    /// `operand_count` is the number of operands the caller
    /// supplied; useful for diagnosing "wrong arity" errors.
    NoMatchingForm {
        mnemonic: &'static str,
        operand_count: usize,
    },
    /// The operand at `slot` doesn't fit the format-code's
    /// expected shape (e.g. an Immediate where the slot
    /// expected a Register, or a register from the wrong
    /// class).
    OperandShapeMismatch {
        slot: usize,
        format_code: &'static str,
        got: &'static str,
    },
    /// Immediate value won't fit into the field's width.
    ImmediateOutOfRange {
        slot: usize,
        value: i64,
        bits: u8,
    },
    /// Register index outside the field's encoded range
    /// (e.g. r9 in a 3-bit low-register slot).
    RegisterOutOfRange {
        slot: usize,
        index: u8,
        max: u8,
    },
    /// Branch displacement exceeds the encoding's reach.
    /// `displacement` is in bytes from the source PC.
    BranchOutOfRange {
        slot: usize,
        displacement: i64,
        bits: u8,
    },
    /// Format code recognized at decode time but not yet
    /// invertible. Caller should use a different row or wait
    /// for encoder coverage to grow.
    UnsupportedFormatCode { format_code: String },
    /// Operand count doesn't match the format string's slot
    /// count. Distinct from NoMatchingForm: this is "right
    /// row, wrong number of args."
    ArityMismatch {
        format_slots: usize,
        operands: usize,
    },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMnemonic { mnemonic } => {
                write!(f, "Thumb encoder: unknown mnemonic {mnemonic:?}")
            }
            Self::NoMatchingForm {
                mnemonic,
                operand_count,
            } => write!(
                f,
                "Thumb encoder: no matching form for {mnemonic:?} with {operand_count} operands",
            ),
            Self::OperandShapeMismatch {
                slot,
                format_code,
                got,
            } => write!(
                f,
                "Thumb encoder: slot {slot} ({format_code}) wants different operand shape, got {got}",
            ),
            Self::ImmediateOutOfRange { slot, value, bits } => write!(
                f,
                "Thumb encoder: slot {slot} immediate {value} doesn't fit in {bits} bits",
            ),
            Self::RegisterOutOfRange { slot, index, max } => write!(
                f,
                "Thumb encoder: slot {slot} register r{index} exceeds max r{max}",
            ),
            Self::BranchOutOfRange {
                slot,
                displacement,
                bits,
            } => write!(
                f,
                "Thumb encoder: slot {slot} branch displacement {displacement} exceeds {bits}-bit range",
            ),
            Self::UnsupportedFormatCode { format_code } => write!(
                f,
                "Thumb encoder: format code {format_code} not yet invertible",
            ),
            Self::ArityMismatch {
                format_slots,
                operands,
            } => write!(
                f,
                "Thumb encoder: format expects {format_slots} operands, got {operands}",
            ),
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
