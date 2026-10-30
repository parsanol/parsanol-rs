//! Compact values carried between recursive atom applications.

use crate::portable::ast::{AstNode, ParseResult};
use crate::portable::capture_state::CaptureState;

/// A capture snapshot is rare during atom evaluation. Keeping it inline in
/// every result reserves its maps and vectors on every recursive stack frame,
/// even for `None`; that exhausts small native stacks on ordinary recursion.
/// The public ParseResult stays unchanged and is constructed at API boundaries.
#[derive(Debug, Clone)]
pub(crate) struct AtomResult {
    pub(crate) value: AstNode,
    pub(crate) end_pos: usize,
    pub(crate) capture_state: Option<Box<CaptureState>>,
}

impl From<AtomResult> for ParseResult {
    fn from(result: AtomResult) -> Self {
        Self {
            value: result.value,
            end_pos: result.end_pos,
            capture_state: result.capture_state.map(|state| *state),
        }
    }
}
