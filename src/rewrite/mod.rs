//! Symbolic, relocation-aware code rewriting.
//!
//! The rewrite layer turns a decoded code region into an editable IR whose
//! branch operands are *symbolic*: instead of pointing at a hard-coded
//! address, each PC-relative target carries a [`Target`] — a reference to
//! a basic block, an extern symbol, a constant pool entry, or a literal
//! address. This is what lets the layout pass freely insert, delete, and
//! grow code without invalidating displacements.
//!
//! ## Pipeline
//!
//! ```text
//!   bytes ──► sweep ──► instructions ──► CFG
//!                                         │
//!                                         ▼
//!                                  RewritePlan::lift
//!                                         │
//!                                         ▼  edit operations
//!                                  (mutate operands, blocks, terminators)
//!                                         │
//!                                         ▼
//!                                    lay_out(plan, base)
//!                                         │
//!                                         ▼
//!                                    emit(plan, layout) ──► bytes
//! ```
//!
//! ## Status
//!
//! - In-place patches (no shifting), single-block edits, and inserts that
//!   trigger conditional-branch widening: implemented and tested.
//! - Out-of-range unconditional branches (>128 MiB), branch islands, and
//!   literal-pool emission: not yet implemented; layout returns
//!   [`LayoutError::DisplacementTooLarge`] in those cases.
//! - Symbol and constant target resolution: types are defined, but no
//!   resolver exists until binary-container ingest lands.

pub mod commit;
pub mod data;
pub mod editor;
pub mod emit;
pub mod ir;
pub mod layout;
pub mod plan;

pub use commit::commit_to_container;
pub use data::{
    commit_to_data_container, emit_data_section, DataEditError, DataEmitOutput, DataItem,
    DataLift, DataLiftError, DataPayload, DataSection,
};
pub use editor::{
    BinaryEditor, BinaryState, InitialiserPosition, LiftedTextSection, TextEditorError,
};
#[allow(deprecated)]
pub use editor::TextEditor;
pub use emit::{emit, EmitError, EmitOutput, EmittedRelocation};
pub use ir::{
    ConstantId, MacroKind, MacroOp, RewriteBlock, RewriteInstruction, RewriteOp, RewriteOperand,
    SymbolId, Target,
};
pub use layout::{lay_out, EmitStrategy, InstructionLayout, Layout, LayoutError};
pub use plan::{EditError, RewritePlan};

pub use crate::mc::ControlFlowGraph;
