#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

use piece_tree::PieceTree;
use piece_tree::{assert_no_mergeable_neighbors, assert_piece_metadata, assert_invariants, assert_state, assert_coordinates};

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
        assert_coordinates(&tree, &model.text);
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
