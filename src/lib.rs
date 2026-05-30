#![cfg_attr(not(feature = "write"), no_std)]

#![cfg_attr(all(not(feature = "runtime-dispatch-simd"), feature = "generic-simd"), feature(portable_simd))]

#![warn(clippy::all, clippy::pedantic, dead_code, clippy::cargo)]
#![allow(
    unused_assignments,
    clippy::inline_always,
    clippy::uninlined_format_args, // ?...
    clippy::borrow_as_ptr,
    clippy::negative_feature_names,
    clippy::redundant_closure_for_method_calls,
    clippy::sliced_string_as_bytes,
    clippy::should_implement_trait,
    clippy::collapsible_if,
    clippy::new_without_default,
    clippy::comparison_chain,
    clippy::redundant_field_names,
    clippy::semicolon_if_nothing_returned,
    clippy::pub_underscore_fields,
    clippy::struct_field_names,
    clippy::ptr_as_ptr,
    clippy::missing_transmute_annotations,
    clippy::multiple_crate_versions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::similar_names,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::used_underscore_binding,
    clippy::nonstandard_macro_braces,
    clippy::used_underscore_items,
    clippy::enum_glob_use,
    clippy::cast_lossless,
    clippy::match_same_arms,
    clippy::too_many_lines,
    clippy::unnested_or_patterns,
    clippy::blocks_in_conditions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
)]

#[cfg(feature = "write")]
extern crate std;

extern crate alloc;

#[allow(unused)]
use alloc::vec;
use alloc::vec::Vec;
use alloc::sync::Arc;
use alloc::string::{String, ToString};

use core::str;
use core::cmp::Ordering;
use core::ops::{Bound, Deref, RangeBounds};

#[cfg(not(feature = "dont_vendor"))]
mod cranelift_entity_vendor;
#[cfg(not(feature = "dont_vendor"))]
use cranelift_entity_vendor as cranelift_entity;

#[cfg(not(feature = "dont_vendor"))]
mod smallvec_vendor;
#[cfg(not(feature = "dont_vendor"))]
use smallvec_vendor as smallvec;

#[cfg(not(feature = "dont_vendor"))]
mod bytecount_vendor;
#[cfg(not(feature = "dont_vendor"))]
use bytecount_vendor as bytecount;

use smallvec::SmallVec;
use cranelift_entity::{EntityRef, PrimaryMap};
#[cfg(feature = "dont_vendor")]
use cranelift_entity::entity_impl;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct BufferRef(pub u32);
entity_impl!(BufferRef);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct NodeRef(pub u32);
entity_impl!(NodeRef);

pub const NIL: NodeRef = NodeRef(0);

pub const MOD_BUFFER: BufferRef = BufferRef(u32::MAX >> 1);

pub const CHECKPOINT_INTERVAL: u32 = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color { Black = 0, Red = 1 }

#[derive(Clone, Copy, Debug)]
pub struct HistoryEntry {
    pub root:          NodeRef,
    pub cursor_offset: u32,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub buffer:            BufferRef,

    pub byte_offset:       u32,
    pub byte_length:       u32,

    pub newline_count:     u32,
    pub char_count:        u32,

    pub buffer_start_line: u32,  // Index into that buffer's `newline_offsets` array of this piece's first '\n'

    pub piece_start_char:  u32,  // Absolute char index of p.offset in its buffer
}

// For tests and other stuff
#[derive(Clone, Copy, Debug)]
pub enum Edit {
    Insert { offset: u32, text: &'static str },
    Remove { offset: u32, length: u32 },
}

impl Edit {
    #[inline(always)]
    #[must_use]
    pub fn offset(&self) -> u32 {
        match self {
            Edit::Insert { offset, .. } => *offset,
            Edit::Remove { offset, .. } => *offset,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Node {
    pub left:              NodeRef, // 4 bytes
    pub right:             NodeRef, // 4 bytes
    pub offset:            u32,     // 4 bytes
    pub subtree_len:       u32,     // 4 bytes
    pub subtree_chars:     u32,     // 4 bytes
    pub subtree_newlines:  u32,     // 4 bytes
    pub meta:              u32,     // 4 bytes (Bit 0: Color, Bits 1..31: BufferIndex)
    pub buffer_start_line: u32,     // 4 bytes
    pub piece_start_char:  u32,     // 4 bytes
}

const _: () = assert!(size_of::<Node>() == 36);

impl Node {
    #[inline(always)]
    #[must_use]
    pub fn color(&self) -> Color {
        if (self.meta & 1) == 1 { Color::Red } else { Color::Black }
    }

    #[inline(always)]
    pub fn set_color(&mut self, color: Color) {
        self.meta = (self.meta & !1) | (color as u32);
    }

    #[inline(always)]
    #[must_use]
    pub fn buffer_index(&self) -> BufferRef {
        BufferRef(self.meta >> 1)
    }

    #[inline(always)]
    pub fn set_buffer(&mut self, buffer: BufferRef) {
        self.meta = (buffer.as_u32() << 1) | (self.meta & 1);
    }
}

#[derive(Debug, Clone)]
pub struct OriginalBuffer {
    pub text:             Arc<str>,
    pub newline_offsets:  Arc<[u32]>,
    pub char_checkpoints: Arc<[(u32, u32)]>,
}

impl Deref for OriginalBuffer {
    type Target = str;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.text.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct Buffers {
    pub original_buffers: PrimaryMap<BufferRef, OriginalBuffer>,

    pub modifications_char_count:       u32,
    pub modifications_buffer:           String,
    pub modifications_newline_offsets:  Vec<u32>,
    pub modifications_char_checkpoints: Vec<(u32, u32)>,
}

impl Default for Buffers {
    #[inline(always)]
    fn default() -> Self {
        Self {
            original_buffers: PrimaryMap::new(),
            modifications_char_count: 0,
            modifications_newline_offsets: Vec::with_capacity(1024),
            modifications_char_checkpoints: Vec::with_capacity(128),
            modifications_buffer: String::with_capacity(1024 * 64),
        }
    }
}

#[must_use]
#[inline(always)]
pub fn count_chars_and_newlines(bytes: &[u8]) -> (u32, u32) {
    let mut chars    = 0u32;
    let mut newlines = 0u32;

    for &b in bytes {
        newlines += (b == b'\n')         as u32;
        chars    += ((b & 0xC0) != 0x80) as u32;
    }

    (chars, newlines)
}

#[must_use]
#[inline(always)]
pub fn count_chars_and_newlines_with_offsets_and_checkpoints(
    bytes: &[u8],
    offsets: &mut Vec<u32>,
    checkpoints: &mut Vec<(u32, u32)>,
) -> (u32, u32) {
    let mut chars    = 0u32;
    let mut newlines = 0u32;

    let mut next_checkpoint = CHECKPOINT_INTERVAL;

    for (i, &b) in bytes.iter().enumerate() {
        if (b as i8) >= -0x40 {
            if chars == next_checkpoint {
                checkpoints.push((chars, i as u32));
                next_checkpoint += CHECKPOINT_INTERVAL;
            }
            chars += 1;
        }

        if b == b'\n' {
            offsets.push(i as u32);
            newlines += 1;
        }
    }

    (chars, newlines)
}

impl Buffers {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self { Self::default() }

    #[inline(always)]
    #[must_use]
    pub fn get(&self, buffer: BufferRef) -> &str {
        if buffer == MOD_BUFFER {
            &self.modifications_buffer
        } else {
            &self.original_buffers[buffer]
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn get_slice(&self, buffer: BufferRef, offset: u32, len: u32) -> &str {
        let buf = self.get(buffer);
        let start = offset as usize;
        let end = start + len as usize;
        unsafe { str::from_utf8_unchecked(buf.as_bytes().get_unchecked(start..end)) }
    }

    #[inline]
    #[must_use]
    pub fn count_chars_and_newlines(&self, buffer: BufferRef, start_offset: u32, len: u32) -> (u32, u32) {
        if len == 0 { return (0, 0) }

        let end_offset = start_offset + len;

        //
        // O(log N) newlines count
        //
        let nl_offsets = self.get_newlines(buffer);
        let start_nl_index = nl_offsets.binary_search(&start_offset).unwrap_or_else(|x| x);
        let end_nl_index = nl_offsets.binary_search(&end_offset).unwrap_or_else(|x| x);
        let newlines = (end_nl_index - start_nl_index) as u32;

        //
        // O(log N) char count
        //

        let checkpoints = self.get_checkpoints(buffer);
        let buffer_text = self.get(buffer);

        //
        // Gets absolute char count from start of buffer to `target_offset`
        //
        let chars_up_to = |target_offset: u32| -> u32 {
            if target_offset == 0 { return 0; }

            //
            // Find the index of the first checkpoint that is strictly strictly greater than our target
            //
            let partition_index = checkpoints.partition_point(|&(_char_cnt, byte_offset)| byte_offset <= target_offset);

            let (base_chars, base_bytes) = if partition_index > 0 {
                checkpoints[partition_index - 1]
            } else {
                (0, 0)
            };

            //
            // Linearly scan only the remaining bytes from the checkpoint to the target
            //
            let tail_bytes = &buffer_text[base_bytes as usize .. target_offset as usize];
            base_chars + bytecount::num_chars(tail_bytes.as_bytes()) as u32
        };

        let chars = chars_up_to(end_offset) - chars_up_to(start_offset);

        (chars, newlines)
    }

    #[inline(always)]
    #[must_use]
    pub fn count_chars(&self, buffer: BufferRef, offset: u32, len: u32) -> u32 {
        bytecount::num_chars(self.get_slice(buffer, offset, len).as_bytes()) as u32
    }

    #[inline(always)]
    #[must_use]
    pub fn count_newlines(&self, buffer: BufferRef, offset: u32, len: u32) -> u32 {
        bytecount::count(self.get_slice(buffer, offset, len).as_bytes(), b'\n') as _
    }

    #[inline(always)]
    fn get_newlines(&self, buffer: BufferRef) -> &[u32] {
        if buffer == MOD_BUFFER {
            &self.modifications_newline_offsets
        } else {
            &self.original_buffers[buffer].newline_offsets
        }
    }

    #[inline(always)]
    fn get_checkpoints(&self, buffer: BufferRef) -> &[(u32, u32)] {
        if buffer == MOD_BUFFER {
            &self.modifications_char_checkpoints
        } else {
            &self.original_buffers[buffer].char_checkpoints
        }
    }

    /// Converts an absolute char index to an absolute byte offset for a specific buffer
    #[must_use]
    #[inline]
    pub fn char_to_byte_absolute(&self, buffer: BufferRef, target_char: u32) -> u32 {
        if target_char == 0 { return 0; }

        let checkpoints = self.get_checkpoints(buffer);
        let cp_index = checkpoints.partition_point(|&(c, _)| c <= target_char);

        let (current_char, current_byte) = if cp_index == 0 {
            (0, 0)
        } else {
            unsafe { *checkpoints.get_unchecked(cp_index - 1) }
        };

        let remainder_chars = target_char - current_char;
        if remainder_chars == 0 { return current_byte; }

        let text = self.get(buffer);
        let slice = unsafe { text.get_unchecked(current_byte as usize..) };

        let additional_bytes = slice
            .char_indices()
            .nth(remainder_chars as usize).map_or_else(|| slice.len() as u32, |(b, _)| b as u32);  // Fallback to end if out of bounds

        current_byte + additional_bytes
    }

    /// Converts an absolute byte offset to an absolute char index for a specific buffer
    #[must_use]
    #[inline]
    pub fn byte_to_char_absolute(&self, buffer: BufferRef, target_byte: u32) -> u32 {
        if target_byte == 0 { return 0; }

        let checkpoints = self.get_checkpoints(buffer);
        let cp_index = checkpoints.partition_point(|&(_, b)| b <= target_byte);

        let (current_char, current_byte) = if cp_index == 0 {
            (0, 0)
        } else {
            unsafe { *checkpoints.get_unchecked(cp_index - 1) }
        };

        let remainder_bytes = target_byte - current_byte;
        if remainder_bytes == 0 { return current_char; }

        let text = self.get(buffer);
        let slice = unsafe { text.get_unchecked(current_byte as usize..target_byte as usize) };
        let additional_chars = bytecount::num_chars(slice.as_bytes()) as u32;

        current_char + additional_chars
    }
}

#[derive(Debug, Clone)]
pub struct Pieces {
    pub nodes: PrimaryMap<NodeRef, Node>,
}

impl Default for Pieces {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl Pieces {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        let mut nodes = PrimaryMap::new();
        nodes.push(Node {
            left: NIL, right: NIL, offset: 0,
            subtree_len: 0, subtree_chars: 0, subtree_newlines: 0,
            meta: 0, buffer_start_line: 0,
            piece_start_char: 0
        });
        Self { nodes }
    }

    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize { self.nodes.len() }

    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }

    #[inline(always)]
    #[must_use]
    pub fn get(&self, index: NodeRef) -> &Node { &self.nodes[index] }

    #[inline(always)]
    #[must_use]
    pub fn get_piece(&self, index: NodeRef) -> Piece {
        if index == NIL { return Piece::default(); }

        let node = &self.nodes[index];
        let l = &self.nodes[node.left];
        let r = &self.nodes[node.right];

        Piece {
            buffer: node.buffer_index(),
            byte_offset: node.offset,
            byte_length: node.subtree_len - l.subtree_len - r.subtree_len,
            char_count: node.subtree_chars - l.subtree_chars - r.subtree_chars,
            newline_count: node.subtree_newlines - l.subtree_newlines - r.subtree_newlines,
            buffer_start_line: node.buffer_start_line,
            piece_start_char: node.piece_start_char,
        }
    }

    #[inline(always)]
    pub fn alloc(&mut self, color: Color, left: NodeRef, piece: Piece, right: NodeRef) -> NodeRef {
        let l = &self.nodes[left];
        let r = &self.nodes[right];

        let mut node = Node {
            left, right,
            offset: piece.byte_offset,
            subtree_len: l.subtree_len + piece.byte_length + r.subtree_len,
            subtree_chars: l.subtree_chars + piece.char_count + r.subtree_chars,
            subtree_newlines: l.subtree_newlines + piece.newline_count + r.subtree_newlines,
            buffer_start_line: piece.buffer_start_line,
            piece_start_char: piece.piece_start_char,
            meta: 0,
        };
        node.set_color(color);
        node.set_buffer(piece.buffer);

        self.nodes.push(node)
    }

    /// Extends a piece without triggering Red-Black structural changes.
    /// It creates a single new path to the root to maintain persistence.
    pub fn extend_piece(
        &mut self,
        root: NodeRef,
        piece_start_offset: u32,
        add_len: u32,
        add_chars: u32,
        add_nl: u32,
    ) -> NodeRef {
        if root == NIL { return NIL }

        let node = self.nodes[root];
        let mut new_node = node;

        let left_len = self.nodes[node.left].subtree_len;
        let piece_len = node.subtree_len - left_len - self.nodes[node.right].subtree_len;

        if piece_start_offset < left_len {
            new_node.left  = self.extend_piece(node.left, piece_start_offset, add_len, add_chars, add_nl);
        } else if piece_start_offset > left_len {
            new_node.right = self.extend_piece(node.right, piece_start_offset - (left_len + piece_len), add_len, add_chars, add_nl);
        } else {
            // piece_start_offset == left_len
            // We found the exact node! No further recursion needed.
        }

        new_node.subtree_len      += add_len;
        new_node.subtree_chars    += add_chars;
        new_node.subtree_newlines += add_nl;

        self.nodes.push(new_node)
    }

    #[inline]
    pub fn insert_node(&mut self, root: NodeRef, piece: Piece, at: u32) -> NodeRef {
        let new_root = self.ins(root, piece, at, 0);
        let r_node = self.nodes[new_root];
        let p = self.get_piece(new_root);
        self.alloc(Color::Black, r_node.left, p, r_node.right)
    }

    #[inline]
    pub fn remove_node(&mut self, root: NodeRef, at: u32) -> NodeRef {
        let new_root = self.rem(root, at, 0);
        if new_root == NIL { return NIL }

        let r_node = self.nodes[new_root];
        self.alloc(Color::Black, r_node.left, self.get_piece(new_root), r_node.right)
    }

    #[inline]
    fn rem(&mut self, root: NodeRef, at: u32, total: u32) -> NodeRef {
        if root == NIL { return NIL }

        let node = self.nodes[root];
        let node_piece = self.get_piece(root);
        let left_len = self.nodes[node.left].subtree_len;

        match at.cmp(&(total + left_len)) {
            Ordering::Less => self.remove_left(root, at, total),
            Ordering::Equal => self.fuse(node.left, node.right),
            Ordering::Greater => {
                let next_total = total + left_len + node_piece.byte_length;
                self.remove_right(root, at, next_total)
            }
        }
    }

    #[inline]
    fn remove_left(&mut self, root: NodeRef, at: u32, total: u32) -> NodeRef {
        let node = self.nodes[root];
        let new_left = self.rem(node.left, at, total);
        let new_node = self.alloc(Color::Red, new_left, self.get_piece(root), node.right);
        if self.nodes[node.left].color() == Color::Black {
            self.balance_left(new_node)
        } else {
            new_node
        }
    }

    #[inline]
    fn remove_right(&mut self, root: NodeRef, at: u32, total: u32) -> NodeRef {
        let node = self.nodes[root];
        let new_right = self.rem(node.right, at, total);
        let new_node = self.alloc(Color::Red, node.left, self.get_piece(root), new_right);
        if self.nodes[node.right].color() == Color::Black {
            self.balance_right(new_node)
        } else {
            new_node
        }
    }

    #[inline]
    fn ins(&mut self, root: NodeRef, p: Piece, at: u32, total_offset: u32) -> NodeRef {
        if root == NIL { return self.alloc(Color::Red, NIL, p, NIL); }

        let node = self.nodes[root];
        let node_piece = self.get_piece(root);
        let left_len = self.nodes[node.left].subtree_len;

        if at < total_offset + left_len + node_piece.byte_length {
            let lft = self.ins(node.left, p, at, total_offset);
            self.balance(node.color(), lft, node_piece, node.right)
        } else {
            let next_offset = total_offset + left_len + node_piece.byte_length;
            let rgt = self.ins(node.right, p, at, next_offset);
            self.balance(node.color(), node.left, node_piece, rgt)
        }
    }

    #[inline]
    fn balance(&mut self, c: Color, l_index: NodeRef, p: Piece, r_index: NodeRef) -> NodeRef {
        let l = self.nodes[l_index];
        let r = self.nodes[r_index];

        if c == Color::Black {
            if l.color() == Color::Red {
                let ll = self.nodes[l.left];
                let lr = self.nodes[l.right];
                if ll.color() == Color::Red {
                    let new_l = self.alloc(Color::Black, ll.left, self.get_piece(l.left), ll.right);
                    let new_r = self.alloc(Color::Black, l.right, p, r_index);
                    return self.alloc(Color::Red, new_l, self.get_piece(l_index), new_r);
                } else if lr.color() == Color::Red {
                    let new_l = self.alloc(Color::Black, l.left, self.get_piece(l_index), lr.left);
                    let new_r = self.alloc(Color::Black, lr.right, p, r_index);
                    return self.alloc(Color::Red, new_l, self.get_piece(l.right), new_r);
                }
            }

            if r.color() == Color::Red {
                let rl = self.nodes[r.left];
                let rr = self.nodes[r.right];
                if rl.color() == Color::Red {
                    let new_l = self.alloc(Color::Black, l_index, p, rl.left);
                    let new_r = self.alloc(Color::Black, rl.right, self.get_piece(r_index), r.right);
                    return self.alloc(Color::Red, new_l, self.get_piece(r.left), new_r);
                } else if rr.color() == Color::Red {
                    let new_l = self.alloc(Color::Black, l_index, p, r.left);
                    let new_r = self.alloc(Color::Black, rr.left, self.get_piece(r.right), rr.right);
                    return self.alloc(Color::Red, new_l, self.get_piece(r_index), new_r);
                }
            }
        }

        self.alloc(c, l_index, p, r_index)
    }

    #[inline]
    fn fuse(&mut self, left: NodeRef, right: NodeRef) -> NodeRef {
        // match: (left, right)

        // case: (None, r)
        if left  == NIL { return right }
        if right == NIL { return left }

        let l_node = self.nodes[left];
        let r_node = self.nodes[right];

        // match: (left.color, right.color)

        // case: (B, R)
        if l_node.color() == Color::Black && r_node.color() == Color::Red {
            let fused = self.fuse(left, r_node.left);
            return self.alloc(Color::Red, fused, self.get_piece(right), r_node.right);
        }

        // case: (R, B)
        if l_node.color() == Color::Red && r_node.color() == Color::Black {
            let fused = self.fuse(l_node.right, right);
            return self.alloc(Color::Red, l_node.left, self.get_piece(left), fused);
        }

        // case: (R, R)
        if l_node.color() == Color::Red && r_node.color() == Color::Red {
            let fused = self.fuse(l_node.right, r_node.left);
            let f_node = self.nodes[fused];
            if fused != NIL && f_node.color() == Color::Red {
                let new_l = self.alloc(Color::Red, l_node.left, self.get_piece(left), f_node.left);
                let new_r = self.alloc(Color::Red, f_node.right, self.get_piece(right), r_node.right);
                return self.alloc(Color::Red, new_l, self.get_piece(fused), new_r);
            }
            let new_r = self.alloc(Color::Red, fused, self.get_piece(right), r_node.right);
            return self.alloc(Color::Red, l_node.left, self.get_piece(left), new_r);
        }

        // case: (B, B)

        let fused = self.fuse(l_node.right, r_node.left);
        let f_node = self.nodes[fused];
        if fused != NIL && f_node.color() == Color::Red {
            let new_l = self.alloc(Color::Black, l_node.left, self.get_piece(left), f_node.left);
            let new_r = self.alloc(Color::Black, f_node.right, self.get_piece(right), r_node.right);
            return self.alloc(Color::Red, new_l, self.get_piece(fused), new_r);
        }

        let new_r = self.alloc(Color::Black, fused, self.get_piece(right), r_node.right);
        let new_node = self.alloc(Color::Red, l_node.left, self.get_piece(left), new_r);
        self.balance_left(new_node)
    }

    #[inline]
    fn balance_left(&mut self, left: NodeRef) -> NodeRef {
        let l_node = self.nodes[left];
        let ll_node = self.nodes[l_node.left];
        let lr_node = self.nodes[l_node.right];

        // match: (color_l, color_r, color_r_l)

        // case: (Some(R), ..)
        if l_node.left != NIL && ll_node.color() == Color::Red {
            let new_ll = self.alloc(Color::Black, ll_node.left, self.get_piece(l_node.left), ll_node.right);
            return self.alloc(Color::Red, new_ll, self.get_piece(left), l_node.right);
        }

        // case: (_, Some(B), _)
        if l_node.right != NIL && lr_node.color() == Color::Black {
            let new_lr = self.alloc(Color::Red, lr_node.left, self.get_piece(l_node.right), lr_node.right);
            let new_l = self.alloc(Color::Black, l_node.left, self.get_piece(left), new_lr);
            let nl = self.nodes[new_l];
            return self.balance(Color::Black, nl.left, self.get_piece(new_l), nl.right);
        }

        // case: (_, Some(R), Some(B))
        if l_node.right != NIL && lr_node.color() == Color::Red {
            let lrl_node = self.nodes[lr_node.left];
            if lr_node.left != NIL && lrl_node.color() == Color::Black {
                let lrr_node = self.nodes[lr_node.right];
                let new_lrr = self.alloc(Color::Red, lrr_node.left, self.get_piece(lr_node.right), lrr_node.right);
                let unbal_r = self.alloc(Color::Black, lrl_node.right, self.get_piece(l_node.right), new_lrr);
                let ur_node = self.nodes[unbal_r];
                let new_r = self.balance(ur_node.color(), ur_node.left, self.get_piece(unbal_r), ur_node.right);
                let new_l = self.alloc(Color::Black, l_node.left, self.get_piece(left), lrl_node.left);
                return self.alloc(Color::Red, new_l, self.get_piece(lr_node.left), new_r);
            }
        }

        left
    }

    #[inline]
    fn balance_right(&mut self, right: NodeRef) -> NodeRef {
        let r_node = self.nodes[right];
        let rl_node = self.nodes[r_node.left];
        let rr_node = self.nodes[r_node.right];

        // match: (color_l, color_l_r, color_r)

        // case: (.., Some(R))
        if r_node.right != NIL && rr_node.color() == Color::Red {
            let new_rr = self.alloc(Color::Black, rr_node.left, self.get_piece(r_node.right), rr_node.right);
            return self.alloc(Color::Red, r_node.left, self.get_piece(right), new_rr);
        }

        // case: (Some(B), ..)
        if r_node.left != NIL && rl_node.color() == Color::Black {
            let new_rl = self.alloc(Color::Red, rl_node.left, self.get_piece(r_node.left), rl_node.right);
            let new_r = self.alloc(Color::Black, new_rl, self.get_piece(right), r_node.right);
            let nr = self.nodes[new_r];
            return self.balance(Color::Black, nr.left, self.get_piece(new_r), nr.right);
        }

        // case: (Some(R), Some(B), _)
        if r_node.left != NIL && rl_node.color() == Color::Red {
            let rlr_node = self.nodes[rl_node.right];
            if rl_node.right != NIL && rlr_node.color() == Color::Black {
                let rll_node = self.nodes[rl_node.left];
                let new_rll = self.alloc(Color::Red, rll_node.left, self.get_piece(rl_node.left), rll_node.right);
                let unbal_l = self.alloc(Color::Black, new_rll, self.get_piece(r_node.left), rlr_node.left);
                let ul_node = self.nodes[unbal_l];
                let new_l = self.balance(ul_node.color(), ul_node.left, self.get_piece(unbal_l), ul_node.right);
                let new_r = self.alloc(Color::Black, rlr_node.right, self.get_piece(right), r_node.right);
                return self.alloc(Color::Red, new_l, self.get_piece(rl_node.right), new_r);
            }
        }

        right
    }

    #[inline]
    #[must_use]
    pub fn find_offset(
        &self,
        mut root: NodeRef,
        mut target_offset: u32,
        prefer_left: bool
    ) -> Option<(NodeRef, u32)> {
        while root != NIL {
            let node = &self.nodes[root];
            let left_len = self.nodes[node.left].subtree_len;
            let piece_len = self.get_piece(root).byte_length;

            if target_offset < left_len {
                root = node.left;

            } else if target_offset == left_len + piece_len && prefer_left {
                // We are perfectly on the boundary and want to merge with the left piece
                return Some((root, piece_len));

            } else if target_offset < left_len + piece_len {
                return Some((root, target_offset - left_len));

            } else {
                target_offset -= left_len + piece_len;
                root = node.right;
            }
        }

        None
    }
}

impl PieceTree {
    /// Slices a tree exactly at `offset`, returning (`LeftTree`, `RightTree`)
    #[inline]
    pub fn split(&mut self, root: NodeRef, offset: u32) -> (NodeRef, NodeRef) {
        if root == NIL {
            return (NIL, NIL);
        }

        let node = self.pieces.nodes[root];
        let p = self.pieces.get_piece(root);
        let left_len = self.pieces.nodes[node.left].subtree_len;

        if offset < left_len {
            let (ll, lr) = self.split(node.left, offset);

            // Fast path: Use join_with_middle directly
            let new_right = self.pieces.join_with_middle(lr, p, node.right);
            (ll, new_right)

        } else if offset > left_len + p.byte_length {
            let (rl, rr) = self.split(node.right, offset - left_len - p.byte_length);

            // Fast path: Use join_with_middle directly
            let new_left = self.pieces.join_with_middle(node.left, p, rl);
            (new_left, rr)

        } else {
            let rel_offset = offset - left_len;

            if rel_offset == 0 {
                (node.left, self.pieces.join_with_middle(NIL, p, node.right))

            } else if rel_offset == p.byte_length {
                (self.pieces.join_with_middle(node.left, p, NIL), node.right)

            } else {
                //
                // We have to slice the Piece exactly in half!
                //
                let left_len = rel_offset;
                let right_len = p.byte_length - rel_offset;

                //
                // Only scan the smaller half of the piece to avoid O(N) bottlenecks
                //
                let (left_chars, left_nl) = if left_len <= right_len {
                    // Left side is shorter, scan normally
                    self.buffers.count_chars_and_newlines(p.buffer, p.byte_offset, left_len)
                } else {
                    // Right side is shorter, scan it and subtract from the known totals
                    let (r_chars, r_nl) = self.buffers.count_chars_and_newlines(
                        p.buffer, p.byte_offset + left_len, right_len
                    );

                    (p.char_count - r_chars, p.newline_count - r_nl)
                };

                let left_stump = Piece {
                    buffer: p.buffer,
                    byte_offset: p.byte_offset,
                    byte_length: left_len,
                    newline_count: left_nl,
                    char_count: left_chars,
                    buffer_start_line: p.buffer_start_line,
                    piece_start_char: p.piece_start_char,
                };

                let right_stump = Piece {
                    buffer: p.buffer,
                    byte_offset: p.byte_offset + left_len,
                    byte_length: right_len,
                    newline_count: p.newline_count - left_nl,
                    char_count: p.char_count - left_chars,
                    buffer_start_line: p.buffer_start_line + left_nl,
                    piece_start_char: p.piece_start_char + left_chars,
                };

                let new_left = self.pieces.join_with_middle(node.left, left_stump, NIL);
                let new_right = self.pieces.join_with_middle(NIL, right_stump, node.right);
                (new_left, new_right)
            }
        }
    }

    /// Glues two arbitrary trees together by extracting the max element of the left tree
    #[inline]
    pub fn concat(&mut self, left: NodeRef, right: NodeRef) -> NodeRef {
        //
        // Group the evaluation so we can force a Black root on the way out!
        //
        let new_root = if left == NIL {
            right
        } else if right == NIL {
            left
        } else {
            let mut curr = left;
            while self.pieces.nodes[curr].right != NIL {
                curr = self.pieces.nodes[curr].right;
            }
            let max_piece = self.pieces.get_piece(curr);
            let max_offset = self.pieces.nodes[left].subtree_len - max_piece.byte_length;

            let left_without_max = self.pieces.remove_node(left, max_offset);

            self.pieces.join_with_middle(left_without_max, max_piece, right)
        };

        //
        // Even if we early-returned right, we MUST ensure the final returned root is Black.
        //
        if new_root != NIL && self.pieces.nodes[new_root].color() == Color::Red {
            let p = self.pieces.get_piece(new_root);
            let r = self.pieces.nodes[new_root];
            return self.pieces.alloc(Color::Black, r.left, p, r.right);
        }

        new_root
    }
}

impl Pieces {
    /// Recursively counts the black height of a given subtree
    #[inline(always)]
    #[must_use]
    pub fn black_height(&self, mut node: NodeRef) -> u32 {
        let mut h = 0;
        while node != NIL {
            if self.nodes[node].color() == Color::Black {
                h += 1;
            }
            node = self.nodes[node].left;
        }
        h
    }

    /// Safely joins two arbitrary Red-Black trees using a middle element
    #[inline(always)]
    pub fn join_with_middle(&mut self, left: NodeRef, piece: Piece, right: NodeRef) -> NodeRef {
        let hl = self.black_height(left);
        let hr = self.black_height(right);

        let new_root = if hl > hr {
            self.join_right(left, piece, right, hl, hr)
        } else if hr > hl {
            self.join_left(left, piece, right, hl, hr)
        } else {
            self.alloc(Color::Red, left, piece, right)
        };

        // Enforce the Red-Black root constraint!
        if new_root != NIL && self.nodes[new_root].color() == Color::Red {
            let p = self.get_piece(new_root);
            let r = self.nodes[new_root];
            return self.alloc(Color::Black, r.left, p, r.right);
        }

        new_root
    }

    #[inline(always)]
    fn join_right(&mut self, left: NodeRef, piece: Piece, right: NodeRef, hl: u32, hr: u32) -> NodeRef {
        if hl == hr {
            return self.alloc(Color::Red, left, piece, right);
        }

        let l_node = self.nodes[left];
        let next_hl = if l_node.color() == Color::Black { hl - 1 } else { hl };

        let new_right = self.join_right(l_node.right, piece, right, next_hl, hr);
        let p = self.get_piece(left);

        self.balance(l_node.color(), l_node.left, p, new_right)
    }

    #[inline(always)]
    fn join_left(&mut self, left: NodeRef, piece: Piece, right: NodeRef, hl: u32, hr: u32) -> NodeRef {
        if hl == hr {
            return self.alloc(Color::Red, left, piece, right);
        }

        let r_node = self.nodes[right];
        let next_hr = if r_node.color() == Color::Black { hr - 1 } else { hr };

        let new_left = self.join_left(left, piece, r_node.left, hl, next_hr);
        let p = self.get_piece(right);

        self.balance(r_node.color(), new_left, p, r_node.right)
    }
}

#[derive(Debug, Clone)]
pub struct PieceTree {
    //
    // `last_insert_end`      is the mod-buffer offset one past the last inserted byte.
    // `last_insert_tree_end` is the document   offset one past that insertion.
    //
    // Both are u32::MAX when unset (after removes, undos, or on init).
    //
    last_mod_end:  u32,  // fredbuf's last_insert     (BufferCursor)
    last_tree_end: u32,  // fredbuf's end_last_insert (CharOffset)

    pub root: NodeRef,

    /// Tracks nested undo groups. 0 if we are outside any group.
    pub transaction_depth: usize,

    pub undo_stack: Vec<HistoryEntry>,
    pub redo_stack: Vec<HistoryEntry>,

    pub pieces:  Pieces,
    pub buffers: Buffers,

    pub scratch_index_map: Vec<u32>,
}

impl Default for PieceTree {
    fn default() -> Self {
        Self {
            last_mod_end:      u32::MAX,
            last_tree_end:     u32::MAX,
            root:              NIL,
            transaction_depth: 0,
            undo_stack:        Vec::new(),
            redo_stack:        Vec::new(),
            pieces:            Pieces::new(),
            buffers:           Buffers::new(),
            scratch_index_map: Vec::new(),
        }
    }
}

impl core::fmt::Display for PieceTree {
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for chunk in self.slice(..).chunks() {
            f.write_str(chunk)?;
        }

        Ok(())
    }
}

impl<T> From<T> for PieceTree where T: AsRef<str> {
    #[inline(always)]
    fn from(value: T) -> Self {
        Self::from_arc_str(value.as_ref())
    }
}

impl str::FromStr for PieceTree {
    type Err = core::convert::Infallible;

    #[inline(always)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(PieceTree::from(s))
    }
}

impl PieceTree {
    #[inline(always)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    #[must_use]
    pub fn from_arc_str(text: impl Into<Arc<str>>) -> Self {
        let text = text.into();

        let mut tree = PieceTree::new();
        if text.is_empty() { return tree; }

        let bytes = text.as_bytes();
        let byte_length = bytes.len() as u32;

        let mut offsets     = Vec::with_capacity(bytes.len() / 40);
        let mut checkpoints = Vec::with_capacity(bytes.len() / 256);

        let (char_count, newline_count) =
            count_chars_and_newlines_with_offsets_and_checkpoints(
                text.as_bytes(),
                &mut offsets,
                &mut checkpoints,
            );

        let buffer = tree.buffers.original_buffers.push(OriginalBuffer {
            newline_offsets:  offsets.into(),
            char_checkpoints: checkpoints.into(),
            text:             text.into(),
        });

        let piece = Piece {
            buffer,
            byte_offset:       0,
            byte_length,
            char_count,
            newline_count,
            buffer_start_line: 0,
            piece_start_char:  0,
        };

        tree.root = tree.pieces.insert_node(NIL, piece, 0);
        tree
    }

    /// Splits the tree at `byte_offset`, returning the right half as a new
    /// `PieceTree`.
    ///
    /// After this call, `self` contains `[0, byte_offset)` and
    /// the returned tree contains `[byte_offset, end)`.
    ///
    /// `byte_offset` must be on a UTF-8 char boundary. Panics if `byte_offset > self.len_bytes()`.
    ///
    /// Runs in O(log n).
    pub fn split_off(&mut self, byte_offset: u32) -> Self {
        assert!(
            byte_offset <= self.len_bytes(),
            "split_off: byte_offset {} out of bounds (len = {})",
            byte_offset, self.len_bytes()
        );

        if byte_offset == 0 {
            //
            // Everything goes to the right half -> self becomes empty
            //
            let mut right = Self::default();
            right.pieces  = self.pieces.clone();
            right.buffers = self.buffers.clone();
            right.root    = self.root;
            self.root     = NIL;
            self.last_mod_end  = u32::MAX;
            self.last_tree_end = u32::MAX;

            return right;
        }

        if byte_offset == self.len_bytes() {
            //
            // Everything stays in self          -> return empty right half
            //

            let mut right = Self::default();
            right.pieces  = self.pieces.clone();
            right.buffers = self.buffers.clone();
            // right.root stays NIL

            return right;
        }

        //
        // Split the tree at the byte boundary
        //
        let (left_root, right_root) = self.split(self.root, byte_offset);

        //
        // Self keeps the left half
        //
        self.root          = left_root;
        self.last_mod_end  = u32::MAX;
        self.last_tree_end = u32::MAX;
        self.undo_stack.clear();
        self.redo_stack.clear();

        //
        // Right half gets a new PieceTree sharing the same arena and buffers
        //
        let mut right = Self::default();
        right.pieces  = self.pieces.clone();
        right.buffers = self.buffers.clone();
        right.root    = right_root;

        right
    }

    #[cfg(feature = "write")]
    pub fn from_reader<R: std::io::Read>(mut reader: R) -> std::io::Result<Self> {
        const CHUNK: usize = 64 * 1024;

        let mut buf  = [0u8; CHUNK];
        let mut text = String::new();
        let mut tail = 0usize;

        loop {
            let n = reader.read(&mut buf[tail..])?;

            if n == 0 {
                // EOF
                if tail > 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "stream ended with incomplete UTF-8 sequence",
                    ));
                }

                break;
            }

            let filled = tail + n;
            let slice  = &buf[..filled];

            //
            // Find how much is valid UTF-8
            //
            let valid_up_to = match str::from_utf8(slice) {
                Ok(_)  => filled,
                Err(e) => {
                    if e.error_len().is_some() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "stream did not contain valid UTF-8",
                        ));
                    }
                    e.valid_up_to()
                }
            };

            //
            // SAFETY: validated above
            //
            text.push_str(unsafe { std::str::from_utf8_unchecked(&slice[..valid_up_to]) });

            //
            // Carry over the incomplete sequence (0-3 bytes) to next iteration
            //
            tail = filled - valid_up_to;
            if tail > 0 {
                buf.copy_within(valid_up_to..filled, 0);
            }
        }

        Ok(Self::from(text))
    }
}

impl PieceTree {
    /// Captures the current state of the document.
    #[inline]
    #[must_use]
    pub fn take_snapshot(&self, current_cursor: u32) -> HistoryEntry {
        HistoryEntry {
            root: self.root,
            cursor_offset: current_cursor,
        }
    }

    /// Restores the tree to a previously saved snapshot.
    /// Returns the cursor offset from the snapshot.
    #[inline]
    #[must_use]
    pub fn snap_to(&mut self, snapshot: HistoryEntry, current_cursor: u32) -> u32 {
        if self.root == snapshot.root {
            return snapshot.cursor_offset;
        }

        //
        // Prevent snapping while in the middle of an active transaction
        //
        assert!(self.transaction_depth == 0, "Cannot snap_to during an active undo group");

        //
        // Save the current state to the undo stack so the user can undo the jump
        //
        self.undo_stack.push(HistoryEntry {
            root: self.root,
            cursor_offset: current_cursor,
        });

        //
        // Clear the redo stack
        //
        self.redo_stack.clear();

        //
        // Invalidate caches
        //
        self.last_tree_end = u32::MAX;
        self.last_mod_end  = u32::MAX;

        //
        //  Restore the tree
        //
        self.root = snapshot.root;

        snapshot.cursor_offset
    }
}

impl PieceTree {
    /// Starts a new undo group, saves the current state if this is the outermost group.
    #[inline]
    pub fn begin_undo_group(&mut self, cursor_offset: u32) {
        if self.transaction_depth == 0 {
            self.commit_head(cursor_offset);
        }

        self.transaction_depth += 1;
    }

    /// Ends the current undo group
    #[inline]
    pub fn end_undo_group(&mut self) {
        if self.transaction_depth > 0 {
            self.transaction_depth -= 1;
        }
    }

    #[inline(always)]
    pub fn commit_head(&mut self, cursor_offset: u32) {
        if let Some(last) = self.undo_stack.last() {
            if last.root == self.root {
                return;  // Root hasn't changed, skip duplication
            }
        }

        self.undo_stack.push(HistoryEntry { root: self.root, cursor_offset });
        self.redo_stack.clear();
    }

    #[inline(always)]
    pub fn try_undo(&mut self, current_cursor: u32) -> Option<u32> {
        if self.transaction_depth > 0 {
            return None;
        }

        if let Some(entry) = self.undo_stack.pop() {
            self.last_tree_end = u32::MAX;
            self.last_mod_end  = u32::MAX;

            self.redo_stack.push(HistoryEntry {
                root: self.root, cursor_offset: current_cursor
            });
            self.root = entry.root;
            return Some(entry.cursor_offset);
        }

        None
    }

    #[inline(always)]
    pub fn try_redo(&mut self, current_cursor: u32) -> Option<u32> {
        if let Some(entry) = self.redo_stack.pop() {
            self.last_tree_end = u32::MAX;
            self.last_mod_end  = u32::MAX;

            self.undo_stack.push(HistoryEntry {
                root: self.root, cursor_offset: current_cursor
            });
            self.root = entry.root;
            return Some(entry.cursor_offset);
        }

        None
    }

    #[inline(always)]
    #[must_use]
    pub fn total_length(&self) -> u32 {
        self.pieces.get(self.root).subtree_len
    }

    #[inline(always)]
    pub fn apply_edits(&mut self, primary_cursor_offset: u32, edits: &mut [Edit]) {
        if edits.is_empty() { return }

        self.begin_undo_group(primary_cursor_offset);

        edits.sort_by_key(|b| core::cmp::Reverse(b.offset()));
        for edit in edits {
            match edit {
                Edit::Insert { offset, text }   => self.insert_no_commit(*offset, text),
                Edit::Remove { offset, length } => self.remove_no_commit(*offset, *length),
            }
        }

        self.end_undo_group();
    }

    /// Inserts a single character at the specified logical byte offset.
    #[inline(always)]
    pub fn insert_char(&mut self, offset: u32, ch: char) {
        self.commit_head(offset);
        self.insert_char_no_commit(offset, ch);
    }

    /// Inserts a single character at the specified logical byte offset.
    #[inline(always)]
    pub fn insert_char_no_commit(&mut self, offset: u32, ch: char) {
        let mut buf = [0; 4];
        self.insert_no_commit(offset, ch.encode_utf8(&mut buf));
    }

    #[inline(always)]
    pub fn insert(&mut self, offset: u32, text: &str) {
        if text.is_empty() { return }

        let auto_group = self.transaction_depth == 0;
        if auto_group {
            self.begin_undo_group(offset);
        }

        self.insert_no_commit(offset, text);

        if auto_group {
            self.end_undo_group();
        }
    }

    #[inline(always)]
    pub fn remove_at(&mut self, offset: u32, length: u32) {
        if length == 0 || self.root == NIL { return }

        let auto_group = self.transaction_depth == 0;
        if auto_group {
            self.begin_undo_group(offset);
        }

        self.remove_no_commit(offset, length);

        if auto_group {
            self.end_undo_group();
        }
    }

    pub fn insert_no_commit(&mut self, offset: u32, text: &str) {
        let mod_offset = self.buffers.modifications_buffer.len() as u32;
        let start_line_in_buffer = self.buffers.modifications_newline_offsets.len() as u32;

        let total_char_count_before = self.buffers.modifications_char_count;
        let mut total_char_count = total_char_count_before;

        self.buffers.modifications_newline_offsets.reserve(text.len() / 20 + 1);
        self.buffers.modifications_char_checkpoints.reserve(text.len() / CHECKPOINT_INTERVAL as usize + 1);

        let mut newline_count = 0;
        let mut char_count = 0;
        {
            let rem = total_char_count % CHECKPOINT_INTERVAL;
            let mut next_checkpoint = total_char_count + ((CHECKPOINT_INTERVAL - rem) % CHECKPOINT_INTERVAL);

            if next_checkpoint == 0 {
                next_checkpoint = CHECKPOINT_INTERVAL;
            }

            //
            // @Cutnpaste from count_chars_and_newlines_with_offsets_and_checkpoints
            //
            for (i, b) in text.bytes().enumerate() {
                if (b as i8) >= -0x40 {
                    if total_char_count == next_checkpoint {
                        self.buffers.modifications_char_checkpoints.push((total_char_count, mod_offset + i as u32));
                        next_checkpoint += CHECKPOINT_INTERVAL;
                    }

                    total_char_count += 1;
                    char_count += 1;
                }

                if b == b'\n' {
                    self.buffers.modifications_newline_offsets.push(mod_offset + i as u32);
                    newline_count += 1;
                }
            }
        }

        self.buffers.modifications_char_count = total_char_count;
        self.buffers.modifications_buffer.push_str(text);
        let text_len = text.len() as u32;

        let new_mod_end  = mod_offset + text_len;
        let new_tree_end = offset + text_len;

        //
        // Fast path for sequential typing
        //
        if offset > 0
        && self.last_tree_end == offset
        && self.last_mod_end == mod_offset
        && let Some((prev_index, prev_rel)) = self.find_position(offset - 1, false)
        {
            let prev = self.pieces.get_piece(prev_index);

            //
            // Predecessor must be a mod-buf piece ending at mod_offset.
            //
            if prev.buffer == MOD_BUFFER && prev.byte_offset + prev.byte_length == mod_offset {
                let prev_start = (offset - 1) - prev_rel;

                self.root = self.pieces.extend_piece(
                    self.root,
                    prev_start,
                    text_len,
                    char_count,
                    newline_count
                );

                self.last_mod_end  = new_mod_end;
                self.last_tree_end = new_tree_end;
                return;
            }
        }

        let new_piece = Piece {
            buffer: MOD_BUFFER,
            byte_offset: mod_offset,
            byte_length: text_len,
            newline_count,
            char_count,
            buffer_start_line: start_line_in_buffer,
            piece_start_char:  total_char_count - char_count,
        };

        if self.root == NIL {
            self.root = self.pieces.insert_node(self.root, new_piece, offset);
        } else {
            //
            // Snip the tree exactly at the insertion offset
            //
            let (left, right) = self.split(self.root, offset);

            //
            // Sandwich the new piece directly between the left and right trees!
            //
            self.root = self.pieces.join_with_middle(left, new_piece, right);

            //
            // Clean up the resulting seams
            //

            // Attempt to merge the left side with new_piece
            self.try_merge_at(offset);

            // Attempt to merge new_piece with the right side
            self.try_merge_at(offset + text_len);
        }

        self.last_mod_end  = new_mod_end;
        self.last_tree_end = new_tree_end;
    }

    pub fn remove_no_commit(&mut self, offset: u32, mut length: u32) {
        let total = self.total_length();
        if offset >= total { return; }
        if offset + length > total { length = total - offset; }
        if length == 0 { return }

        //
        // Snip off the portion of the tree that comes BEFORE the deletion
        //
        let (left, remainder) = self.split(self.root, offset);

        //
        // Snip the deleted portion out of the remainder
        //
        let (_deleted, right) = self.split(remainder, length);

        //
        // Glue the surviving outer halves back together
        //
        self.root = self.concat(left, right);

        //
        // Clean up the resulting seam
        //
        self.try_merge_at(offset);
    }

    #[inline]
    fn try_merge_at(&mut self, pos: u32) {
        if pos == 0 { return }

        let Some((right_index, right_rel)) = self.find_position(pos, false) else { return };

        if right_rel != 0 { return }

        let right = self.pieces.get_piece(right_index);
        self.try_merge_right_with_left(pos, right);
    }

    // Returns Some((merged_start, merged_piece)) if merge happened, None otherwise.
    #[inline]
    fn try_merge_right_with_left(&mut self, right_start: u32, right: Piece) -> Option<(u32, Piece)> {
        if right_start == 0 { return None; }
        if right.buffer != MOD_BUFFER { return None; }

        let (left_index, left_rel) = self.find_position(right_start - 1, false)?;
        let left = self.pieces.get_piece(left_index);

        if left.buffer != MOD_BUFFER { return None; }
        if left.byte_offset + left.byte_length != right.byte_offset { return None; }

        let left_start = (right_start - 1) - left_rel;

        self.root = self.pieces.remove_node(self.root, left_start);
        self.root = self.pieces.remove_node(self.root, left_start);

        let merged = Piece {
            buffer:  MOD_BUFFER,
            byte_offset:        left.byte_offset,
            byte_length:        left.byte_length        + right.byte_length,
            newline_count: left.newline_count + right.newline_count,
            char_count:    left.char_count    + right.char_count,
            buffer_start_line: left.buffer_start_line,
            piece_start_char: left.piece_start_char,  // right side extends, left start unchanged
        };
        self.root = self.pieces.insert_node(self.root, merged, left_start);
        Some((left_start, merged))
    }

    #[inline(always)]
    #[must_use]
    pub fn get_piece(&self, index: NodeRef) -> Piece {
        self.pieces.get_piece(index)
    }

    #[inline]
    #[must_use]
    pub fn find_position(&self, offset: u32, prefer_left: bool) -> Option<(NodeRef, u32)> {
        let total = self.total_length();
        if offset > total { return None; }

        if offset == total && total > 0 {
            let mut current = self.root;
            let mut last_valid = NIL;
            while current != NIL {
                last_valid = current;
                current = self.pieces.get(current).right;
            }
            let len = self.pieces.get_piece(last_valid).byte_length;
            return Some((last_valid, len));
        }

        self.pieces.find_offset(self.root, offset, prefer_left)
    }

    #[cfg(feature = "write")]
    #[inline]
    pub fn write_to<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()> {
        if self.root == NIL {
            return Ok(());
        }

        for (_, piece) in self.pieces() {
            let slice = self.buffers.get_slice(piece.buffer, piece.byte_offset, piece.byte_length);
            writer.write_all(slice.as_bytes())?;
        }

        writer.flush()
    }
}

impl PieceTree {
    #[inline]
    pub fn compact(&mut self) {
        #[inline(always)]
        fn copy_node(
            old_index: NodeRef, old_arena: &Pieces,
            new_arena: &mut Pieces, index_map: &mut [u32]
        ) -> NodeRef {
            if old_index == NIL { return NIL }

            if index_map[old_index.index()] != 0 {
                return NodeRef::new(index_map[old_index.index()] as usize);
            }

            let node = old_arena.get(old_index);
            let left_new = copy_node(node.left, old_arena, new_arena, index_map);
            let right_new = copy_node(node.right, old_arena, new_arena, index_map);

            let new_index = NodeRef::new(new_arena.nodes.len());
            new_arena.nodes.push(Node {
                left: left_new,
                right: right_new,
                offset: node.offset,
                subtree_len: node.subtree_len,
                subtree_chars: node.subtree_chars,
                subtree_newlines: node.subtree_newlines,
                piece_start_char: node.piece_start_char,
                meta: node.meta,
                buffer_start_line: node.buffer_start_line
            });

            index_map[old_index.index()] = new_index.index() as u32;
            new_index
        }

        let mut new_pieces = Pieces::new();

        self.scratch_index_map.clear();
        self.scratch_index_map.resize(self.pieces.nodes.len(), 0);

        self.root = copy_node(self.root, &self.pieces, &mut new_pieces, &mut self.scratch_index_map);
        for entry in &mut self.undo_stack {
            entry.root = copy_node(entry.root, &self.pieces, &mut new_pieces, &mut self.scratch_index_map);
        }
        for entry in &mut self.redo_stack {
            entry.root = copy_node(entry.root, &self.pieces, &mut new_pieces, &mut self.scratch_index_map);
        }

        self.pieces = new_pieces;
    }

    #[inline]
    pub fn squash(&mut self) {  // @Memory
        if self.root == NIL { return }

        let squashed_text = self.to_string();  // @Memory
        let bytes  = squashed_text.as_bytes();
        let length = squashed_text.len() as u32;

        let mut offsets     = Vec::with_capacity(bytes.len() / 40);
        let mut checkpoints = Vec::with_capacity(bytes.len() / 256);

        let (char_count, newline_count) =
            count_chars_and_newlines_with_offsets_and_checkpoints(
                squashed_text.as_bytes(),
                &mut offsets,
                &mut checkpoints
            );

        self.buffers = Buffers::new();
        let buffer = self.buffers.original_buffers.push(OriginalBuffer {
            newline_offsets: offsets.into(),
            char_checkpoints: checkpoints.into(),
            text: squashed_text.into()
        });
        self.pieces = Pieces::new();

        let piece = Piece {
            byte_length: length, newline_count, char_count,
            buffer, byte_offset: 0,
            buffer_start_line: 0, piece_start_char: 0
        };
        self.root = self.pieces.insert_node(NIL, piece, 0);

        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryUsage {
    pub node_arena:       u32,
    pub mod_buffer:       u32,
    pub original_buffers: u32,
    pub history:          u32,
}

impl MemoryUsage {
    #[inline(always)]
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.node_arena + self.mod_buffer + self.original_buffers + self.history
    }

    /// Overhead = everything except the actual document content in buffers.
    #[inline(always)]
    #[must_use]
    pub const fn overhead(&self) -> u32 {
        self.node_arena + self.history
    }
}

impl PieceTree {
    /// Total bytes allocated for the node arena (includes NIL sentinel and
    /// all historical nodes retained for undo/redo).
    #[inline(always)]
    #[must_use]
    pub fn node_arena_bytes(&self) -> u32 {
        (self.pieces.nodes.len() * size_of::<Node>()) as _
    }

    /// Bytes consumed by the modifications buffer (append-only, never shrinks).
    #[inline(always)]
    #[must_use]
    pub fn mod_buffer_bytes(&self) -> u32 {
        self.buffers.modifications_buffer.len() as u32 +
            (self.buffers.modifications_char_checkpoints.len() * size_of::<(u32, u32)>()) as u32 +
            (self.buffers.modifications_newline_offsets.len() * size_of::<u32>()) as u32
    }

    /// Bytes consumed by all original (read) buffers.
    #[inline(always)]
    #[must_use]
    pub fn original_buffers_bytes(&self) -> u32 {
        self.buffers.original_buffers.values().map(|s| {
            s.len() +
                s.char_checkpoints.len() * size_of::<(u32, u32)>() +
                s.newline_offsets.len() * size_of::<u32>()
        }).sum::<usize>() as _
    }

    /// Bytes consumed by undo + redo history entries.
    #[inline(always)]
    #[must_use]
    pub fn history_bytes(&self) -> u32 {
        ((self.undo_stack.len() + self.redo_stack.len()) * size_of::<HistoryEntry>()) as _
    }

    /// Number of live nodes in the arena (includes NIL and all historical nodes).
    #[inline(always)]
    #[must_use]
    pub fn node_count(&self) -> u32 {
        self.pieces.nodes.len() as _
    }

    /// Aggregate memory usage breakdown.
    #[inline(always)]
    #[must_use]
    pub fn memory_usage(&self) -> MemoryUsage {
        MemoryUsage {
            node_arena:       self.node_arena_bytes(),
            mod_buffer:       self.mod_buffer_bytes(),
            original_buffers: self.original_buffers_bytes(),
            history:          self.history_bytes(),
        }
    }
}

#[derive(Debug)]
pub struct LinesIter<'a> {
    tree: &'a PieceTree,
    current_line: u32,
    total_lines: u32,
}

impl Iterator for LinesIter<'_> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_line >= self.total_lines { return None }

        let line = self.tree.get_line_content_allocating(self.current_line)?;
        self.current_line += 1;
        Some(line)
    }
}

/// A lazy, non-allocating slice view over a byte range of the tree
#[derive(Debug)]
pub struct ChunkIter<'a> {
    tree:       &'a PieceTree,
    pieces:     PieceTreeIter<'a>,
    // Byte range we care about
    start:      u32,
    end:        u32,
    // Bytes consumed so far across all pieces
    piece_start: u32,
}

impl<'a> ChunkIter<'a> {
    #[inline]
    #[must_use]
    pub fn new(tree: &'a PieceTree, start: u32, end: u32) -> Self {
        Self {
            tree,
            pieces: PieceTreeIter::new(&tree.pieces, tree.root),
            start,
            end,
            piece_start: 0,
        }
    }

    /// Returns the total number of characters within this chunk view boundary.
    #[inline]
    #[must_use]
    pub fn len_chars(&self) -> u32 {
        let start_char = self.tree.try_byte_to_char(self.start).unwrap_or(0);
        let end_char = self.tree.try_byte_to_char(self.end).unwrap_or_else(|| self.tree.len_chars());
        end_char.saturating_sub(start_char)
    }

    /// Returns the total number of characters within this chunk view boundary.
    #[inline]
    #[must_use]
    pub fn len_bytes(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        loop {
            let (_, p) = self.pieces.next()?;
            let piece_end = self.piece_start + p.byte_length;

            // Skip pieces entirely before our window
            if piece_end <= self.start {
                self.piece_start = piece_end;
                continue;
            }
            // Stop once we're past our window
            if self.piece_start >= self.end {
                return None;
            }

            let slice_start = self.start.saturating_sub(self.piece_start);
            let slice_end   = (self.end - self.piece_start).min(p.byte_length);

            self.piece_start = piece_end;

            let text = self.tree.buffers.get_slice(p.buffer, p.byte_offset + slice_start, slice_end - slice_start);
            if text.is_empty() { continue; }
            return Some(text);
        }
    }
}

/// A zero-copy iterator that yields lines as perfectly bounded `TreeSlice`s.
#[derive(Debug, Clone)]
pub struct SliceLines<'a> {
    slice: TreeSlice<'a>,
    current_abs_line: u32,
    end_abs_line: u32,
    yielded_all: bool,
}

impl<'a> Iterator for SliceLines<'a> {
    type Item = TreeSlice<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.yielded_all { return None; }

        // Fetch the absolute start byte of the current line from the tree
        let line_start = self.slice.tree.try_line_to_byte(self.current_abs_line).unwrap_or(self.slice.start);

        //
        // If we are on the very last line that this slice overlaps
        //
        if self.current_abs_line >= self.end_abs_line {
            self.yielded_all = true;

            // Clamp the start byte between the slice start and end boundaries
            let start_byte = line_start.max(self.slice.start).min(self.slice.end);

            return Some(TreeSlice {
                tree: self.slice.tree,
                start: start_byte,
                end: self.slice.end,
            });
        }

        // We are on an intermediate line, find where the next line starts.
        let next_line_start = self.slice.tree.try_line_to_byte(self.current_abs_line + 1).unwrap_or(self.slice.end);

        // Clamp boundaries to ensure we never bleed outside the TreeSlice
        let start_byte = line_start.max(self.slice.start).min(self.slice.end);
        let end_byte = next_line_start.min(self.slice.end);

        self.current_abs_line += 1;

        Some(TreeSlice {
            tree: self.slice.tree,
            start: start_byte,
            end: end_byte,
        })
    }
}

#[derive(Debug)]
pub struct SliceCharsRev<'a> {
    walker: ReverseTreeWalker<'a>,
    current_byte: u32,
    start_byte: u32,
}

impl<'a> Iterator for SliceCharsRev<'a> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<char> {
        // If we've hit or crossed the start boundary, stop
        if self.current_byte <= self.start_byte {
            return None;
        }

        let c = self.walker.next()?;

        // Subtract the exact byte width of the character we just yielded
        self.current_byte = self.current_byte.saturating_sub(c.len_utf8() as u32);

        // If yielding this character pushed us before the slice's start
        // boundary, we discard it and end the iterator.
        if self.current_byte < self.start_byte {
            return None;
        }

        Some(c)
    }
}

impl core::fmt::Display for SliceCharsRev<'_> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut chars = Vec::new();
        let mut temp_walker = ReverseTreeWalker::new_at(self.walker.tree, self.current_byte);

        let mut temp_byte = self.current_byte;
        while temp_byte > self.start_byte {
            let Some(c) = temp_walker.next() else { break };
            let new_byte = temp_byte.saturating_sub(c.len_utf8() as u32);
            if new_byte < self.start_byte { break; }
            temp_byte = new_byte;
            chars.push(c);
        }

        chars.reverse();

        let mut buf = Vec::with_capacity(chars.iter().map(|c| c.len_utf8()).sum());
        let mut tmp = [0u8; 4];
        for c in &chars {
            buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
        }

        f.write_str(unsafe { core::str::from_utf8_unchecked(&buf) })
    }
}

/// Non-allocating char iterator over a byte-bounded window of the tree.
#[derive(Debug)]
pub struct SliceChars<'a> {
    walker:   TreeWalker<'a>,
    byte_end: u32,
}

impl Iterator for SliceChars<'_> {
    type Item = char;
    #[inline]
    fn next(&mut self) -> Option<char> {
        if self.walker.offset >= self.byte_end { return None; }
        self.walker.next()
    }
}

impl core::fmt::Display for SliceChars<'_> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Grab the current absolute byte offset from the existing walker
        let current_offset = self.walker.offset;

        // Spawn a transient walker and seek it to the exact current offset
        let mut temp_walker = TreeWalker::<32>::new(self.walker.tree);
        temp_walker.seek(current_offset);

        while temp_walker.offset < self.byte_end {
            let chunk = temp_walker.current_str.as_str();
            if chunk.is_empty() {
                if temp_walker.stack.is_empty() { break; }
                temp_walker.populate_chars();
                continue;
            }

            // Clamp chunk to byte_end
            let remaining = (self.byte_end - temp_walker.offset) as usize;
            let chunk = &chunk[..remaining.min(chunk.len())];
            f.write_str(chunk)?;
            temp_walker.offset += chunk.len() as u32;
            temp_walker.current_str = "".chars();
            temp_walker.populate_chars();
        }

        Ok(())
    }
}

/// A zero-copy borrowed view over a byte range of the tree.
/// All offsets are relative to the slice start.
#[derive(Debug, Clone, Copy)]
pub struct TreeSlice<'a> {
    tree:  &'a PieceTree,
    start: u32,   // byte offset into tree (inclusive)
    end:   u32,   // byte offset into tree (exclusive)
}

impl PartialEq<&str> for TreeSlice<'_> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        let mut remaining = other.as_bytes();

        for chunk in self.chunks() {
            let chunk_bytes = chunk.as_bytes();

            // If the chunk is longer than the remaining string, they don't match
            if remaining.len() < chunk_bytes.len() {
                return false;
            }

            // If the bytes don't match, fail fast
            if !remaining.starts_with(chunk_bytes) {
                return false;
            }

            remaining = &remaining[chunk_bytes.len()..];
        }

        // If we exhausted all chunks and have no remaining bytes, it's a perfect match
        remaining.is_empty()
    }
}

impl PartialEq<str> for TreeSlice<'_> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self == &other
    }
}

impl core::fmt::Display for TreeSlice<'_> {
    #[inline(always)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for chunk in self.chunks() {
            f.write_str(chunk)?;
        }
        Ok(())
    }
}

/// Slice Iterators
impl<'a> TreeSlice<'a> {
    #[inline]
    pub fn chunks(self) -> ChunkIter<'a> {
        ChunkIter::new(self.tree, self.start, self.end)
    }

    #[inline]
    pub fn bytes(self) -> impl Iterator<Item = u8> + 'a {
        self.chunks().flat_map(|chunk| chunk.bytes())
    }

    #[inline]
    pub fn chars(self) -> impl Iterator<Item = char> + 'a {
        self.chunks().flat_map(|chunk| chunk.chars())
    }

    #[inline]
    pub fn chars_rev(self) -> SliceCharsRev<'a> {
        SliceCharsRev {
            walker: ReverseTreeWalker::new_at(self.tree, self.end),
            current_byte: self.end,
            start_byte: self.start,
        }
    }

    #[inline]
    pub fn lines(self) -> SliceLines<'a> {
        // Query the tree directly for the absolute line overlap
        let start_line = self.tree.byte_to_line(self.start);
        let end_line = self.tree.byte_to_line(self.end);

        SliceLines {
            slice: self,
            current_abs_line: start_line,
            end_abs_line: end_line,
            yielded_all: false,
        }
    }

    #[inline]
    pub fn lines_at(self, line_idx: u32) -> SliceLines<'a> {
        let start_line = self.tree.byte_to_line(self.start);
        let end_line = self.tree.byte_to_line(self.end);

        let target_line = (start_line + line_idx).min(end_line);

        SliceLines {
            slice: self,
            current_abs_line: target_line,
            end_abs_line: end_line,
            yielded_all: false,
        }
    }

    #[inline]
    pub fn chunks_at_byte(self, byte_idx: u32) -> ChunkIter<'a> {
        let safe_idx = byte_idx.min(self.len_bytes());
        ChunkIter::new(self.tree, self.start + safe_idx, self.end)
    }

    #[inline]
    pub fn bytes_at(self, byte_idx: u32) -> impl Iterator<Item = u8> + 'a {
        self.chunks_at_byte(byte_idx).flat_map(|chunk| chunk.bytes())
    }

    #[inline]
    pub fn chars_at(self, char_idx: u32) -> impl Iterator<Item = char> + 'a {
        let safe_idx = char_idx.min(self.len_chars());
        let byte_offset = self.try_char_to_byte(safe_idx).unwrap_or(self.len_bytes());
        self.chunks_at_byte(byte_offset).flat_map(|chunk| chunk.chars())
    }
}

/// Tree Iterators
impl PieceTree {
    #[inline]
    pub fn slice_whole(&self) -> TreeSlice<'_> {
        self.slice(0..self.len_bytes())
    }

    #[inline]
    pub fn chunks(&self) -> ChunkIter<'_> { self.slice_whole().chunks() }

    #[inline]
    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ { self.slice_whole().bytes() }

    #[inline]
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ { self.slice_whole().chars() }

    #[inline]
    pub fn lines(&self) -> SliceLines<'_> { self.slice_whole().lines() }

    #[inline(always)]
    #[must_use]
    pub fn chars_rev(&self) -> ReverseTreeWalker<'_> { ReverseTreeWalker::new(self) }

    #[inline(always)]
    #[must_use]
    pub fn chars_at_rev(&self, char_idx: u32) -> ReverseTreeWalker<'_> { ReverseTreeWalker::new_at(self, char_idx) }

    #[inline]
    pub fn chunks_at_byte(&self, byte_idx: u32) -> ChunkIter<'_> {
        self.slice_whole().chunks_at_byte(byte_idx)
    }

    #[inline]
    pub fn bytes_at(&self, byte_idx: u32) -> impl Iterator<Item = u8> + '_ {
        self.slice_whole().bytes_at(byte_idx)
    }

    #[inline]
    pub fn chars_at(&self, char_idx: u32) -> impl Iterator<Item = char> + '_ {
        self.slice_whole().chars_at(char_idx)
    }

    #[inline]
    pub fn lines_at(&self, line_idx: u32) -> SliceLines<'_> {
        self.slice_whole().lines_at(line_idx)
    }
}

impl<'a> TreeSlice<'a> {
    #[inline(always)]
    #[must_use]
    pub fn new(tree: &'a PieceTree, start: u32, end: u32) -> Self {
        let end = end.min(tree.len_bytes());
        debug_assert!(start <= end);
        Self { tree, start, end }
    }

    #[inline(always)]
    #[must_use]
    pub fn len_bytes(&self) -> u32  { self.end - self.start }

    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self)  -> bool { self.start == self.end }

    #[inline(always)]
    #[must_use]
    pub fn len_chars(&self) -> u32 {
        let a = self.tree.try_byte_to_char(self.start).unwrap_or(0);
        let b = self.tree.try_byte_to_char(self.end).unwrap_or_else(|| self.tree.len_chars());
        b - a
    }

    #[inline]
    #[must_use]
    pub fn len_lines(&self) -> u32 {
        let start_line = self.tree.try_byte_to_line(self.start).unwrap_or(0);
        let end_line = self.tree.try_byte_to_line(self.end).unwrap_or_else(|| self.tree.len_lines() - 1);
        end_line - start_line + 1
    }

    #[must_use]
    pub fn chunk_at_byte(&self, offset: u32) -> &[u8] {
        if offset >= self.len_bytes() { return &[] }

        let abs_byte = self.start + offset;
        let chunk = self.tree.chunk_at_byte(abs_byte).0;

        // Truncate the chunk if it bleeds past the end of the slice
        let max_valid_len = (self.end - abs_byte) as usize;
        let bounded_len = chunk.len().min(max_valid_len);

        &chunk[..bounded_len]
    }

    #[inline]
    #[must_use]
    pub fn chunk_at_char(&self, char_offset: u32) -> &[u8] {
        if char_offset >= self.len_chars() { return &[] }

        let Some(base_char) = self.tree.try_byte_to_char(self.start) else {
            return &[];
        };
        let abs_char = base_char + char_offset;

        let chunk = self.tree.chunk_at_char(abs_char).0;

        // Truncate the chunk if it bleeds past the end of the slice
        let Some(abs_byte) = self.tree.try_char_to_byte(abs_char) else {
            return &[];
        };

        let max_valid_len = (self.end - abs_byte) as usize;
        let bounded_len = chunk.len().min(max_valid_len);

        &chunk[..bounded_len]
    }

    #[inline]
    #[must_use]
    pub fn chunk_at_line_break(&self, break_index: u32) -> &[u8] {
        let Some((base_line, _)) = self.tree.try_byte_to_line_col(self.start) else {
            return &[];
        };

        let abs_break_index = base_line + break_index;

        let Some(abs_byte) = self.tree.try_line_break_to_byte(abs_break_index) else {
            return &[];
        };

        // Ensure the line break actually falls within the slice bounds
        if abs_byte < self.start || abs_byte >= self.end {
            return &[];
        }

        let chunk = self.tree.chunk_at_byte(abs_byte).0;

        // Bound the chunk so it doesn't bleed past self.end
        let max_valid_len = (self.end - abs_byte) as usize;
        let bounded_len = chunk.len().min(max_valid_len);

        &chunk[..bounded_len]
    }

    /// Byte at a slice-relative byte offset.
    #[inline(always)]
    #[must_use]
    pub fn byte(&self, offset: u32) -> u8 {
        self.try_byte(offset).unwrap()
    }

    /// Byte at a slice-relative byte offset.
    #[inline(always)]
    #[must_use]
    pub fn try_byte(&self, offset: u32) -> Option<u8> {
        if offset >= self.len_bytes() { return None; }
        self.tree.try_byte(self.start + offset)
    }

    /// Char at a slice-relative char index.
    #[inline(always)]
    #[must_use]
    pub fn char(&self, char_index: u32) -> char {
        self.try_char(char_index).unwrap()
    }

    /// Char at a slice-relative char index.
    #[inline(always)]
    #[must_use]
    pub fn try_char(&self, char_index: u32) -> Option<char> {
        if char_index >= self.len_chars() { return None; }
        let abs_char = self.tree.try_byte_to_char(self.start)? + char_index;
        self.tree.try_char(abs_char)
    }

    #[inline(always)]
    #[must_use]
    pub fn line(&self, line: u32) -> ChunkIter<'a> {
        self.try_line(line).unwrap()
    }

    #[inline(always)]
    #[must_use]
    pub fn try_line(&self, line: u32) -> Option<ChunkIter<'a>> {
        let (abs_start, abs_end) = self.abs_line_range(line)?;
        // Use ChunkIter directly to enforce the absolute end boundary
        Some(ChunkIter::new(self.tree, abs_start, abs_end))
    }

    /// Returns a sub-slice over a byte range (relative to this slice).
    #[inline(always)]
    #[must_use]
    pub fn slice<R: RangeBounds<u32>>(&self, range: R) -> TreeSlice<'a> {
        let s = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded    => 0,
        };
        let e = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded    => self.len_bytes(),
        };
        TreeSlice::new(self.tree, self.start + s, self.start + e)
    }

    #[inline(always)]
    #[must_use]
    pub fn byte_to_char(&self, byte_offset: u32) -> u32 {
        self.try_byte_to_char(byte_offset).unwrap()
    }

    #[inline(always)]
    #[must_use]
    pub fn try_byte_to_char(&self, byte_offset: u32) -> Option<u32> {
        let base = self.tree.try_byte_to_char(self.start)?;
        let abs  = self.tree.try_byte_to_char(self.start + byte_offset)?;
        Some(abs - base)
    }

    #[inline(always)]
    #[must_use]
    pub fn char_to_byte(&self, char_index: u32) -> u32 {
        self.try_char_to_byte(char_index).unwrap()
    }

    #[inline(always)]
    #[must_use]
    pub fn try_char_to_byte(&self, char_index: u32) -> Option<u32> {
        let base_char = self.tree.try_byte_to_char(self.start)?;
        let abs_byte  = self.tree.try_char_to_byte(base_char + char_index)?;
        Some(abs_byte - self.start)
    }

    #[inline(always)]
    #[must_use]
    pub fn byte_to_line(&self, byte_offset: u32) -> u32 {
        self.try_byte_to_line(byte_offset).unwrap()
    }

    #[inline(always)]
    #[must_use]
    pub fn try_byte_to_line(&self, byte_offset: u32) -> Option<u32> {
        let (abs_line, _) = self.tree.try_byte_to_line_col(self.start + byte_offset)?;
        let base_line     = self.tree.try_byte_to_line_col(self.start).map(|(l, _)| l)?;
        Some(abs_line - base_line)
    }

    #[inline(always)]
    #[must_use]
    pub fn line_to_byte(&self, line: u32) -> u32 {
        self.try_line_to_byte(line).unwrap()
    }

    #[inline(always)]
    #[must_use]
    pub fn try_line_to_byte(&self, line: u32) -> Option<u32> {
        let (abs_start, _) = self.abs_line_range(line)?;
        Some(abs_start - self.start)
    }

    #[inline]
    #[must_use]
    pub fn abs_line_range(&self, rel_line: u32) -> Option<(u32, u32)> {
        let base_line = self.tree.try_byte_to_line_col(self.start).map(|(l, _)| l)?;
        let abs_line  = base_line + rel_line;

        let line_start = self.tree.try_line_to_byte(abs_line)?;
        let line_end   = self.tree.try_line_to_byte(abs_line + 1)
            .unwrap_or_else(|| self.tree.len_bytes());

        //
        // Clamp to slice bounds.
        //
        let s = line_start.max(self.start);
        let e = line_end.min(self.end);
        if s > e { return None; }

        Some((s, e))
    }
}

// Allow slicing directly from PieceTree.
impl PieceTree {
    #[inline]
    pub fn slice<R: RangeBounds<u32>>(&self, range: R) -> TreeSlice<'_> {
        let s = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded    => 0,
        };
        let e = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded    => self.len_bytes(),
        };
        TreeSlice::new(self, s, e)
    }

    /// Slice by char range.
    #[inline]
    pub fn slice_chars_range<R: RangeBounds<u32>>(&self, range: R) -> TreeSlice<'_> {
        let sc = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded    => 0,
        };
        let ec = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded    => self.len_chars(),
        };
        let s = self.try_char_to_byte(sc).unwrap_or(0);
        let e = self.try_char_to_byte(ec).unwrap_or_else(|| self.len_bytes());
        TreeSlice::new(self, s, e)
    }
}

impl PieceTree {
    /// Fast-path chunk reader specifically designed for the Tree-sitter C API.
    /// Given an absolute byte offset, it returns the largest contiguous byte
    /// slice available starting exactly at that offset.
    #[inline]
    #[must_use]
    pub fn read_largest_contigous_chunk_at_byte(&self, offset: u32) -> (&[u8], u32) {
        let total = self.total_length();
        if offset >= total {
            return (&[], offset);
        }

        let mut current = self.root;
        let mut current_offset = offset;
        let mut doc_offset = 0u32;

        while current != NIL {
            let node = self.pieces.get(current);
            let p = self.pieces.get_piece(current);
            let left_len = self.pieces.get(node.left).subtree_len;
            let piece_len = p.byte_length;

            if current_offset < left_len {
                current = node.left;

            } else if current_offset < left_len + piece_len {
                //
                // The requested offset falls inside this exact piece
                //

                let rel_offset = current_offset - left_len;
                let piece_doc_start = doc_offset + left_len;

                let text = self.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);

                return (&text.as_bytes()[rel_offset as usize..], piece_doc_start);

            } else {
                doc_offset     += left_len + piece_len;
                current_offset -= left_len + piece_len;
                current = node.right;
            }
        }

        (&[], offset)
    }

    #[inline]
    #[must_use]
    pub fn chunk_at_byte(&self, offset: u32) -> (&[u8], u32) {
        self.read_largest_contigous_chunk_at_byte(offset)
    }

    #[inline]
    #[must_use]
    pub fn chunk_at_char(&self, char_index: u32) -> (&[u8], u32) {
        let Some(offset) = self.try_char_to_byte(char_index) else { return (&[], 0) };
        self.chunk_at_byte(offset)
    }

    #[inline]
    #[must_use]
    pub fn chunk_at_line_break(&self, break_index: u32) -> (&[u8], u32) {
        let Some(offset) = self.try_line_break_to_byte(break_index) else { return (&[], 0) };
        self.chunk_at_byte(offset)
    }

    #[inline]
    #[must_use]
    pub fn line_break_to_byte(&self, break_index: u32) -> u32 {
        self.try_line_break_to_byte(break_index).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_line_break_to_byte(&self, break_index: u32) -> Option<u32> {
        // @Cutnpaste from line_to_byte

        let total_newlines = self.pieces.get(self.root).subtree_newlines;
        if break_index >= total_newlines { return None; }

        let mut current = self.root;
        let mut current_offset = 0;
        let mut current_newlines = 0;

        while current != NIL {
            let node = self.pieces.get(current);
            let p = self.pieces.get_piece(current);
            let left_newlines = self.pieces.get(node.left).subtree_newlines;
            let left_len      = self.pieces.get(node.left).subtree_len;
            let piece_newlines = p.newline_count;

            if break_index < current_newlines + left_newlines {
                //
                // The target line break is somewhere in the left subtree
                //
                current = node.left;

            } else if break_index < current_newlines + left_newlines + piece_newlines {
                //
                // The target line break is inside this piece
                //
                let rel_break = break_index - (current_newlines + left_newlines);
                let absolute_newline_index = p.buffer_start_line + rel_break;

                let absolute_byte_offset = if p.buffer == MOD_BUFFER {
                    self.buffers.modifications_newline_offsets[absolute_newline_index as usize]
                } else {
                    self.buffers.original_buffers[p.buffer].newline_offsets[absolute_newline_index as usize]
                };

                //
                //
                //
                //
                //
                // Unlike `line_to_byte` which adds + 1 to find the start of the *next* line,
                // we omit it here to return the exact byte offset of the newline character itself.
                //
                //
                //
                //
                //
                let local_piece_offset = absolute_byte_offset - p.byte_offset;
                return Some(current_offset + left_len + local_piece_offset);

            } else {
                current_newlines += left_newlines + piece_newlines;
                current_offset   += left_len + p.byte_length;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    #[must_use]
    pub fn line_to_byte(&self, target_line: u32) -> u32 {
        self.try_line_to_byte(target_line).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_line_to_byte(&self, target_line: u32) -> Option<u32> {
        if target_line == 0 { return Some(0) }
        let mut current = self.root;
        let mut current_offset = 0;
        let mut current_line = 0;
        while current != NIL {
            let node = self.pieces.get(current);
            let p = self.pieces.get_piece(current);
            let left_newlines = self.pieces.get(node.left).subtree_newlines;
            let left_len      = self.pieces.get(node.left).subtree_len;
            let piece_newlines = p.newline_count;
            let piece_len      = p.byte_length;

            if target_line <= current_line + left_newlines {
                //
                // The target line starts somewhere in the left subtree
                //
                current = node.left;

            } else if target_line <= current_line + left_newlines + piece_newlines {
                //
                // The target line starts inside this piece
                //

                current_line   += left_newlines;
                current_offset += left_len;

                let rel_line = target_line - current_line; // >= 1, guaranteed
                let absolute_newline_index = p.buffer_start_line + rel_line - 1;
                let absolute_byte_offset = if p.buffer == MOD_BUFFER {
                    self.buffers.modifications_newline_offsets[absolute_newline_index as usize]
                } else {
                    self.buffers.original_buffers[p.buffer].newline_offsets[absolute_newline_index as usize]
                };

                let local_piece_offset = absolute_byte_offset - p.byte_offset + 1;
                return Some(current_offset + local_piece_offset);

            } else {
                current_line   += left_newlines + piece_newlines;
                current_offset += left_len + piece_len;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    #[must_use]
    pub fn char_to_byte(&self, char_index: u32) -> u32 {
        self.try_char_to_byte(char_index).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_char_to_byte(&self, char_index: u32) -> Option<u32> {
        let total_chars = self.len_chars();
        if char_index > total_chars { return None; }
        if char_index == total_chars { return Some(self.len_bytes()); }

        let mut current = self.root;
        let mut current_byte = 0;
        let mut current_char = 0;

        while current != NIL {
            let node = self.pieces.get(current);
            let left_node = self.pieces.get(node.left);

            let left_chars = left_node.subtree_chars;
            let left_bytes = left_node.subtree_len;

            let p = self.pieces.get_piece(current);
            let piece_chars = p.char_count;

            if char_index < current_char + left_chars {
                current = node.left;

            } else if char_index < current_char + left_chars + piece_chars {
                //
                // Target char is inside this piece,
                // p.piece_start_char is the absolute char index of p.offset in its buffer,
                // so (p.piece_start_char + rel_char) is the absolute char target.
                //

                let rel_char = char_index - (current_char + left_chars);
                let absolute_target_byte =
                    self.buffers.char_to_byte_absolute(p.buffer, p.piece_start_char + rel_char);

                return Some(current_byte + left_bytes + (absolute_target_byte - p.byte_offset));

            } else {
                current_char += left_chars + piece_chars;
                current_byte += left_bytes + p.byte_length;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    #[must_use]
    pub fn byte_to_char(&self, byte_offset: u32) -> u32 {
        self.try_byte_to_char(byte_offset).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_byte_to_char(&self, byte_offset: u32) -> Option<u32> {
        let total_bytes = self.len_bytes();
        if byte_offset > total_bytes { return None; }
        if byte_offset == total_bytes { return Some(self.len_chars()); }

        let mut current = self.root;
        let mut current_byte = 0;
        let mut current_char = 0;

        while current != NIL {
            let node = self.pieces.get(current);
            let left_node = self.pieces.get(node.left);

            let left_bytes = left_node.subtree_len;
            let left_chars = left_node.subtree_chars;

            let p = self.pieces.get_piece(current);
            let piece_bytes = p.byte_length;

            if byte_offset < current_byte + left_bytes {
                current = node.left;

            } else if byte_offset < current_byte + left_bytes + piece_bytes {
                // Target byte is inside this piece.
                // byte_to_char_absolute gives the absolute char index, so subtracting
                // piece_start_char converts it to a piece-relative char count.

                let rel_byte = byte_offset - (current_byte + left_bytes);
                let absolute_target_char =
                    self.buffers.byte_to_char_absolute(p.buffer, p.byte_offset + rel_byte);

                return Some(current_char + left_chars + (absolute_target_char - p.piece_start_char));

            } else {
                current_byte += left_bytes + piece_bytes;
                current_char += left_chars + p.char_count;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    #[must_use]
    pub fn byte_to_line(&self, offset: u32) -> u32 {
        self.try_byte_to_line(offset).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_byte_to_line(&self, offset: u32) -> Option<u32> {
        self.try_byte_to_line_col(offset).map(|(l, _)| l)
    }

    #[inline]
    #[must_use]
    pub fn byte_to_line_col(&self, offset: u32) -> (u32, u32) {
        self.try_byte_to_line_col(offset).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_byte_to_line_col(&self, offset: u32) -> Option<(u32, u32)> {
        let total_bytes = self.len_bytes();
        if offset > total_bytes { return None }
        if offset == total_bytes {
            //
            // Get offset of the last line
            //
            let line            = self.pieces.get(self.root).subtree_newlines;
            let line_start_byte = self.try_line_to_byte(line).unwrap_or(0);
            let target_char     = self.len_chars();
            let line_start_char = self.try_byte_to_char(line_start_byte)?;
            return Some((line, target_char - line_start_char));
        }

        let mut current         = self.root;
        let mut current_line    = 0u32;
        let mut current_byte    = 0u32;
        let mut current_char    = 0u32;

        while current != NIL {
            let node          = self.pieces.get(current);
            let left_node     = self.pieces.get(node.left);
            let left_bytes    = left_node.subtree_len;
            let left_newlines = left_node.subtree_newlines;
            let left_chars    = left_node.subtree_chars;
            let p             = self.pieces.get_piece(current);
            let piece_bytes   = p.byte_length;

            if offset < current_byte + left_bytes {
                current = node.left;

            } else if offset < current_byte + left_bytes + piece_bytes {
                current_line += left_newlines;
                current_char += left_chars;

                let rel_byte = offset - (current_byte + left_bytes);

                let (local_newlines, col) = if p.newline_count == 0 {
                    let target_char = self.buffers.byte_to_char_absolute(p.buffer, p.byte_offset + rel_byte);
                    let rel_char    = target_char - p.piece_start_char;
                    let abs_char    = current_char + rel_char;

                    let line_start_byte = self.try_line_to_byte(current_line)?;
                    let line_start_char = self.try_byte_to_char(line_start_byte)?;

                    (0, abs_char - line_start_char)
                } else {
                    let start_index       = p.buffer_start_line as usize;
                    let end_index         = start_index + p.newline_count as usize;
                    let absolute_target = p.byte_offset + rel_byte;

                    let offsets = &self.buffers.get_newlines(p.buffer)[start_index..end_index];

                    let local_nl = offsets.partition_point(|&off| off < absolute_target) as u32;

                    let target_char = self.buffers.byte_to_char_absolute(p.buffer, p.byte_offset + rel_byte);
                    let col = if local_nl > 0 {
                        let last_nl_abs_byte = offsets[local_nl as usize - 1];
                        let last_nl_char     = self.buffers.byte_to_char_absolute(p.buffer, last_nl_abs_byte);
                        target_char - (last_nl_char + 1)
                    } else {
                        let line_start_byte = self.try_line_to_byte(current_line)?;
                        let line_start_char = self.try_byte_to_char(line_start_byte)?;
                        let rel_char        = target_char - p.piece_start_char;
                        (current_char + rel_char) - line_start_char
                    };

                    (local_nl, col)
                };

                return Some((current_line + local_newlines, col));

            } else {
                current_line += left_newlines + p.newline_count;
                current_byte += left_bytes + piece_bytes;
                current_char += left_chars + p.char_count;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    #[must_use]
    pub fn pieces(&self) -> PieceTreeIter<'_> {
        PieceTreeIter::new(&self.pieces, self.root)
    }

    #[inline]
    #[must_use]
    pub fn get_line_range(&self, line: u32) -> (u32, u32) {
        self.try_get_line_range(line).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_get_line_range(&self, line: u32) -> Option<(u32, u32)> {
        let start = self.try_line_to_byte(line)?;
        let end = self.try_line_to_byte(line + 1).unwrap_or_else(|| self.total_length());
        Some((start, end))
    }

    #[inline]
    #[must_use]
    pub fn get_line_content_allocating(&self, line: u32) -> Option<String> {
        let (start, end) = self.try_get_line_range(line)?;

        let mut content = String::with_capacity((end - start) as usize);
        let mut walker = TreeWalker::new(self);

        walker.seek(start);
        while walker.offset < end {
            if let Some(c) = walker.next() {
                content.push(c);
            } else {
                break;
            }
        }

        Some(content)
    }

    /// Byte at a given byte offset. O(log n).
    #[inline]
    #[must_use]
    pub fn byte(&self, offset: u32) -> u8 {
        self.try_byte(offset).unwrap()
    }

    /// Byte at a given byte offset. O(log n).
    #[inline]
    #[must_use]
    pub fn try_byte(&self, offset: u32) -> Option<u8> {
        let (node, rel) = self.find_position(offset, false)?;
        let p = self.get_piece(node);
        let text = self.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);
        text.as_bytes().get(rel as usize).copied()
    }

    /// Char at a given char index. O(log n) to find the piece, then
    /// a short scan within the piece.
    #[inline]
    #[must_use]
    pub fn char(&self, char_index: u32) -> char {
        self.try_char(char_index).unwrap()
    }

    /// Char at a given char index. O(log n) to find the piece, then
    /// a short scan within the piece.
    #[inline]
    #[must_use]
    pub fn try_char(&self, char_index: u32) -> Option<char> {
        let byte_offset = self.try_char_to_byte(char_index)?;
        let (node, rel) = self.find_position(byte_offset, false)?;
        let p = self.get_piece(node);
        let text = self.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);
        text[rel as usize..].chars().next()
    }

    /// Returns a non-allocating iterator of chars over the given char range.
    /// Backed by `TreeWalker::seek` so it reuses existing infrastructure.
    #[inline]
    #[must_use]
    pub fn slice_chars(&self, char_start: u32, char_end: u32) -> SliceChars<'_> {
        let byte_start = self.try_char_to_byte(char_start).unwrap_or(0);
        let byte_end   = self.try_char_to_byte(char_end).unwrap_or_else(|| self.total_length());
        SliceChars {
            walker:   { let mut w = TreeWalker::new(self); w.seek(byte_start); w },
            byte_end,
        }
    }

    /// Returns a non-allocating iterator of chars over the given byte range.
    #[inline]
    #[must_use]
    pub fn slice_bytes(&self, byte_start: u32, byte_end: u32) -> SliceChars<'_> {
        SliceChars {
            walker:   { let mut w = TreeWalker::new(self); w.seek(byte_start); w },
            byte_end,
        }
    }

    /// Non-allocating line view: returns a `ChunkIter` over the byte range of
    /// `line`. Line numbers are 0-based. The trailing \n is included if present.
    #[inline]
    #[must_use]
    pub fn line(&self, line: u32) -> TreeSlice<'_> {
        self.try_line(line).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_line(&self, line: u32) -> Option<TreeSlice<'_>> {
        let start = self.try_line_to_byte(line)?;
        let end = self.try_line_to_byte(line + 1).unwrap_or_else(|| self.len_bytes());

        Some(self.slice(start..end))
    }

    /// Number of lines (= newline count + 1)
    #[inline]
    #[must_use]
    pub fn len_lines(&self) -> u32 { self.pieces.get(self.root).subtree_newlines + 1 }

    #[inline(always)]
    #[must_use]
    pub fn len_chars(&self) -> u32 { self.pieces.get(self.root).subtree_chars }

    #[inline]
    #[must_use]
    pub fn len_bytes(&self) -> u32 { self.total_length() }

    #[inline]
    pub fn remove<R>(&mut self, range: R) where R: RangeBounds<u32> {
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&n) => n + 1,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => self.len_bytes(),
        };
        if end > start { self.remove_at(start, end - start) }
    }

    #[inline]
    #[must_use]
    pub fn char_to_line(&self, char_index: u32) -> u32 {
        self.try_char_to_line(char_index).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_char_to_line(&self, char_index: u32) -> Option<u32> {
        let byte_index = self.try_char_to_byte(char_index)?;
        self.try_byte_to_line_col(byte_index).map(|(line, _)| line)
    }

    #[inline]
    #[must_use]
    pub fn char_to_line_col(&self, char_index: u32) -> (u32, u32) {
        self.try_char_to_line_col(char_index).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_char_to_line_col(&self, char_index: u32) -> Option<(u32, u32)> {
        let byte_index = self.try_char_to_byte(char_index)?;
        self.try_byte_to_line_col(byte_index)
    }

    #[inline]
    #[must_use]
    pub fn line_to_char(&self, line: u32) -> u32 {
        self.try_line_to_char(line).unwrap()
    }

    #[inline]
    #[must_use]
    pub fn try_line_to_char(&self, line: u32) -> Option<u32> {
        let byte_index = self.try_line_to_byte(line)?;
        self.try_byte_to_char(byte_index)
    }
}

#[derive(Debug)]
pub struct PieceTreeIter<'a, const MAX_INLINE_TREE_DEPTH: usize = 32> {
    arena: &'a Pieces,
    stack: SmallVec<[NodeRef; MAX_INLINE_TREE_DEPTH]>,
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> PieceTreeIter<'a, MAX_INLINE_TREE_DEPTH> {
    #[inline]
    #[must_use]
    pub fn new(arena: &'a Pieces, mut root: NodeRef) -> Self {
        let mut stack = SmallVec::new();
        while root != NIL {
            stack.push(root);
            root = arena.get(root).left;
        }

        Self { arena, stack }
    }
}

impl<const MAX_INLINE_TREE_DEPTH: usize> Iterator for PieceTreeIter<'_, MAX_INLINE_TREE_DEPTH> {
    type Item = (NodeRef, Piece);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let node_index = self.stack.pop()?;
        let node = self.arena.get(node_index);
        let p = self.arena.get_piece(node_index);

        let mut current = node.right;
        while current != NIL {
            self.stack.push(current);
            current = self.arena.get(current).left;
        }

        Some((node_index, p))
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Direction { Left, Center, Right }

#[derive(Debug)]
pub struct TreeWalker<'a, const MAX_INLINE_TREE_DEPTH: usize = 32> {
    tree: &'a PieceTree,
    stack: SmallVec<[(NodeRef, Direction); MAX_INLINE_TREE_DEPTH]>,
    current_str: str::Chars<'a>,
    pub offset: u32,
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> TreeWalker<'a, MAX_INLINE_TREE_DEPTH> {
    #[inline]
    #[must_use]
    pub fn new(tree: &'a PieceTree) -> Self {
        let mut walker = Self {
            tree,
            stack: SmallVec::new(),
            current_str: "".chars(),
            offset: 0,
        };
        if tree.root != NIL {
            walker.stack.push((tree.root, Direction::Left));
        }
        walker.populate_chars();
        walker
    }

    #[inline]
    pub fn seek(&mut self, target: u32) {
        self.stack.clear();
        self.offset = target;
        self.current_str = "".chars();

        if self.tree.root == NIL { return; }

        let mut current = self.tree.root;
        let mut current_offset = target;

        while current != NIL {
            let node = self.tree.pieces.get(current);
            let p = self.tree.pieces.get_piece(current);
            let left_len = self.tree.pieces.get(node.left).subtree_len;
            let piece_len = p.byte_length;

            if current_offset < left_len {
                self.stack.push((current, Direction::Center));
                current = node.left;
            } else if current_offset < left_len + piece_len {
                self.stack.push((current, Direction::Right));
                let text = self.tree.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);
                let rel_offset = current_offset - left_len;
                self.current_str = text[rel_offset as usize..].chars();
                break;
            } else {
                current_offset -= left_len + piece_len;
                current = node.right;
            }
        }
    }

    #[inline]
    fn populate_chars(&mut self) {
        while let Some((node_index, dir)) = self.stack.pop() {
            let node = self.tree.pieces.get(node_index);
            match dir {
                Direction::Left => {
                    self.stack.push((node_index, Direction::Center));
                    if node.left != NIL { self.stack.push((node.left, Direction::Left)); }
                }

                Direction::Center => {
                    self.stack.push((node_index, Direction::Right));
                    let p = self.tree.pieces.get_piece(node_index);
                    let text = self.tree.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);
                    self.current_str = text.chars();
                    return;
                }

                Direction::Right => {
                    if node.right != NIL { self.stack.push((node.right, Direction::Left)); }
                }
            }
        }
    }
}

impl Iterator for TreeWalker<'_> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<char> {
        loop {
            if let Some(c) = self.current_str.next() {
                self.offset += c.len_utf8() as u32;
                return Some(c);
            }

            if self.stack.is_empty() { return None; }
            self.populate_chars();
        }
    }
}

#[derive(Debug)]
pub struct ReverseTreeWalker<'a, const MAX_INLINE_TREE_DEPTH: usize = 32> {
    tree: &'a PieceTree,
    stack: SmallVec<[(NodeRef, bool); MAX_INLINE_TREE_DEPTH]>,
    current_str: str::Chars<'a>,
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> ReverseTreeWalker<'a, MAX_INLINE_TREE_DEPTH> {
    #[inline(always)]
    #[must_use]
    pub fn new(tree: &'a PieceTree) -> Self {
        let mut walker = Self {
            tree,
            stack: SmallVec::new(),
            current_str: "".chars(),
        };
        walker.push_rightmost(tree.root);
        walker
    }

    #[inline]
    #[must_use]
    pub fn new_at(tree: &'a PieceTree, target_offset: u32) -> Self {
        let mut walker = Self {
            tree,
            stack: SmallVec::new(),
            current_str: "".chars(),
        };
        walker.seek(target_offset);
        walker
    }

    #[inline(always)]
    fn push_rightmost(&mut self, mut node: NodeRef) {
        while node != NIL {
            self.stack.push((node, false));
            node = self.tree.pieces.get(node).right;
        }
    }

    #[inline]
    pub fn seek(&mut self, mut target_offset: u32) {
        self.stack.clear();
        self.current_str = "".chars();

        if self.tree.root == NIL { return; }

        let total_bytes = self.tree.len_bytes();

        if target_offset > total_bytes {
            target_offset = total_bytes;
        }
        if target_offset == total_bytes {
            self.push_rightmost(self.tree.root);
            return;
        }

        let mut current = self.tree.root;
        let mut current_offset = target_offset;

        while current != NIL {
            let node = self.tree.pieces.get(current);
            let p = self.tree.pieces.get_piece(current);
            let left_len = self.tree.pieces.get(node.left).subtree_len;
            let piece_len = p.byte_length;

            if current_offset < left_len {
                self.stack.push((current, true));
                current = node.left;

            } else if current_offset < left_len + piece_len {
                self.stack.push((current, true));

                let text = self.tree.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);
                let rel_offset = current_offset - left_len;

                // Keep the subset of the string before the offset
                self.current_str = text[..rel_offset as usize].chars();

                break;

            } else {
                self.stack.push((current, false));
                current_offset -= left_len + piece_len;
                current = node.right;
            }
        }
    }
}

impl Iterator for ReverseTreeWalker<'_> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.current_str.next_back() { return Some(c); }

        while let Some((node_index, visited_right)) = self.stack.pop() {
            let node = self.tree.pieces.get(node_index);
            if visited_right {
                self.push_rightmost(node.left);
            } else {
                self.stack.push((node_index, true));

                let p = self.tree.pieces.get_piece(node_index);
                let text_slice = self.tree.buffers.get_slice(p.buffer, p.byte_offset, p.byte_length);

                self.current_str = text_slice.chars();

                if let Some(c) = self.current_str.next_back() { return Some(c); }
            }
        }

        None
    }
}

impl PieceTree {
    #[inline]
    pub fn debug(&self, f: &mut impl core::fmt::Write) -> core::fmt::Result {
        writeln!(f, "\n--- Tree State (Root: {:?}) ---", self.root)?;
        self.print_inorder(f, self.root, &mut None, 0)?;
        writeln!(f, "------------------------------\n")
    }

    fn print_inorder(&self, f: &mut impl core::fmt::Write, node: NodeRef, last: &mut Option<Piece>, depth: usize) -> core::fmt::Result {
        if node == NIL { return Ok(()) }

        let n = self.pieces.nodes[node];

        self.print_inorder(f, n.left, last, depth + 1)?;

        let cur = self.get_piece(node);

        //
        // Check for the mergeable invariant
        //
        let warning = if let Some(prev) = last {
            if prev.buffer == cur.buffer
               && prev.byte_offset + prev.byte_length == cur.byte_offset
            {
                " --------- [!!! MERGEABLE NEIGHBORS NOT MERGED !!!] ---------"
            } else {
                ""
            }
        } else {
            ""
        };

        let Piece { buffer, byte_offset: offset, byte_length: length, .. } = cur;

        writeln!(
            f,
            "{:indent$}Node {node:?}: Buf={}, Off={offset}, Len={length}{warning}",
            "", buffer.as_u32(), indent = depth * 4
        )?;

        *last = Some(cur);

        self.print_inorder(f, n.right, last, depth + 1)
    }
}

pub fn assert_state(tree: &PieceTree, expected: &str) {
    let tree_text = tree.to_string();

    assert_eq!(tree_text, expected, "Text mismatch");

    if !expected.is_empty() {
        let offsets = [0, expected.len() / 2, expected.len() - 1];
        for off in offsets {
            let chunk = tree.read_largest_contigous_chunk_at_byte(off as u32).0;
            let chunk_str = str::from_utf8(chunk).unwrap();
            assert!(
                expected[off..].starts_with(chunk_str),
                "Chunk mismatch at offset {}",
                off
            );
        }
    }
}

pub fn assert_invariants(tree: &PieceTree) {
    fn check(tree: &PieceTree, node: NodeRef) -> (usize, usize) {
        if node == NIL { return (0, 1) }

        let n = tree.pieces.nodes[node];
        let piece = tree.get_piece(node);

        let (l_len, l_bh) = check(tree, n.left);
        let (r_len, r_bh) = check(tree, n.right);

        let expected_len = l_len + piece.byte_length as usize + r_len;
        assert_eq!(n.subtree_len as usize, expected_len, "subtree_len mismatch");

        if n.color() == Color::Red {
            assert_eq!(
                tree.pieces.nodes[n.left].color(),
                Color::Black,
                "red node with red left child"
            );
            assert_eq!(
                tree.pieces.nodes[n.right].color(),
                Color::Black,
                "red node with red right child"
            );
        }

        assert_eq!(l_bh, r_bh, "black height mismatch");

        let bh = l_bh + usize::from(n.color() == Color::Black);
        (expected_len, bh)
    }

    if tree.root == NIL {
        assert_eq!(tree.total_length(), 0,            "Empty tree has nonzero length");
    } else {
        assert_eq!(
            tree.pieces.nodes[tree.root].color(),
            Color::Black,
            "root is not black"
        );
        let (len, _) = check(tree, tree.root);
        assert_eq!(len, tree.total_length() as usize, "Tree length and root length differ");

    }
}

pub fn assert_piece_metadata(tree: &PieceTree) {
    fn collect(tree: &PieceTree, node: NodeRef, out: &mut Vec<Piece>) {
        if node == NIL { return }

        let n = tree.pieces.nodes[node];

        collect(tree, n.left, out);
        out.push(tree.pieces.get_piece(node));
        collect(tree, n.right, out);
    }

    let mut pieces = Vec::new();
    collect(tree, tree.root, &mut pieces);

    for p in &pieces {
        let buf   = tree.buffers.get(p.buffer);
        let start = p.byte_offset as usize;
        let end   = start + p.byte_length as usize;

        assert!(end <= buf.len(), "Piece points past end of buffer");
        assert!(p.byte_length > 0, "Zero-length piece found");

        let slice = &buf[start..end];
        let (actual_chars, actual_nl) = count_chars_and_newlines(slice.as_bytes());
        assert_eq!(p.char_count,    actual_chars, "char_count mismatch");
        assert_eq!(p.newline_count, actual_nl,    "newline_count mismatch");

        //
        // piece_start_char and buffer_start_line: scan only the prefix once
        //
        let prefix = &buf.as_bytes()[..p.byte_offset as usize];
        let actual_start_char = bytecount::num_chars(prefix) as u32;
        let actual_start_line = bytecount::count(prefix, b'\n') as u32;

        assert_eq!(p.piece_start_char,  actual_start_char,
            "piece_start_char mismatch buffer={} offset={}", p.buffer.as_u32(), p.byte_offset);

        assert_eq!(p.buffer_start_line, actual_start_line,
            "buffer_start_line mismatch buffer={} offset={}", p.buffer.as_u32(), p.byte_offset);

        //
        // Verify the newline_offsets slice
        //
        let nl_offsets = &tree.buffers.get_newlines(p.buffer)
            [p.buffer_start_line as usize..(p.buffer_start_line + p.newline_count) as usize];

        for (i, &abs_byte) in nl_offsets.iter().enumerate() {
            assert!(
                abs_byte >= p.byte_offset && abs_byte < p.byte_offset + p.byte_length,
                "newline_offsets[{}] outside piece", p.buffer_start_line as usize + i
            );

            assert_eq!(
                buf.as_bytes()[abs_byte as usize], b'\n',
                "newline_offsets[{}] not a newline", p.buffer_start_line as usize + i
            );
        }
    }
}

pub fn assert_no_mergeable_neighbors(tree: &PieceTree) {
    fn inorder(tree: &PieceTree, node: NodeRef, last: &mut Option<Piece>) {
        if node == NIL { return }

        let n = tree.pieces.nodes[node];
        inorder(tree, n.left, last);

        let cur = tree.get_piece(node);

        if let Some(prev) = *last {
            if prev.buffer == cur.buffer
                && prev.byte_offset + prev.byte_length == cur.byte_offset
            {
                panic!("Mergeable neighboring pieces were left unmerged");
            }
        }

        *last = Some(cur);
        inorder(tree, n.right, last);
    }

    let mut last = None;
    inorder(tree, tree.root, &mut last);
}

pub fn assert_coordinates(tree: &PieceTree, oracle: &str) {
    #[inline]
    fn oracle_offset_to_line_col(s: &str, byte_offset: usize) -> (usize, usize) {
        let slice = &s[..byte_offset];
        let line  = bytecount::count(slice.as_bytes(), b'\n');
        let last_nl = slice.rfind('\n').map_or(0, |i| i + 1);
        let col   = bytecount::num_chars(s[last_nl..byte_offset].as_bytes());
        (line, col)
    }

    #[inline]
    fn oracle_line_to_offset(s: &str, target_line: usize) -> usize {
        if target_line == 0 { return 0 }

        let mut line = 0;
        for (i, c) in s.char_indices() {
            if c == '\n' {
                line += 1;
                if line == target_line { return i + 1; }
            }
        }

        s.len()
    }

    fn check_coordinate(tree: &PieceTree, oracle: &str, byte_index: u32) {
        let char_index = bytecount::num_chars(oracle[..byte_index as usize].as_bytes()) as u32;

        assert_eq!(tree.char_to_byte(char_index), byte_index,
                   "char_to_byte({}) wrong", char_index);

        assert_eq!(tree.byte_to_char(byte_index), char_index,
                   "byte_to_char({}) wrong", byte_index);

        let expected_lc = oracle_offset_to_line_col(oracle, byte_index as usize);
        assert_eq!(
            tree.byte_to_line_col(byte_index),
            (expected_lc.0 as u32, expected_lc.1 as u32),
            "offset_to_line_col({}) wrong", byte_index
        );
    }

    let total_bytes = oracle.len() as u32;
    let total_chars = oracle.chars().count() as u32;

    //
    // Always check these
    //
    check_coordinate(tree, oracle, 0);
    if total_bytes > 0 {
        check_coordinate(tree, oracle, total_bytes - 1);
        check_coordinate(tree, oracle, total_bytes / 2);
    }


    //
    // Sample ~8 positions spread across the document
    //
    for i in 0u32..8 {
        let byte =
            ((total_bytes as u64 * ((i as u64 * 2_654_435_761) & 0xFFFF_FFFF)) >> 32) as u32 % total_bytes.max(1);

        // Snap to char boundary
        let byte = oracle.as_bytes()[..byte as usize]
            .iter().rposition(|&b| !(0x80..0xC0).contains(&b))
            .map_or(0, |i| i as u32);

        check_coordinate(tree, oracle, byte);
    }

    //
    // EOF and out-of-bounds always
    //
    assert_eq!(tree.char_to_byte(total_chars), total_bytes);
    assert_eq!(tree.byte_to_char(total_bytes), total_chars);
    assert_eq!(tree.try_char_to_byte(total_chars + 1), None);
    assert_eq!(tree.try_byte_to_char(total_bytes + 1), None);
    assert_eq!(tree.try_byte_to_line_col(total_bytes + 1), None);

    //
    // line_to_offset: check first, last, middle line only
    //
    let total_lines = oracle.chars().filter(|&c| c == '\n').count() + 1;
    for line in [0, total_lines / 2, total_lines.saturating_sub(1)] {
        let expected = oracle_line_to_offset(oracle, line) as u32;
        assert_eq!(tree.try_line_to_byte(line as u32), Some(expected),
            "line_to_offset({}) wrong", line);
    }

    assert_eq!(tree.try_line_to_byte(total_lines as u32), None);
}
