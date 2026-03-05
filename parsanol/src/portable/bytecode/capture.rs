//! Capture processing for the bytecode VM
//!
//! This module processes the capture stack after a successful parse
//! and builds proper AST nodes (Hash nodes for named captures, etc.)
//!
//! # Architecture
//!
//! The capture processing follows these principles:
//! - **OOP**: Each struct has a single responsibility
//! - **MECE**: All capture kinds are handled distinctly
//! - **Separation of Concerns**: Processing is separate from execution
//!
//! # Capture Model
//!
//! During execution:
//! 1. OpenCapture pushes a frame with start position
//! 2. Pattern matching continues
//! 3. CloseCapture marks the end position
//!
//! After successful parse:
//! 1. CaptureProcessor processes the capture stack
//! 2. Builds Hash nodes for named captures
//! 3. Handles nested captures properly
//! 4. Returns the final AST node

use super::instruction::CaptureKind;
use super::program::Program;
use crate::portable::arena::AstArena;
use crate::portable::ast::AstNode;

/// A capture frame that tracks both open and closed state
#[derive(Debug, Clone)]
pub struct CaptureFrame {
    /// Start position of the capture
    pub start_pos: usize,
    /// End position of the capture (set when closed)
    pub end_pos: Option<usize>,
    /// Kind of capture
    pub kind: CaptureKind,
    /// Key index (for named captures)
    pub key_idx: u32,
    /// Nested captures (captures made within this capture's scope)
    pub children: Vec<CaptureFrame>,
}

impl CaptureFrame {
    /// Create a new open capture frame
    #[inline]
    pub fn open(start_pos: usize, kind: CaptureKind, key_idx: u32) -> Self {
        Self {
            start_pos,
            end_pos: None,
            kind,
            key_idx,
            children: Vec::new(),
        }
    }

    /// Close the capture at the given position
    #[inline]
    pub fn close(&mut self, end_pos: usize) {
        self.end_pos = Some(end_pos);
    }

    /// Check if the capture is closed
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.end_pos.is_some()
    }

    /// Get the length of the captured region
    #[inline]
    pub fn length(&self) -> usize {
        self.end_pos.unwrap_or(self.start_pos) - self.start_pos
    }
}

/// Process captures and build AST nodes
pub struct CaptureProcessor<'a> {
    /// Reference to the program (for key table access)
    program: &'a Program,
    /// Reference to the AST arena
    arena: &'a mut AstArena,
}

impl<'a> CaptureProcessor<'a> {
    /// Create a new capture processor
    #[inline]
    pub fn new(program: &'a Program, arena: &'a mut AstArena) -> Self {
        Self { program, arena }
    }

    /// Process a flat capture stack and build the result AST
    ///
    /// This handles both flat captures and nested captures by building
    /// a tree structure based on capture ranges.
    pub fn process_flat(&mut self, captures: &[CaptureFrame], final_pos: usize) -> AstNode {
        if captures.is_empty() {
            // No captures: return reference to entire match
            return self.arena.input_ref(0, final_pos);
        }

        // Build a tree structure from flat captures based on nesting
        let tree = self.build_capture_tree(captures, final_pos);

        // Process the tree to build AST nodes
        self.process_tree(&tree, final_pos)
    }

    /// Build a tree structure from flat captures
    ///
    /// Captures are nested when one capture's range contains another.
    /// For example: `outer:(inner:"ab")` produces:
    /// - outer: start=0, end=2
    /// - inner: start=0, end=2
    ///
    /// The inner capture is a child of outer.
    fn build_capture_tree(&self, captures: &[CaptureFrame], final_pos: usize) -> Vec<CaptureFrame> {
        let mut root_captures: Vec<CaptureFrame> = Vec::new();

        for capture in captures {
            let mut capture = capture.clone();

            // If not closed, use final_pos as end position
            if capture.end_pos.is_none() {
                capture.end_pos = Some(final_pos);
            }

            // Try to find a parent capture that contains this one
            let mut inserted = false;
            for parent in &mut root_captures {
                if self.try_insert_into_parent(&mut capture, parent) {
                    inserted = true;
                    break;
                }
            }

            // If no parent found, add as root capture
            if !inserted {
                root_captures.push(capture);
            }
        }

        root_captures
    }

    /// Try to insert a capture into a parent capture (or its children)
    fn try_insert_into_parent(
        &self,
        capture: &mut CaptureFrame,
        parent: &mut CaptureFrame,
    ) -> bool {
        let cap_start = capture.start_pos;
        let cap_end = capture.end_pos.unwrap();
        let parent_start = parent.start_pos;
        let parent_end = parent.end_pos.unwrap();

        // Check if capture is contained within parent
        if cap_start >= parent_start && cap_end <= parent_end {
            // Check if this capture is the same as parent (same range and same key)
            // In that case, don't nest - this handles the case where the same
            // named capture appears multiple times
            if cap_start == parent_start
                && cap_end == parent_end
                && capture.key_idx == parent.key_idx
            {
                return false;
            }

            // Try to insert into existing children first
            for child in &mut parent.children {
                if self.try_insert_into_parent(capture, child) {
                    return true;
                }
            }

            // No child contains this capture, add as new child
            parent.children.push(capture.clone());
            return true;
        }

        false
    }

    /// Process a tree of captures and build AST nodes
    fn process_tree(&mut self, captures: &[CaptureFrame], final_pos: usize) -> AstNode {
        if captures.is_empty() {
            return self.arena.input_ref(0, final_pos);
        }

        if captures.len() == 1 {
            return self.frame_to_node_with_children(&captures[0], final_pos);
        }

        // Multiple root captures: build an array
        let items: Vec<AstNode> = captures
            .iter()
            .map(|frame| self.frame_to_node_with_children(frame, final_pos))
            .collect();

        let (pool_index, length) = self.arena.store_array(&items);
        AstNode::Array { pool_index, length }
    }

    /// Convert a capture frame to an AST node, processing children
    fn frame_to_node_with_children(&mut self, frame: &CaptureFrame, final_pos: usize) -> AstNode {
        let end_pos = frame.end_pos.unwrap_or(final_pos);
        let length = end_pos - frame.start_pos;

        match frame.kind {
            CaptureKind::Named => {
                // Named capture: build a Hash node
                let name = self.program.get_key(frame.key_idx).unwrap_or("unknown");

                // Determine the value:
                // - If there are children, process them as the value
                // - Otherwise, use InputRef to the matched text
                let value = if frame.children.is_empty() {
                    self.arena.input_ref(frame.start_pos, length)
                } else {
                    // Process children
                    self.process_tree(&frame.children, end_pos)
                };

                let (pool_index, length) = self.arena.store_hash(&[(name, value)]);
                AstNode::Hash { pool_index, length }
            }
            CaptureKind::Simple | CaptureKind::Group => {
                // Simple capture: if has children, process them; otherwise return reference
                if frame.children.is_empty() {
                    self.arena.input_ref(frame.start_pos, length)
                } else {
                    self.process_tree(&frame.children, end_pos)
                }
            }
            CaptureKind::Position => {
                // Position capture: return the position as a number
                // For now, we use InputRef with zero length
                self.arena.input_ref(frame.start_pos, 0)
            }
            CaptureKind::Range => {
                // Range capture: return reference to matched text
                self.arena.input_ref(frame.start_pos, length)
            }
            CaptureKind::Constant => {
                // Constant capture: would need constant table
                // For now, return empty reference
                self.arena.input_ref(frame.start_pos, 0)
            }
            CaptureKind::Action => {
                // Action capture: would need runtime function dispatch
                // For now, return reference to matched text
                self.arena.input_ref(frame.start_pos, length)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_program() -> Program {
        Program::new()
    }

    #[test]
    fn test_capture_frame_open_close() {
        let mut frame = CaptureFrame::open(10, CaptureKind::Simple, 0);
        assert!(!frame.is_closed());
        assert_eq!(frame.length(), 0);

        frame.close(20);
        assert!(frame.is_closed());
        assert_eq!(frame.length(), 10);
    }

    #[test]
    fn test_processor_empty_captures() {
        let program = make_test_program();
        let mut arena = AstArena::new();
        let mut processor = CaptureProcessor::new(&program, &mut arena);

        let result = processor.process_flat(&[], 10);

        // Should return InputRef to entire match
        match result {
            AstNode::InputRef { offset, length } => {
                assert_eq!(offset, 0);
                assert_eq!(length, 10);
            }
            _ => panic!("Expected InputRef"),
        }
    }

    #[test]
    fn test_processor_single_simple_capture() {
        let program = make_test_program();
        let mut arena = AstArena::new();
        let mut processor = CaptureProcessor::new(&program, &mut arena);

        let mut frame = CaptureFrame::open(5, CaptureKind::Simple, 0);
        frame.close(10);
        let captures = vec![frame];

        let result = processor.process_flat(&captures, 10);

        // Should return InputRef to captured region
        match result {
            AstNode::InputRef { offset, length } => {
                assert_eq!(offset, 5);
                assert_eq!(length, 5);
            }
            _ => panic!("Expected InputRef"),
        }
    }

    #[test]
    fn test_processor_single_named_capture() {
        let mut program = make_test_program();
        let key_idx = program.add_key("name");
        let mut arena = AstArena::new();
        let mut processor = CaptureProcessor::new(&program, &mut arena);

        let mut frame = CaptureFrame::open(0, CaptureKind::Named, key_idx);
        frame.close(5);
        let captures = vec![frame];

        let result = processor.process_flat(&captures, 5);

        // Should return Hash node
        match result {
            AstNode::Hash { pool_index, length } => {
                assert_eq!(length, 1);
                let items = arena.get_hash_items(pool_index as usize, length as usize);
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].0, "name");
            }
            _ => panic!("Expected Hash node"),
        }
    }

    #[test]
    fn test_processor_multiple_captures() {
        let program = make_test_program();
        let mut arena = AstArena::new();
        let mut processor = CaptureProcessor::new(&program, &mut arena);

        let mut frame1 = CaptureFrame::open(0, CaptureKind::Simple, 0);
        frame1.close(3);
        let mut frame2 = CaptureFrame::open(3, CaptureKind::Simple, 0);
        frame2.close(6);
        let captures = vec![frame1, frame2];

        let result = processor.process_flat(&captures, 6);

        // Should return Array node
        match result {
            AstNode::Array { pool_index, length } => {
                assert_eq!(length, 2);
                let items = arena.get_array(pool_index as usize, length as usize);
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected Array node"),
        }
    }

    #[test]
    fn test_processor_nested_captures() {
        let mut program = make_test_program();
        let outer_key = program.add_key("outer");
        let inner_key = program.add_key("inner");
        let mut arena = AstArena::new();
        let mut processor = CaptureProcessor::new(&program, &mut arena);

        // Nested captures: outer:(inner:"ab")
        // Both start at 0, end at 2
        let mut outer = CaptureFrame::open(0, CaptureKind::Named, outer_key);
        outer.close(2);
        let mut inner = CaptureFrame::open(0, CaptureKind::Named, inner_key);
        inner.close(2);

        // The outer is added first, then inner
        let captures = vec![outer, inner];

        let result = processor.process_flat(&captures, 2);

        // Should return Hash node for outer, containing Hash for inner
        match result {
            AstNode::Hash { pool_index, length } => {
                assert_eq!(length, 1);
                let items = arena.get_hash_items(pool_index as usize, length as usize);
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].0, "outer");
                // The value should be another Hash for inner
                match items[0].1 {
                    AstNode::Hash { .. } => {}
                    _ => panic!("Expected nested Hash node for inner"),
                }
            }
            _ => panic!("Expected Hash node"),
        }
    }
}
