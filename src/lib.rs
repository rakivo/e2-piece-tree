#![cfg_attr(not(feature = "write"), no_std)]

#[cfg(feature = "write")]
extern crate std;

extern crate alloc;

#[allow(unused)]
use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;

use core::str;
use core::cmp::Ordering;
use core::ops::{Bound, RangeBounds};

use smallvec::SmallVec;
use cranelift_entity::{EntityRef, PrimaryMap};

pub const MOD_BUFFER_INDEX: u32 = u32::MAX >> 1;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct NodeRef(pub u32);

impl EntityRef for NodeRef {
    #[inline(always)] fn new(index: usize) -> Self { Self(index as u32) }
    #[inline(always)] fn index(self)       -> usize { self.0 as usize }
}

pub const NIL: NodeRef = NodeRef(0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color { Black = 0, Red = 1 }

#[derive(Clone, Copy, Debug)]
pub struct HistoryEntry {
    pub root: NodeRef,
    pub cursor_offset: u32,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub buffer_index:  u32,
    pub offset:        u32,
    pub length:        u32,
    pub newline_count: u32,
    pub char_count:    u32,
}

// For tests and other stuff
#[derive(Clone, Copy, Debug)]
pub enum Edit {
    Insert { offset: u32, text: &'static str },
    Remove { offset: u32, length: u32 },
}

impl Edit {
    #[inline(always)]
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
    pub left: NodeRef,          // 4 bytes
    pub right: NodeRef,         // 4 bytes
    pub offset: u32,            // 4 bytes
    pub subtree_len: u32,       // 4 bytes
    pub subtree_chars: u32,     // 4 bytes
    pub subtree_newlines: u32,  // 4 bytes
    pub meta: u32,              // 4 bytes (Bit 0: Color, Bits 1..31: BufferIndex)
    pub _pad: u32,              // 4 bytes
}

const _: () = assert!(size_of::<Node>() == 32);

impl Node {
    #[inline(always)]
    pub fn color(&self) -> Color {
        if (self.meta & 1) == 1 { Color::Red } else { Color::Black }
    }

    #[inline(always)]
    pub fn set_color(&mut self, color: Color) {
        self.meta = (self.meta & !1) | (color as u32);
    }

    #[inline(always)]
    pub fn buffer_index(&self) -> u32 {
        self.meta >> 1
    }

    #[inline(always)]
    pub fn set_buffer_index(&mut self, index: u32) {
        self.meta = (index << 1) | (self.meta & 1);
    }
}

pub struct Buffers {
    pub original_buffers: Vec<String>,
    pub modifications_buffer: String,
}

impl Default for Buffers {
    fn default() -> Self {
        Self {
            original_buffers: Vec::new(),
            modifications_buffer: String::with_capacity(1024 * 64),
        }
    }
}

impl Buffers {
    #[inline(always)]
    pub fn new() -> Self { Self::default() }

    #[inline(always)]
    pub fn get_slice(&self, index: u32, offset: u32, len: u32) -> &str {
        let buf = if index == MOD_BUFFER_INDEX {
            &self.modifications_buffer
        } else {
            unsafe { self.original_buffers.get_unchecked(index as usize) }
        };
        let start = offset as usize;
        let end = start + len as usize;
        unsafe { str::from_utf8_unchecked(buf.as_bytes().get_unchecked(start..end)) }
    }

    #[inline(always)]
    pub fn count_chars(&self, index: u32, offset: u32, len: u32) -> u32 {
        self.get_slice(index, offset, len).chars().count() as u32
    }

    #[inline(always)]
    pub fn count_newlines(&self, index: u32, offset: u32, len: u32) -> u32 {
        let bytes = self.get_slice(index, offset, len).as_bytes();
        bytecount::count(bytes, b'\n') as _
    }
}

pub struct Pieces {
    pub nodes: PrimaryMap<NodeRef, Node>,
}

impl Default for Pieces {
    fn default() -> Self {
        Self::new()
    }
}

impl Pieces {
    pub fn new() -> Self {
        let mut nodes = PrimaryMap::new();
        nodes.push(Node {
            left: NIL, right: NIL, offset: 0,
            subtree_len: 0, subtree_chars: 0, subtree_newlines: 0,
            meta: 0, _pad: 0,
        });
        Self { nodes }
    }

    #[inline(always)]
    pub fn get(&self, index: NodeRef) -> &Node {
        &self.nodes[index]
    }

    #[inline(always)]
    pub fn get_piece(&self, index: NodeRef) -> Piece {
        if index == NIL { return Piece::default(); }

        let node = &self.nodes[index];
        let l = &self.nodes[node.left];
        let r = &self.nodes[node.right];

        Piece {
            buffer_index: node.buffer_index(),
            offset: node.offset,
            length: node.subtree_len - l.subtree_len - r.subtree_len,
            char_count: node.subtree_chars - l.subtree_chars - r.subtree_chars,
            newline_count: node.subtree_newlines - l.subtree_newlines - r.subtree_newlines,
        }
    }

    #[inline(always)]
    pub fn alloc(&mut self, color: Color, left: NodeRef, piece: Piece, right: NodeRef) -> NodeRef {
        let l = &self.nodes[left];
        let r = &self.nodes[right];

        let mut node = Node {
            left, right,
            offset: piece.offset,
            subtree_len: l.subtree_len + piece.length + r.subtree_len,
            subtree_chars: l.subtree_chars + piece.char_count + r.subtree_chars,
            subtree_newlines: l.subtree_newlines + piece.newline_count + r.subtree_newlines,
            meta: 0, _pad: 0,
        };
        node.set_color(color);
        node.set_buffer_index(piece.buffer_index);

        self.nodes.push(node)
    }

    #[inline]
    pub fn insert_node(&mut self, root: NodeRef, piece: Piece, at: u32) -> NodeRef {
        let new_root = self.ins(root, piece, at, 0);
        let r_node = self.nodes[new_root];
        let p = self.get_piece(new_root);
        self.alloc(Color::Black, r_node.left, p, r_node.right)
    }

    #[inline]
    fn ins(&mut self, root: NodeRef, p: Piece, at: u32, total_offset: u32) -> NodeRef {
        if root == NIL { return self.alloc(Color::Red, NIL, p, NIL); }

        let node = self.nodes[root];
        let node_piece = self.get_piece(root);
        let left_len = self.nodes[node.left].subtree_len as u32;

        if at < total_offset + left_len + (node_piece.length as u32) {
            let lft = self.ins(node.left, p, at, total_offset);
            self.balance(node.color(), lft, node_piece, node.right)
        } else {
            let next_offset = total_offset + left_len + (node_piece.length as u32);
            let rgt = self.ins(node.right, p, at, next_offset);
            self.balance(node.color(), node.left, node_piece, rgt)
        }
    }

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
                let next_total = total + left_len + node_piece.length;
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
    fn fuse(&mut self, left: NodeRef, right: NodeRef) -> NodeRef {
        if left  == NIL { return right }
        if right == NIL { return left }

        let l_node = self.nodes[left];
        let r_node = self.nodes[right];

        if l_node.color() == Color::Black && r_node.color() == Color::Red {
            let fused = self.fuse(left, r_node.left);
            return self.alloc(Color::Red, fused, self.get_piece(right), r_node.right);
        }

        if l_node.color() == Color::Red && r_node.color() == Color::Black {
            let fused = self.fuse(l_node.right, right);
            return self.alloc(Color::Red, l_node.left, self.get_piece(left), fused);
        }

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

    fn balance_left(&mut self, left: NodeRef) -> NodeRef {
        let l_node = self.nodes[left];
        let ll_node = self.nodes[l_node.left];
        let lr_node = self.nodes[l_node.right];

        if l_node.left != NIL && ll_node.color() == Color::Red {
            let new_ll = self.alloc(Color::Black, ll_node.left, self.get_piece(l_node.left), ll_node.right);
            return self.alloc(Color::Red, new_ll, self.get_piece(left), l_node.right);
        }

        if l_node.right != NIL && lr_node.color() == Color::Black {
            let new_lr = self.alloc(Color::Red, lr_node.left, self.get_piece(l_node.right), lr_node.right);
            let new_l = self.alloc(Color::Black, l_node.left, self.get_piece(left), new_lr);
            let nl = self.nodes[new_l];
            return self.balance(Color::Black, nl.left, self.get_piece(new_l), nl.right);
        }

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

    fn balance_right(&mut self, right: NodeRef) -> NodeRef {
        let r_node = self.nodes[right];
        let rl_node = self.nodes[r_node.left];
        let rr_node = self.nodes[r_node.right];

        if r_node.right != NIL && rr_node.color() == Color::Red {
            let new_rr = self.alloc(Color::Black, rr_node.left, self.get_piece(r_node.right), rr_node.right);
            return self.alloc(Color::Red, r_node.left, self.get_piece(right), new_rr);
        }

        if r_node.left != NIL && rl_node.color() == Color::Black {
            let new_rl = self.alloc(Color::Red, rl_node.left, self.get_piece(r_node.left), rl_node.right);
            let new_r = self.alloc(Color::Black, new_rl, self.get_piece(right), r_node.right);
            let nr = self.nodes[new_r];
            return self.balance(Color::Black, nr.left, self.get_piece(new_r), nr.right);
        }

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
    pub fn find_offset(
        &self,
        mut root: NodeRef, mut target_offset: u32, prefer_left: bool
    ) -> Option<(NodeRef, u32)> {
        while root != NIL {
            let node = &self.nodes[root];
            let left_len = self.nodes[node.left].subtree_len;
            let piece_len = self.get_piece(root).length;

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

#[derive(Default)]
pub struct PieceTree {
    pub pieces:  Pieces,
    pub buffers: Buffers,

    pub root: NodeRef,

    pub undo_stack: Vec<HistoryEntry>,
    pub redo_stack: Vec<HistoryEntry>,

    pub scratch_index_map: Vec<u32>,
}

impl PieceTree {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn commit_head(&mut self, cursor_offset: u32) {
        self.undo_stack.push(HistoryEntry { root: self.root, cursor_offset });
        self.redo_stack.clear();
    }

    #[inline(always)]
    pub fn try_undo(&mut self, current_cursor: u32) -> Option<u32> {
        if let Some(entry) = self.undo_stack.pop() {
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
            self.undo_stack.push(HistoryEntry {
                root: self.root, cursor_offset: current_cursor
            });
            self.root = entry.root;
            return Some(entry.cursor_offset);
        }

        None
    }

    #[inline(always)]
    pub fn chars_rev(&self) -> ReverseTreeWalker<'_> { ReverseTreeWalker::new(self) }

    #[inline(always)]
    pub fn total_length(&self) -> u32 {
        self.pieces.get(self.root).subtree_len
    }

    #[inline(always)]
    pub fn apply_edits(&mut self, primary_cursor_offset: u32, edits: &mut [Edit]) {
        if edits.is_empty() { return }

        edits.sort_by(|a, b| b.offset().cmp(&a.offset()));
        self.commit_head(primary_cursor_offset);

        for edit in edits {
            match edit {
                Edit::Insert { offset, text } => self.insert_no_commit(*offset, text),
                Edit::Remove { offset, length } => self.remove_no_commit(*offset, *length),
            }
        }
    }

    #[inline(always)]
    pub fn insert(&mut self, offset: u32, text: &str) {
        if text.is_empty() { return }

        self.commit_head(offset);
        self.insert_no_commit(offset, text);
    }

    pub fn insert_no_commit(&mut self, offset: u32, text: &str) {
        let mod_offset = self.buffers.modifications_buffer.len() as u32;
        self.buffers.modifications_buffer.push_str(text);

        let newline_count = bytecount::count(text.as_bytes(), b'\n') as u32;
        let char_count = text.chars().count() as u32;

        let new_piece = Piece {
            buffer_index: MOD_BUFFER_INDEX,
            offset: mod_offset,
            length: text.len() as u32,
            newline_count,
            char_count,
        };

        if self.root == NIL {
            self.root = self.pieces.insert_node(self.root, new_piece, offset);
            return;
        }

        let Some((node_index, rel_offset)) = self.find_position(offset, true) else {
            self.root = self.pieces.insert_node(self.root, new_piece, offset);
            return;
        };

        let p = self.pieces.get_piece(node_index);
        let start_offset = offset - rel_offset;

        //
        // End of an existing mod-buffer piece, contiguous in buffer -> extend it
        //
        if rel_offset == p.length && p.buffer_index == MOD_BUFFER_INDEX {
            if p.offset + p.length == new_piece.offset {
                self.root = self.pieces.remove_node(self.root, start_offset);

                let mut extended = p;
                extended.length        += new_piece.length;
                extended.newline_count += new_piece.newline_count;
                extended.char_count    += new_piece.char_count;

                self.root = self.pieces.insert_node(self.root, extended, start_offset);
                return;
            }
        }

        //
        // Start of a node - check left neighbor
        //
        if rel_offset == 0 && offset > 0 {
            if let Some((prev_index, prev_rel)) = self.find_position(offset - 1, false) {
                let prev = self.pieces.get_piece(prev_index);
                let prev_start = (offset - 1) - prev_rel;
                if prev.buffer_index == MOD_BUFFER_INDEX
                    && prev.offset + prev.length == new_piece.offset
                {
                    self.root = self.pieces.remove_node(self.root, prev_start);

                    let mut extended = prev;
                    extended.length        += new_piece.length;
                    extended.newline_count += new_piece.newline_count;
                    extended.char_count    += new_piece.char_count;

                    self.root = self.pieces.insert_node(self.root, extended, prev_start);
                    return;
                }
            }
        }

        if rel_offset == 0 || rel_offset == p.length {
            self.root = self.pieces.insert_node(self.root, new_piece, offset);
            return;
        }

        //
        // Split
        //
        let left_len = rel_offset as u32;
        let left_nl = self.buffers.count_newlines(p.buffer_index, p.offset, left_len);
        let left_chars = self.buffers.count_chars(p.buffer_index, p.offset, left_len);

        let left = Piece {
            buffer_index: p.buffer_index,
            offset: p.offset,
            length: left_len,
            newline_count: left_nl,
            char_count: left_chars,
        };

        let right = Piece {
            buffer_index: p.buffer_index,
            offset: p.offset + left_len,
            length: p.length - left_len,
            newline_count: p.newline_count - left_nl,
            char_count: p.char_count - left_chars,
        };

        self.root = self.pieces.remove_node(self.root, start_offset);
        self.root = self.pieces.insert_node(self.root, left, start_offset);
        self.root = self.pieces.insert_node(self.root, new_piece, start_offset + left.length);
        self.root = self.pieces.insert_node(self.root, right, start_offset + left.length + new_piece.length);
    }

    #[inline(always)]
    pub fn remove_at(&mut self, offset: u32, length: u32) {
        if length == 0 || self.root == NIL { return }

        self.commit_head(offset);
        self.remove_no_commit(offset, length);
    }

    pub fn remove_no_commit(&mut self, offset: u32, mut length: u32) {
        let total = self.total_length();
        if offset >= total { return }
        if offset + length > total { length = total - offset }

        let mut remaining = length;
        let current_offset   = offset;

        while remaining > 0 {
            let Some((
                node_index, rel_offset
            )) = self.find_position(current_offset, false) else { break };

            let p = self.pieces.get_piece(node_index);
            if rel_offset == p.length { break }

            let piece_start        = current_offset - rel_offset;
            let left_len           = rel_offset;
            let right_delete_start = rel_offset + remaining;

            self.root = self.pieces.remove_node(self.root, piece_start);

            if left_len > 0 {
                let left = Piece {
                    buffer_index:  p.buffer_index,
                    offset:        p.offset,
                    length:        left_len,
                    newline_count: self.buffers.count_newlines(p.buffer_index, p.offset, left_len),
                    char_count:    self.buffers.count_chars(p.buffer_index, p.offset, left_len),
                };
                self.root = self.pieces.insert_node(self.root, left, piece_start);
            }

            if right_delete_start < p.length {
                //
                // Deletion ends inside this piece - insert right stump and done
                //
                let right_len    = p.length - right_delete_start;
                let right_offset = p.offset + right_delete_start;
                let right = Piece {
                    buffer_index:  p.buffer_index,
                    offset:        right_offset,
                    length:        right_len,
                    newline_count: self.buffers.count_newlines(p.buffer_index, right_offset, right_len),
                    char_count:    self.buffers.count_chars(p.buffer_index, right_offset, right_len),
                };
                let right_insert_pos = piece_start + left_len;
                self.root = self.pieces.insert_node(self.root, right, right_insert_pos);

                //
                // Check both new boundaries
                //
                self.try_merge_at(piece_start);                   // Left  stump vs its left neighbor
                self.try_merge_at(right_insert_pos + right_len);  // Right stump vs its right neighbor
                break;
            }

            //
            // Deletion spans this entire piece (or its tail) - consumed it fully
            //
            let end_clamp = core::cmp::min(p.length, right_delete_start);
            let deleted   = end_clamp - left_len;
            if deleted == 0 { break }
            remaining -= deleted;

            //
            // The predecessor (or left stump) is now adjacent to whatever comes next,
            // check at current_offset - this is the new boundary exposed after full removal.
            //
            self.try_merge_at(current_offset);
        }
    }

    fn try_merge_at(&mut self, pos: u32) {
        if pos == 0 { return }

        let left_pos = pos - 1;

        let (left_index, left_rel) = match self.find_position(left_pos, false) {
            Some(x) => x,
            None    => return,
        };
        let (right_index, right_rel) = match self.find_position(pos, false) {
            Some(x) => x,
            None    => return,
        };

        if right_rel != 0 { return }

        let left  = self.pieces.get_piece(left_index);
        let right = self.pieces.get_piece(right_index);

        if left.buffer_index         != MOD_BUFFER_INDEX { return }
        if right.buffer_index        != MOD_BUFFER_INDEX { return }
        if left.offset + left.length != right.offset     { return }

        let left_start = left_pos - left_rel;

        //
        // Remove left, then right (which is now at left_start after left is gone)
        //
        self.root = self.pieces.remove_node(self.root, left_start);
        self.root = self.pieces.remove_node(self.root, left_start);

        let mut merged = left;
        merged.length        += right.length;
        merged.newline_count += right.newline_count;
        merged.char_count    += right.char_count;

        self.root = self.pieces.insert_node(self.root, merged, left_start);
    }

    #[inline(always)]
    pub fn to_string_allocating(&self) -> String {
        let mut result = String::with_capacity(self.total_length() as usize);
        let walker = TreeWalker::new(self);
        result.extend(walker.into_iter());
        result
    }

    #[inline(always)]
    pub fn get_piece(&self, index: NodeRef) -> Piece {
        self.pieces.get_piece(index)
    }

    #[inline]
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
            let len = self.pieces.get_piece(last_valid).length as u32;
            return Some((last_valid, len));
        }

        self.pieces.find_offset(self.root, offset, prefer_left)
    }

    #[cfg(feature = "write")]
    /// Efficiently streams the logical document to any `std::io::Write` target (File, stdout, Network, etc).
    /// Bypasses `TreeWalker`'s char-by-char decoding to write raw byte slices in bulk.
    #[inline]
    pub fn write_to<W: std::io::Write>(&self, mut writer: W) -> std::io::Result<()> {
        if self.root == NIL {
            return Ok(());
        }

        for (_, piece) in self.pieces() {
            let slice = self.buffers.get_slice(piece.buffer_index, piece.offset, piece.length);
            writer.write_all(slice.as_bytes())?;
        }

        writer.flush()
    }
}

pub struct LinesIter<'a> {
    tree: &'a PieceTree,
    current_line: u32,
    total_lines: u32,
}

impl<'a> Iterator for LinesIter<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_line >= self.total_lines { return None }

        let line = self.tree.get_line_content_allocating(self.current_line)?;
        self.current_line += 1;
        Some(line)
    }
}

/// A lazy, non-allocating slice view over a byte range of the tree.
/// Yields chunks (borrowed directly from buffers) via iteration.
pub struct ChunkIter<'a> {
    tree:       &'a PieceTree,
    pieces:     PieceTreeIter<'a>,
    // byte range we care about
    start:      u32,
    end:        u32,
    // bytes consumed so far across all pieces
    piece_start: u32,
}

impl<'a> ChunkIter<'a> {
    #[inline]
    pub fn new(tree: &'a PieceTree, start: u32, end: u32) -> Self {
        Self {
            tree,
            pieces: PieceTreeIter::new(&tree.pieces, tree.root),
            start,
            end,
            piece_start: 0,
        }
    }
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        loop {
            let (_, p) = self.pieces.next()?;
            let piece_end = self.piece_start + p.length;

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
            let slice_end   = (self.end - self.piece_start).min(p.length);

            self.piece_start = piece_end;

            let text = self.tree.buffers.get_slice(p.buffer_index, p.offset + slice_start, slice_end - slice_start);
            if text.is_empty() { continue; }
            return Some(text);
        }
    }
}

impl PieceTree {
    /// Returns an iterator of &str chunks over the given byte range.
    /// Zero allocation. Analogous to ropey's chunks_at_byte.
    #[inline]
    pub fn chunks(&self, start: u32, end: u32) -> ChunkIter<'_> {
        let end = end.min(self.total_length());
        ChunkIter::new(self, start, end)
    }

    /// Byte at a given byte offset. O(log n).
    #[inline]
    pub fn byte(&self, offset: u32) -> Option<u8> {
        let (node, rel) = self.find_position(offset, false)?;
        let p = self.get_piece(node);
        let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);
        text.as_bytes().get(rel as usize).copied()
    }

    /// Char at a given char index. O(log n) to find the piece, then
    /// a short scan within the piece.
    #[inline]
    pub fn char(&self, char_index: u32) -> Option<char> {
        let byte_offset = self.char_to_byte(char_index)?;
        let (node, rel) = self.find_position(byte_offset, false)?;
        let p = self.get_piece(node);
        let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);
        text[rel as usize..].chars().next()
    }

    /// Returns a non-allocating iterator of chars over the given char range.
    /// Backed by TreeWalker::seek so it reuses existing infrastructure.
    #[inline]
    pub fn slice_chars(&self, char_start: u32, char_end: u32) -> SliceChars<'_> {
        let byte_start = self.char_to_byte(char_start).unwrap_or(0);
        let byte_end   = self.char_to_byte(char_end).unwrap_or_else(|| self.total_length());
        SliceChars {
            walker:   { let mut w = TreeWalker::new(self); w.seek(byte_start); w },
            byte_end,
        }
    }

    /// Returns a non-allocating iterator of chars over the given byte range.
    #[inline]
    pub fn slice_bytes(&self, byte_start: u32, byte_end: u32) -> SliceChars<'_> {
        SliceChars {
            walker:   { let mut w = TreeWalker::new(self); w.seek(byte_start); w },
            byte_end,
        }
    }

    /// Non-allocating line view: returns a ChunkIter over the byte range of
    /// `line`. Line numbers are 0-based. The trailing \n is included if present,
    /// matching ropey's line() behaviour.
    #[inline]
    pub fn line(&self, line: u32) -> Option<ChunkIter<'_>> {
        let start = self.line_to_offset(line)?;
        let end   = self.line_to_offset(line + 1)
                        .unwrap_or_else(|| self.total_length());
        Some(self.chunks(start, end))
    }

    /// Number of lines (= newline count + 1, matching ropey).
    #[inline]
    pub fn len_lines(&self) -> u32 {
        self.pieces.get(self.root).subtree_newlines + 1
    }

    #[inline(always)]
    pub fn len_chars(&self) -> u32 {
        self.pieces.get(self.root).subtree_chars
    }

    #[inline]
    pub fn len_bytes(&self) -> u32 { self.total_length() }

    #[inline]
    pub fn chars(&self) -> TreeWalker<'_> { TreeWalker::new(self) }

    #[inline]
    pub fn lines(&self) -> LinesIter<'_> {
        LinesIter { tree: self, current_line: 0, total_lines: self.len_lines() }
    }

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
    pub fn char_to_line(&self, char_index: u32) -> Option<u32> {
        let byte_index = self.char_to_byte(char_index)?;
        self.offset_to_line_col(byte_index).map(|(line, _)| line)
    }

    #[inline]
    pub fn line_to_char(&self, line: u32) -> Option<u32> {
        let byte_index = self.line_to_offset(line)?;
        self.byte_to_char(byte_index)
    }

    #[inline]
    pub fn line_to_byte(&self, line: u32) -> Option<u32> {
        self.line_to_offset(line)
    }

    #[inline]
    pub fn char_to_byte(&self, char_index: u32) -> Option<u32> {
        let total_chars = self.len_chars();
        if char_index  > total_chars { return None }
        if char_index == total_chars { return Some(self.len_bytes()) }

        let mut current = self.root;
        let mut current_byte = 0;
        let mut current_char = 0;

        while current != NIL {
            let node = self.pieces.get(current);
            let p = self.get_piece(current);
            let left_chars = self.pieces.get(node.left).subtree_chars;
            let piece_chars = p.char_count;

            if        char_index < current_char + left_chars {
                current = node.left;

            } else if char_index < current_char + left_chars + piece_chars {
                let rel_char = char_index - (current_char + left_chars);

                let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);
                let rel_byte = text.char_indices().nth(rel_char as usize).unwrap().0 as u32;

                let left_len = self.pieces.get(node.left).subtree_len;
                return Some(current_byte + left_len + rel_byte);

            } else {
                current_char += left_chars + piece_chars;
                let left_len = self.pieces.get(node.left).subtree_len;
                current_byte += left_len + p.length;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    pub fn byte_to_char(&self, byte_offset: u32) -> Option<u32> {
        let total_bytes = self.len_bytes();

        if byte_offset > total_bytes  { return None }
        if byte_offset == total_bytes { return Some(self.len_chars()) }

        let mut current = self.root;
        let mut current_byte = 0;
        let mut current_char = 0;

        while current != NIL {
            let node = self.pieces.get(current);
            let p = self.pieces.get_piece(current);
            let left_bytes = self.pieces.get(node.left).subtree_len;
            let piece_bytes = p.length;

            if byte_offset < current_byte + left_bytes {
                current = node.left;

            } else if byte_offset < current_byte + left_bytes + piece_bytes {
                let rel_byte = byte_offset - (current_byte + left_bytes);

                let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);
                let chars_up_to = text[..rel_byte as usize].chars().count() as u32;

                let left_chars = self.pieces.get(node.left).subtree_chars;
                return Some(current_char + left_chars + chars_up_to);

            } else {
                current_byte += left_bytes + piece_bytes;
                let left_chars = self.pieces.get(node.left).subtree_chars;
                current_char += left_chars + p.char_count;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    pub fn offset_to_line_col(&self, offset: u32) -> Option<(u32, u32)> {
        let total_len = self.len_bytes();
        if offset > total_len { return None }

        let line;
        if offset == total_len {
            line = self.pieces.get(self.root).subtree_newlines;

        } else {
            let mut current = self.root;
            let mut current_line = 0;
            let mut current_byte = 0;

            while current != NIL {
                let node = self.pieces.get(current);
                let p = self.pieces.get_piece(current);
                let left_len = self.pieces.get(node.left).subtree_len;
                let left_newlines = self.pieces.get(node.left).subtree_newlines;
                let piece_len = p.length;

                if offset < current_byte + left_len {
                    current = node.left;

                } else if offset < current_byte + left_len + piece_len {
                    current_line += left_newlines;

                    let rel_off = offset - (current_byte + left_len);

                    let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);
                    let newline_count = bytecount::count(
                        text[..rel_off as usize].as_bytes(),
                        b'\n'
                    ) as u32;

                    current_line += newline_count;
                    break;

                } else {
                    current_line += left_newlines + p.newline_count;
                    current_byte += left_len + piece_len;
                    current = node.right;
                }
            }

            line = current_line;
        }

        let line_start_byte = self.line_to_offset(line).unwrap_or(0);
        let target_char     = self.byte_to_char(offset)?;
        let line_start_char = self.byte_to_char(line_start_byte)?;

        Some((line, target_char - line_start_char))
    }
}

/// Non-allocating char iterator over a byte-bounded window of the tree.
pub struct SliceChars<'a> {
    walker:   TreeWalker<'a>,
    byte_end: u32,
}

impl<'a> Iterator for SliceChars<'a> {
    type Item = char;
    #[inline]
    fn next(&mut self) -> Option<char> {
        if self.walker.offset >= self.byte_end { return None; }
        self.walker.next()
    }
}

impl PieceTree {
    /// Fast-path chunk reader specifically designed for the Tree-sitter C API.
    /// Given an absolute byte offset, it returns the largest contiguous byte
    /// slice available starting exactly at that offset.
    #[inline]
    pub fn read_largest_contigous_chunk_at_byte<'a>(&'a self, offset: u32) -> &'a [u8] {
        let total = self.total_length();
        if offset >= total {
            return &[];
        }

        let mut current = self.root;
        let mut current_offset = offset;

        while current != NIL {
            let node = self.pieces.get(current);
            let p = self.pieces.get_piece(current);
            let left_len = self.pieces.get(node.left).subtree_len;
            let piece_len = p.length;

            if current_offset < left_len {
                current = node.left;

            } else if current_offset < left_len + piece_len {
                //
                // The requested offset falls inside this exact piece
                //

                let rel_offset = current_offset - left_len;
                let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);

                return &text.as_bytes()[rel_offset as usize..];

            } else {
                current_offset -= left_len + piece_len;
                current = node.right;
            }
        }

        &[]
    }

    #[inline]
    pub fn line_to_offset(&self, target_line: u32) -> Option<u32> {
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
            let piece_len      = p.length;

            if target_line < current_line + left_newlines {
                current = node.left;

            } else if target_line <= current_line + left_newlines + piece_newlines {
                let rel_line = target_line - (current_line + left_newlines);
                if rel_line == 0 {
                    return Some(current_offset + left_len)
                }

                let text = self.buffers.get_slice(p.buffer_index, p.offset, p.length);

                let mut nl_count = 0;
                for (i, &b) in text.as_bytes().iter().enumerate() {
                    if b == b'\n' {
                        nl_count += 1;
                        if nl_count == rel_line {
                            return Some(current_offset + left_len + i as u32 + 1);
                        }
                    }
                }

                unreachable!();

            } else {
                current_line += left_newlines + piece_newlines;
                current_offset += left_len + piece_len;
                current = node.right;
            }
        }

        None
    }

    #[inline]
    pub fn pieces(&self) -> PieceTreeIter<'_> {
        PieceTreeIter::new(&self.pieces, self.root)
    }

    #[inline]
    pub fn get_line_range(&self, line: u32) -> Option<(u32, u32)> {
        let start = self.line_to_offset(line)?;
        let end = self.line_to_offset(line + 1).unwrap_or_else(|| self.total_length());
        Some((start, end))
    }

    #[inline]
    pub fn get_line_content_allocating(&self, line: u32) -> Option<String> {
        let (start, end) = self.get_line_range(line)?;

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
}

impl PieceTree {
    #[inline]
    pub fn compact(&mut self) {
        let mut new_arena = Pieces::new();

        self.scratch_index_map.clear();
        self.scratch_index_map.resize(self.pieces.nodes.len(), 0);

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
                meta: node.meta,
                _pad: 0,
            });

            index_map[old_index.index()] = new_index.index() as u32;
            new_index
        }

        self.root = copy_node(self.root, &self.pieces, &mut new_arena, &mut self.scratch_index_map);
        for entry in &mut self.undo_stack {
            entry.root = copy_node(entry.root, &self.pieces, &mut new_arena, &mut self.scratch_index_map);
        }
        for entry in &mut self.redo_stack {
            entry.root = copy_node(entry.root, &self.pieces, &mut new_arena, &mut self.scratch_index_map);
        }

        self.pieces = new_arena;
    }

    #[inline]
    pub fn squash(&mut self) {
        if self.root == NIL { return }

        let squashed_text = self.to_string_allocating();
        let length = squashed_text.len() as u32;

        let newline_count = bytecount::count(squashed_text.as_bytes(), b'\n') as u32;
        let char_count = squashed_text.chars().count() as u32;

        self.buffers = Buffers::new();
        self.buffers.original_buffers.push(squashed_text);
        self.pieces = Pieces::new();

        let piece = Piece { buffer_index: 0, offset: 0, length, newline_count, char_count };
        self.root = self.pieces.insert_node(NIL, piece, 0);

        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

pub struct PieceTreeIter<'a, const MAX_INLINE_TREE_DEPTH: usize = 32> {
    arena: &'a Pieces,
    stack: SmallVec<[NodeRef; MAX_INLINE_TREE_DEPTH]>,
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> PieceTreeIter<'a, MAX_INLINE_TREE_DEPTH> {
    #[inline]
    pub fn new(arena: &'a Pieces, mut root: NodeRef) -> Self {
        let mut stack = SmallVec::new();
        while root != NIL {
            stack.push(root);
            root = arena.get(root).left;
        }

        Self { arena, stack }
    }
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> Iterator for PieceTreeIter<'a, MAX_INLINE_TREE_DEPTH> {
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

#[derive(Clone, Copy, PartialEq)]
enum Direction { Left, Center, Right }

pub struct TreeWalker<'a, const MAX_INLINE_TREE_DEPTH: usize = 32> {
    tree: &'a PieceTree,
    stack: SmallVec<[(NodeRef, Direction); MAX_INLINE_TREE_DEPTH]>,
    current_str: str::Chars<'a>,
    pub offset: u32,
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> TreeWalker<'a, MAX_INLINE_TREE_DEPTH> {
    #[inline]
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
            let piece_len = p.length;

            if current_offset < left_len {
                self.stack.push((current, Direction::Center));
                current = node.left;
            } else if current_offset < left_len + piece_len {
                self.stack.push((current, Direction::Right));
                let text = self.tree.buffers.get_slice(p.buffer_index, p.offset, p.length);
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
                    let text = self.tree.buffers.get_slice(p.buffer_index, p.offset, p.length);
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

impl<'a> Iterator for TreeWalker<'a> {
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

pub struct ReverseTreeWalker<'a, const MAX_INLINE_TREE_DEPTH: usize = 32> {
    tree: &'a PieceTree,
    stack: SmallVec<[(NodeRef, bool); MAX_INLINE_TREE_DEPTH]>,
    // Store Chars directly instead of collecting into a Vec
    current_str: str::Chars<'a>,
}

impl<'a, const MAX_INLINE_TREE_DEPTH: usize> ReverseTreeWalker<'a, MAX_INLINE_TREE_DEPTH> {
    #[inline(always)]
    pub fn new(tree: &'a PieceTree) -> Self {
        let mut walker = Self {
            tree,
            stack: SmallVec::new(),
            current_str: "".chars(),
        };
        walker.push_rightmost(tree.root);
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
            let piece_len = p.length;

            if current_offset < left_len {
                self.stack.push((current, true));
                current = node.left;

            } else if current_offset < left_len + piece_len {
                self.stack.push((current, true));

                let text = self.tree.buffers.get_slice(p.buffer_index, p.offset, p.length);
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

impl<'a> Iterator for ReverseTreeWalker<'a> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.current_str.next_back() { return Some(c); }

        while let Some((node_index, visited_right)) = self.stack.pop() {
            let node = self.tree.pieces.get(node_index);
            if !visited_right {
                self.stack.push((node_index, true));

                let p = self.tree.pieces.get_piece(node_index);
                let text_slice = self.tree.buffers.get_slice(p.buffer_index, p.offset, p.length);

                self.current_str = text_slice.chars();

                if let Some(c) = self.current_str.next_back() { return Some(c); }
            } else {
                self.push_rightmost(node.left);
            }
        }

        None
    }
}
