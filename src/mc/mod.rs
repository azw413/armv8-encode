//! Architecture-neutral machine-code model.
//!
//! This layer will hold decoded instructions, symbols, relocations, sections,
//! and basic blocks independently of any single ISA.

/// Stable identifier for a basic block in a machine-code function.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct BasicBlockId(pub usize);

/// Stable identifier for a decoded instruction in a machine-code function.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct InstructionId(pub usize);

/// A relocatable address expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Address {
    Absolute(u64),
    Symbol { name: String, addend: i64 },
}

/// Placeholder for an architecture-neutral instruction wrapper.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Instruction {
    pub address: u64,
    pub bytes: Vec<u8>,
}

/// Placeholder for a basic block.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BasicBlock {
    pub id: BasicBlockId,
    pub instructions: Vec<InstructionId>,
}
