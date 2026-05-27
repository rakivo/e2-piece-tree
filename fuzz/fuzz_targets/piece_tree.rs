#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use piece_tree::{Color, MOD_BUFFER_INDEX, NIL, NodeRef, Piece, PieceTree};

#[derive(Arbitrary, Debug)]
enum Action {
    Insert { offset_pct: u8, text: String },
    InsertByte { offset_pct: u8, byte: u8 },
    Remove { offset_pct: u8, len_pct: u8 },
    Undo { count: u8 },
    Redo { count: u8 },
}

struct Model {
    text: String,
    undo: Vec<String>,
    redo: Vec<String>,
}

impl Model {
    fn new() -> Self {
        Self {
            text: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        self.undo.push(self.text.clone());
        self.redo.clear();
        self.text.insert_str(offset, text);
    }

    fn remove(&mut self, offset: usize, len: usize) {
        if len == 0 {
            return;
        }
        self.undo.push(self.text.clone());
        self.redo.clear();
        self.text.replace_range(offset..offset + len, "");
    }

    fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(self.text.clone());
            self.text = prev;
            true
        } else {
            false
        }
    }

    fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.text.clone());
            self.text = next;
            true
        } else {
            false
        }
    }
}

fuzz_target!(|actions: Vec<Action>| {
    let mut tree = PieceTree::new();
    let mut model = Model::new();

    for action in actions {
        match action {
            Action::Insert { offset_pct, text } => {
                let text = sanitize_ascii(&text);
                if text.is_empty() {
                    continue;
                }

                let offset = biased_offset(offset_pct, model.text.len());
                model.insert(offset, &text);
                tree.insert(offset as u32, &text);
            }

            Action::InsertByte { offset_pct, byte } => {
                let ch = ascii_byte(byte);
                let text = ch.to_string();

                let offset = biased_offset(offset_pct, model.text.len());
                model.insert(offset, &text);
                tree.insert(offset as u32, &text);
            }

            Action::Remove { offset_pct, len_pct } => {
                let len = model.text.len();
                if len == 0 {
                    continue;
                }

                let offset = biased_offset(offset_pct, len);
                if offset >= len {
                    continue;
                }

                let available = len - offset;
                let remove_len = biased_len(len_pct, available);
                if remove_len == 0 {
                    continue;
                }

                model.remove(offset, remove_len);
                tree.remove_at(offset as u32, remove_len as u32);
            }

            Action::Undo { count } => {
                let n = (count as usize % 16) + 1;
                for _ in 0..n {
                    if model.undo() {
                        let _ = tree.try_undo(0);
                    } else {
                        break;
                    }
                }
            }

            Action::Redo { count } => {
                let n = (count as usize % 16) + 1;
                for _ in 0..n {
                    if model.redo() {
                        let _ = tree.try_redo(0);
                    } else {
                        break;
                    }
                }
            }
        }

        assert_state(&tree, &model.text);
        assert_invariants(&tree);
        assert_piece_metadata(&tree);
        assert_no_mergeable_neighbors(&tree);
    }
});

fn sanitize_ascii(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii()).collect()
}

fn ascii_byte(b: u8) -> char {
    match b % 96 {
        0 => '\n',
        1 => '\t',
        v => (32 + (v - 2)) as u8 as char,
    }
}

fn biased_offset(pct: u8, len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    match pct % 8 {
        0 => 0,
        1 => len.saturating_sub(1),
        2 => len / 2,
        3 => len / 3,
        4 => (len * 2) / 3,
        _ => scale_offset(pct, len),
    }.min(len)
}

fn biased_len(pct: u8, available: usize) -> usize {
    if available == 0 {
        return 0;
    }

    match pct % 8 {
        0 => 1.min(available),
        1 => available,
        2 => (available / 2).max(1),
        3 => (available / 3).max(1),
        4 => ((available * 2) / 3).max(1),
        _ => scale_offset(pct, available).max(1),
    }.min(available)
}

fn scale_offset(pct: u8, max_len: usize) -> usize {
    if max_len == 0 {
        return 0;
    }
    ((pct as usize * max_len) / 255).min(max_len)
}

fn assert_state(tree: &PieceTree, expected: &str) {
    let tree_text = tree.to_string_allocating();

    assert_eq!(tree_text, expected, "text mismatch");
    assert_eq!(tree.total_length() as usize, expected.len(), "length mismatch");

    if !expected.is_empty() {
        let offsets = [0, expected.len() / 2, expected.len() - 1];
        for off in offsets {
            let chunk = tree.read_largest_contigous_chunk_at_byte(off as u32);
            let chunk_str = std::str::from_utf8(chunk).unwrap();
            assert!(
                expected[off..].starts_with(chunk_str),
                "chunk mismatch at offset {}",
                off
            );
        }
    }
}

fn assert_invariants(tree: &PieceTree) {
    fn check(tree: &PieceTree, node: NodeRef) -> (usize, usize) {
        if node == NIL {
            return (0, 1);
        }

        let n = tree.pieces.nodes[node];
        let piece = tree.get_piece(node);

        let (l_len, l_bh) = check(tree, n.left);
        let (r_len, r_bh) = check(tree, n.right);

        let expected_len = l_len + piece.length as usize + r_len;
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

        let bh = l_bh + if n.color() == Color::Black { 1 } else { 0 };
        (expected_len, bh)
    }

    if tree.root != NIL {
        assert_eq!(
            tree.pieces.nodes[tree.root].color(),
            Color::Black,
            "root is not black"
        );
        let (len, _) = check(tree, tree.root);
        assert_eq!(len, tree.total_length() as usize, "tree length and root length differ");
    } else {
        assert_eq!(tree.total_length(), 0, "empty tree has nonzero length");
    }
}

fn assert_piece_metadata(tree: &PieceTree) {
    fn walk(tree: &PieceTree, node: NodeRef) {
        if node == NIL {
            return;
        }

        let n = tree.pieces.nodes[node];
        let p = tree.get_piece(node);

        let buf = buffer_for_piece(tree, p.buffer_index);
        let start = p.offset as usize;
        let end = start + p.length as usize;

        assert!(end <= buf.len(), "piece points past end of buffer");
        assert!(p.length > 0, "zero-length piece found");

        let slice = &buf[start..end];
        let actual_chars = slice.chars().count() as u32;
        let actual_nl = slice.as_bytes().iter().filter(|&&b| b == b'\n').count() as u32;

        assert_eq!(p.char_count, actual_chars, "char_count mismatch");
        assert_eq!(p.newline_count, actual_nl, "newline_count mismatch");

        walk(tree, n.left);
        walk(tree, n.right);
    }

    walk(tree, tree.root);
}

fn assert_no_mergeable_neighbors(tree: &PieceTree) {
    fn inorder(tree: &PieceTree, node: NodeRef, last: &mut Option<Piece>) {
        if node == NIL {
            return;
        }

        let n = tree.pieces.nodes[node];
        inorder(tree, n.left, last);

        let cur = tree.get_piece(node);

        if let Some(prev) = *last {
            if prev.buffer_index == cur.buffer_index
                && prev.offset + prev.length == cur.offset
            {
                panic!("mergeable neighboring pieces were left unmerged");
            }
        }

        *last = Some(cur);
        inorder(tree, n.right, last);
    }

    let mut last = None;
    inorder(tree, tree.root, &mut last);
}

fn buffer_for_piece<'a>(tree: &'a PieceTree, buffer_index: u32) -> &'a str {
    if buffer_index == MOD_BUFFER_INDEX {
        &tree.buffers.modifications_buffer
    } else {
        &tree.buffers.original_buffers[buffer_index as usize]
    }
}
