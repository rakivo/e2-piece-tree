use piece_tree::{PieceTree};

// Some of these are stolen from: https://github.com/cdacamar/fredbuf/blob/main/fredbuf-test.cpp,
// and others are AI-generated, but yeah I'm not that crazy to write tests myself.

#[cfg(test)]
mod tests_fredbuf {
    use super::*;

    #[test]
    fn test_fredbuf_test3_fragments_and_lines() {
        let mut tree = PieceTree::new();
        // Builder accepting fragments
        tree.insert_no_commit(tree.total_length(), "Hello");
        tree.insert_no_commit(tree.total_length(), ",");
        tree.insert_no_commit(tree.total_length(), " ");
        tree.insert_no_commit(tree.total_length(), "World");
        tree.insert_no_commit(tree.total_length(), "!");
        tree.insert_no_commit(tree.total_length(), "\nThis is a second line.");
        tree.insert_no_commit(tree.total_length(), " Continue...\nANOTHER!");

        assert_eq!(tree.get_line_content_allocating(0).unwrap(), "Hello, World!\n");
        assert_eq!(tree.get_line_content_allocating(1).unwrap(), "This is a second line. Continue...\n");
        assert_eq!(tree.get_line_content_allocating(2).unwrap(), "ANOTHER!");

        tree.insert(37, "Hello"); // This bumps offsets forward

        tree.remove_at(13, 5); // Delete "This "
        tree.remove_at(37, 5); // Delete the "Hello" we just added

        tree.insert(tree.total_length(), "a");
        tree.insert(tree.total_length(), "a");
        tree.insert(tree.total_length(), "a");
        tree.insert(tree.total_length(), "a");
        tree.insert(tree.total_length(), "END!!");

        tree.remove_at(52, 4);

        tree.insert(tree.total_length(), "\nfoobar\nnext\nnextnext\nnextnextnext");
        tree.insert(tree.total_length(), "\nfoobar2\nnext\nnextnext\nnextnextnext");

        // Verify out-of-bounds line requests return None gracefully
        assert_eq!(tree.get_line_content_allocating(99), None);
    }

    #[test]
    fn test_fredbuf_test4_boundary_removal() {
        let mut tree = PieceTree::new();
        tree.insert_no_commit(0, "ABCD");
        tree.insert(4, "a");
        assert_eq!(tree.to_string(), "ABCDa");

        // Delete across the boundary of the original piece and the appended piece
        tree.remove_at(3, 2);
        assert_eq!(tree.to_string(), "ABC");
    }

    #[test]
    fn test_fredbuf_test5_empty_buffer_lifecycle() {
        let mut tree = PieceTree::new();
        // Insert into nothing
        tree.insert(0, "a");
        assert_eq!(tree.to_string(), "a");

        // Remove to nothing
        tree.remove_at(0, 1);
        assert_eq!(tree.to_string(), "");
    }

    #[test]
    fn test_fredbuf_test8_suppress_history() {
        let mut tree = PieceTree::new();
        tree.insert_no_commit(0, "Hello, World!");

        // SuppressHistory::Yes == insert_text_internal
        tree.insert_no_commit(0, "a");
        assert_eq!(tree.to_string(), "aHello, World!");

        // Try undo should fail because internal edits don't record history
        assert!(tree.try_undo(0).is_none());

        tree.remove_no_commit(0, 1); // SuppressHistory::Yes
        assert_eq!(tree.to_string(), "Hello, World!");
        assert!(tree.try_undo(0).is_none());

        // Snap back to "Hello, World!" by committing head
        tree.commit_head(0);
        tree.insert_no_commit(0, "a");
        tree.insert_no_commit(1, "b");
        tree.insert_no_commit(2, "c");
        assert_eq!(tree.to_string(), "abcHello, World!");

        // Undo should bring us back to the manual commit
        assert!(tree.try_undo(0).is_some());
        assert_eq!(tree.to_string(), "Hello, World!");

        tree.commit_head(0);
        tree.remove_no_commit(0, 7);
        assert_eq!(tree.to_string(), "World!");

        tree.remove_no_commit(5, 1);
        assert_eq!(tree.to_string(), "World");

        assert!(tree.try_undo(0).is_some());
        assert_eq!(tree.to_string(), "Hello, World!");

        assert!(tree.try_redo(0).is_some());
        assert_eq!(tree.to_string(), "World");
    }

    #[test]
    fn test_fredbuf_test9_branching_and_snap_to() {
        let mut tree = PieceTree::new();
        tree.insert_no_commit(0, "Hello, World!");

        // In Rust, 'tree.head()' is just capturing `tree.root` (NodeRef)
        let initial_commit = tree.root;

        tree.insert_no_commit(0, "a");
        assert_eq!(tree.to_string(), "aHello, World!");
        assert!(tree.try_undo(0).is_none());

        let commit = tree.root;

        // tree.snap_to(initial_commit)
        tree.root = initial_commit;
        assert_eq!(tree.to_string(), "Hello, World!");

        // Snap back to commit
        tree.root = commit;
        assert_eq!(tree.to_string(), "aHello, World!");

        tree.remove_no_commit(0, 8);
        assert_eq!(tree.to_string(), "World!");

        tree.root = commit;
        assert_eq!(tree.to_string(), "aHello, World!");

        tree.root = initial_commit;
        assert_eq!(tree.to_string(), "Hello, World!");

        // Create a new branch
        tree.insert_no_commit(13, " My name is fredbuf.");
        assert_eq!(tree.to_string(), "Hello, World! My name is fredbuf.");

        let branch = tree.root;

        // Revert back
        tree.root = commit;
        assert_eq!(tree.to_string(), "aHello, World!");

        // Revert to branch
        tree.root = branch;
        assert_eq!(tree.to_string(), "Hello, World! My name is fredbuf.");
    }
}

#[cfg(test)]
mod tests_proptest {
    use super::*;
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Insert { pos_ratio: f64, text: String },
        Remove { pos_ratio: f64, len_ratio: f64 },
        Undo,
        Redo,
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            // 50% chance to insert
            5 => (0.0..=1.0, "[a-zA-Z0-9 \n\t🦀🌎]{1, 20}").prop_map(|(r, t)| Op::Insert { pos_ratio: r, text: t }),
            // 30% chance to remove
            3 => (0.0..=1.0, 0.0..=1.0).prop_map(|(pr, lr)| Op::Remove { pos_ratio: pr, len_ratio: lr }),
            // 10% chance to undo
            1 => Just(Op::Undo),
            // 10% chance to redo
            1 => Just(Op::Redo),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]
        #[test]
        fn piece_tree_matches_string_oracle(ops in prop::collection::vec(op_strategy(), 1..500)) {
            let mut tree = PieceTree::new();

            // The Oracle State
            let mut oracle = String::new();
            let mut oracle_undo = Vec::new();
            let mut oracle_redo = Vec::new();

            for op in ops {
                match op {
                    Op::Insert { pos_ratio, text } => {
                        let offset = (tree.len_bytes() as f64 * pos_ratio) as u32;
                        let offset = clamp_to_char_boundary(&oracle, offset);

                        // Commit state to oracle history
                        oracle_undo.push(oracle.clone());
                        oracle_redo.clear();

                        tree.insert(offset, &text);
                        oracle.insert_str(offset as usize, &text);
                    }
                    Op::Remove { pos_ratio, len_ratio } => {
                        let current_len = tree.len_bytes();
                        if current_len == 0 { continue; }

                        let offset = (current_len as f64 * pos_ratio) as u32;
                        let offset = clamp_to_char_boundary(&oracle, offset);

                        let max_len = current_len - offset;
                        let len = (max_len as f64 * len_ratio) as u32;
                        let end = clamp_to_char_boundary(&oracle, offset + len);
                        let final_len = end - offset;

                        if final_len > 0 {
                            oracle_undo.push(oracle.clone());
                            oracle_redo.clear();

                            tree.remove_at(offset, final_len);
                            oracle.drain(offset as usize..end as usize);
                        }
                    }
                    Op::Undo => {
                        if let Some(prev) = oracle_undo.pop() {
                            oracle_redo.push(oracle.clone());
                            oracle = prev;
                            tree.try_undo(0);
                        }
                    }
                    Op::Redo => {
                        if let Some(next) = oracle_redo.pop() {
                            oracle_undo.push(oracle.clone());
                            oracle = next;
                            tree.try_redo(0);
                        }
                    }
                }

                // INVARIANT: The Piece Tree MUST strictly equal the Oracle String
                assert_eq!(tree.to_string(), oracle);
                assert_eq!(tree.len_bytes() as usize, oracle.len());
                assert_eq!(tree.len_chars() as usize, oracle.chars().count());
            }
        }
    }

    // Helper to prevent slicing strings inside multibyte UTF-8 characters
    fn clamp_to_char_boundary(s: &str, mut index: u32) -> u32 {
        while index > 0 && !s.is_char_boundary(index as usize) {
            index -= 1;
        }
        index
    }
}

#[cfg(test)]
mod tests_edit_merging {
    use super::*;

    #[test]
    fn test_sequential_typing_merges_pieces() {
        let mut tree = PieceTree::new();

        // Emulate a user typing sequentially at the end of the file
        tree.insert(0, "H");
        tree.insert(1, "e");
        tree.insert(2, "l");
        tree.insert(3, "l");
        tree.insert(4, "o");

        assert_eq!(tree.to_string(), "Hello");

        // If merging is working, this should logically be exactly ONE piece.
        // Without merging, this would be a fragmented tree of 5 distinct pieces.
        assert_eq!(tree.pieces().count(), 1);
    }

    #[test]
    fn test_cursor_jumps_break_merging() {
        let mut tree = PieceTree::new();

        // Type at the start
        tree.insert(0, "H");
        tree.insert(1, "o");

        // Move the cursor back into the middle of the document and type
        tree.insert(1, "e");
        tree.insert(2, "l");
        tree.insert(3, "l");

        assert_eq!(tree.to_string(), "Hello");

        // The tree should naturally fracture around the cursor jump.
        // Expected logical pieces: ["H"], ["ell"], ["o"]
        assert_eq!(tree.pieces().count(), 3);
    }

    #[test]
    fn test_merging_preserves_history() {
        let mut tree = PieceTree::new();

        tree.insert(0, "A");
        tree.insert(1, "B");
        tree.insert(2, "C");

        assert_eq!(tree.to_string(), "ABC");
        assert_eq!(tree.pieces().count(), 1, "Should be perfectly merged in memory");

        // Travel back in time.
        // Because the Red-Black tree allocates new nodes on modification,
        // the old roots still point to older pieces with lengths of 2 and 1.
        // They safely ignore the extra bytes appended to the dynamic buffer.

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "AB");

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "A");

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "");

        // Travel forward
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "A");

        tree.try_redo(0);
        assert_eq!(tree.to_string(), "AB");

        tree.try_redo(0);
        assert_eq!(tree.to_string(), "ABC");
    }

    #[test]
    fn test_merging_transaction_batch() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Base");

        // A macro execution types sequentially. We manually commit to
        // group them into a single undo boundary, then insert internally.
        tree.commit_head(0);
        tree.insert_no_commit(4, "X");
        tree.insert_no_commit(5, "Y");
        tree.insert_no_commit(6, "Z");

        assert_eq!(tree.to_string(), "BaseXYZ");

        // The transaction perfectly merged with the previous text!
        // This is mathematically valid because the old root in the undo stack
        // retains its original length metrics.
        assert_eq!(tree.pieces().count(), 1, "Tree successfully merged across a transaction boundary");

        // PROVE THE HISTORY IS INTACT
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "Base", "Undo successfully truncated the view of the merged piece");
        assert_eq!(tree.pieces().count(), 1);
    }
}

#[cfg(test)]
mod tests_coordinates {
    use super::*;

    #[test]
    fn test_exact_coordinate_mapping() {
        let mut tree = PieceTree::new();

        // Corpus: 20 chars, 29 bytes.
        tree.insert(0, "Hello\n🦀world\r\nПривет");

        // --- char_to_byte & byte_to_char ---
        assert_eq!(tree.char_to_byte(0), 0);
        assert_eq!(tree.byte_to_char(0), 0);

        // '🦀' start
        assert_eq!(tree.char_to_byte(6), 6);
        assert_eq!(tree.byte_to_char(6), 6);

        // 'w' after 🦀
        assert_eq!(tree.char_to_byte(7),  10);
        assert_eq!(tree.byte_to_char(10), 7);

        // EOF
        assert_eq!(tree.char_to_byte(20), 29);
        assert_eq!(tree.byte_to_char(29), 20);

        // Out of bounds
        assert_eq!(tree.try_char_to_byte(21), None);
        assert_eq!(tree.try_byte_to_char(30), None);

        // --- line_to_offset ---
        assert_eq!(tree.line_to_byte(0), 0);
        assert_eq!(tree.line_to_byte(1), 6);
        assert_eq!(tree.line_to_byte(2), 17);

        // Out of bounds line
        assert_eq!(tree.try_line_to_byte(3), None);

        // --- offset_to_line_col ---
        assert_eq!(tree.byte_to_line_col(0), (0, 0));
        assert_eq!(tree.byte_to_line_col(5), (0, 5));

        // '🦀' on line 1
        assert_eq!(tree.byte_to_line_col(6), (1, 0));

        // 'w' on line 1
        assert_eq!(tree.byte_to_line_col(10), (1, 1));

        // 'П' on line 2
        assert_eq!(tree.byte_to_line_col(17), (2, 0));

        // EOF
        assert_eq!(tree.byte_to_line_col(29), (2, 6));

        // Out of bounds offset
        assert_eq!(tree.try_byte_to_line_col(30), None);
    }

    #[test]
    fn test_crlf_vs_lf_line_endings() {
        let mut tree = PieceTree::new();
        tree.insert(0, "A\r\nB\nC\r\n");

        assert_eq!(tree.line_to_byte(0), 0);
        assert_eq!(tree.line_to_byte(1), 3);
        assert_eq!(tree.line_to_byte(2), 5);
        assert_eq!(tree.line_to_byte(3), 8);

        assert_eq!(tree.byte_to_line_col(3), (1, 0));
        assert_eq!(tree.byte_to_line_col(5), (2, 0));
    }
}

#[cfg(test)]
mod tests_proptest_coordinates {
    use super::*;
    use proptest::prelude::*;

    fn oracle_byte_to_char(s: &str, byte_idx: usize) -> Option<usize> {
        if byte_idx > s.len() || !s.is_char_boundary(byte_idx) { return None; }
        Some(s[..byte_idx].chars().count())
    }

    fn oracle_char_to_byte(s: &str, char_idx: usize) -> Option<usize> {
        s.char_indices().nth(char_idx).map(|(i, _)| i)
            .or_else(|| if char_idx == s.chars().count() { Some(s.len()) } else { None })
    }

    fn oracle_byte_to_line(s: &str, byte_idx: usize) -> Option<usize> {
        if byte_idx > s.len() { return None; }
        Some(s[..byte_idx].chars().filter(|&c| c == '\n').count())
    }

    fn oracle_line_to_byte(s: &str, line_idx: usize) -> Option<usize> {
        if line_idx == 0 { return Some(0); }
        let mut lines_seen = 0;
        for (i, c) in s.char_indices() {
            if c == '\n' {
                lines_seen += 1;
                if lines_seen == line_idx { return Some(i + 1); }
            }
        }
        None
    }

    fn oracle_line_break_to_byte(s: &str, break_idx: usize) -> Option<usize> {
        let mut breaks_seen = 0;
        for (i, c) in s.char_indices() {
            if c == '\n' {
                if breaks_seen == break_idx { return Some(i); }
                breaks_seen += 1;
            }
        }
        None
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(5_000))]
        #[test]
        fn piece_tree_comprehensive_oracle(
            chunks in prop::collection::vec("[a-zA-Z0-9 \n\r\t🦀🌎Привет]{1,15}", 1..50),
            // Random slice bounds (values between 0.0 and 1.0 to pick percentages of tree length)
            slice_start_pct in 0.0f64..1.0f64,
            slice_end_pct in 0.0f64..1.0f64,
        ) {
            let mut tree = PieceTree::new();
            let mut oracle = String::new();

            //
            // Build the Tree and Oracle
            //
            for chunk in chunks {
                let insert_pos = if tree.len_bytes() == 0 { 0 } else { tree.char_to_byte(tree.len_chars() / 2) };
                let mut safe_insert_byte = insert_pos;
                while safe_insert_byte > 0 && !oracle.is_char_boundary(safe_insert_byte as usize) {
                    safe_insert_byte -= 1;
                }

                tree.insert(safe_insert_byte, &chunk);
                oracle.insert_str(safe_insert_byte as usize, &chunk);
            }

            let total_chars = oracle.chars().count();
            let total_bytes = oracle.len();
            let total_lines = oracle.chars().filter(|&c| c == '\n').count() + 1;
            let total_breaks = total_lines - 1;

            //
            // Base Tree Checks (Coordinates & Content)
            //
            for (byte_idx, c) in oracle.char_indices() {
                let char_idx = oracle_byte_to_char(&oracle, byte_idx).unwrap();
                let byte_idx_u32 = byte_idx as u32;
                let char_idx_u32 = char_idx as u32;

                // Coordinate Mappings
                prop_assert_eq!(tree.char_to_byte(char_idx_u32), byte_idx_u32);
                prop_assert_eq!(tree.byte_to_char(byte_idx_u32), char_idx_u32);

                // Assuming tree.char_to_line / line_to_char are composition methods:
                let expected_line = oracle_byte_to_line(&oracle, byte_idx).unwrap() as u32;
                prop_assert_eq!(tree.byte_to_line(byte_idx_u32), expected_line);
                prop_assert_eq!(tree.char_to_line(char_idx_u32), expected_line);

                // Content Retrieval
                prop_assert_eq!(tree.byte(byte_idx_u32), oracle.as_bytes()[byte_idx]);
                prop_assert_eq!(tree.char(char_idx_u32), c);

                // Chunk Retrieval
                let chunk_byte = tree.chunk_at_byte(byte_idx_u32).0;
                prop_assert!(!chunk_byte.is_empty());
                prop_assert!(oracle[byte_idx..].as_bytes().starts_with(chunk_byte));

                let chunk_char = tree.chunk_at_char(char_idx_u32).0;
                prop_assert_eq!(chunk_char, chunk_byte);
            }

            // Line and Line-Break Iteration
            for line_idx in 0..total_lines {
                let expected_byte = oracle_line_to_byte(&oracle, line_idx).unwrap() as u32;
                prop_assert_eq!(tree.line_to_byte(line_idx as u32), expected_byte);

                let expected_char = oracle_byte_to_char(&oracle, expected_byte as usize).unwrap() as u32;
                prop_assert_eq!(tree.line_to_char(line_idx as u32), expected_char);

                if line_idx < total_breaks {
                    let break_byte = oracle_line_break_to_byte(&oracle, line_idx).unwrap();
                    let chunk_break = tree.chunk_at_line_break(line_idx as u32).0;
                    prop_assert!(!chunk_break.is_empty());
                    prop_assert!(oracle[break_byte..].as_bytes().starts_with(chunk_break));
                }
            }

            //
            // View Slices (Whole, Sub-Slice, Nested Slices)
            //
            // Whole Tree Slice
            let whole_slice = tree.slice(0..total_bytes as u32);
            prop_assert_eq!(whole_slice.len_bytes(), total_bytes as u32);
            prop_assert_eq!(whole_slice.len_chars(), total_chars as u32);

            // Random Valid Sub-Slice Boundaries
            let mut raw_start = (total_chars as f64 * slice_start_pct) as usize;
            let mut raw_end = (total_chars as f64 * slice_end_pct) as usize;
            if raw_start > raw_end { std::mem::swap(&mut raw_start, &mut raw_end); }

            let s_byte = oracle_char_to_byte(&oracle, raw_start).unwrap();
            let e_byte = oracle_char_to_byte(&oracle, raw_end).unwrap();
            let oracle_slice = &oracle[s_byte..e_byte];

            let sub_slice = tree.slice(s_byte as u32 .. e_byte as u32);

            // Nested Slice (Slice of a Slice)
            let nested_slice = sub_slice.slice(0..sub_slice.len_bytes());
            prop_assert_eq!(nested_slice.len_bytes(), sub_slice.len_bytes());

            //
            // Slice Checks (Coordinate & Chunk inside slice)
            //
            prop_assert_eq!(sub_slice.len_bytes(), oracle_slice.len() as u32);
            prop_assert_eq!(sub_slice.len_chars(), oracle_slice.chars().count() as u32);

            let slice_lines = oracle_slice.chars().filter(|&c| c == '\n').count() + 1;
            prop_assert_eq!(sub_slice.len_lines(), slice_lines as u32);

            for (rel_byte_idx, c) in oracle_slice.char_indices() {
                let rel_char_idx = oracle_byte_to_char(oracle_slice, rel_byte_idx).unwrap() as u32;
                let rel_byte_u32 = rel_byte_idx as u32;

                // Slice Mappings
                prop_assert_eq!(sub_slice.char_to_byte(rel_char_idx), rel_byte_u32);
                prop_assert_eq!(sub_slice.byte_to_char(rel_byte_u32), rel_char_idx);

                // Slice Content
                prop_assert_eq!(sub_slice.char(rel_char_idx), c);

                // Slice Chunks (Checking they don't overrun slice bounds unexpectedly)
                let chunk_byte_slice = sub_slice.chunk_at_byte(rel_byte_u32);
                prop_assert!(!chunk_byte_slice.is_empty());
                prop_assert!(oracle_slice[rel_byte_idx..].as_bytes().starts_with(chunk_byte_slice));

                let chunk_char_slice = sub_slice.chunk_at_char(rel_char_idx);
                prop_assert_eq!(chunk_char_slice, chunk_byte_slice);
            }

            // Slice Line Breaks
            for break_idx in 0..(slice_lines - 1) {
                let rel_break_byte = oracle_line_break_to_byte(oracle_slice, break_idx).unwrap();
                let chunk_break_slice = sub_slice.chunk_at_line_break(break_idx as u32);

                prop_assert!(!chunk_break_slice.is_empty());
                prop_assert!(oracle_slice[rel_break_byte..].as_bytes().starts_with(chunk_break_slice));
            }

            //
            // Explicit Out-of-Bounds Checks
            //
            prop_assert_eq!(tree.try_char_to_byte(total_chars as u32 + 1), None);
            prop_assert_eq!(tree.try_byte_to_char(total_bytes as u32 + 1), None);
            prop_assert_eq!(tree.try_line_to_byte(total_lines as u32), None);
            prop_assert_eq!(tree.try_byte_to_line(total_bytes as u32 + 1), None);
            prop_assert_eq!(tree.try_byte(total_bytes as u32), None);
            prop_assert_eq!(tree.try_char(total_chars as u32), None);

            prop_assert_eq!(sub_slice.try_byte(sub_slice.len_bytes()), None);
            prop_assert_eq!(sub_slice.try_char(sub_slice.len_chars()), None);

            //
            // Iterators & String Representation (TreeSlice)
            //
            // The Display trait relies on `chars()`, so this implicitly tests both.
            prop_assert_eq!(sub_slice.to_string(), oracle_slice);

            // chars() iterator exhaustive collection
            let collected_chars: String = sub_slice.chars().collect();
            prop_assert_eq!(&collected_chars, oracle_slice);

            // chunks() iterator reconstruction
            let mut collected_chunks = String::new();
            for chunk in sub_slice.chunks() {
                // Note: In real-world ropes, chunks might end mid-multi-byte char.
                // If your tree strictly boundaries chunks on chars, from_utf8 is safe.
                // Otherwise, use String::from_utf8_lossy(chunk) for the proptest.
                collected_chunks.push_str(chunk);
            }
            prop_assert_eq!(&collected_chunks, oracle_slice);

            // line() iterative reconstruction
            // Using `split_inclusive('\n')` perfectly mimics Ropey's line definition
            // (where lines retain their terminating newline).
            let oracle_lines: Vec<&str> = if oracle_slice.is_empty() {
                vec![""] // An empty string still technically has 1 empty line
            } else {
                oracle_slice.split_inclusive('\n').collect()
            };

            for (line_idx, expected_line) in oracle_lines.into_iter().enumerate() {
                let mut line_str = String::new();
                if let Some(chunk_iter) = sub_slice.try_line(line_idx as u32) {
                    for chunk in chunk_iter {
                        line_str.push_str(chunk);
                    }
                }
                prop_assert_eq!(&line_str, expected_line, "Line {} mismatch", line_idx);
            }

            //
            // Local Line/Byte Coordinate Translations
            //
            for line_idx in 0..(slice_lines as u32) {
                // 1. Convert local line -> local byte
                let local_byte_val = sub_slice.line_to_byte(line_idx);

                // 2. Round-trip: Convert local byte back to local line
                prop_assert_eq!(sub_slice.byte_to_line(local_byte_val), line_idx);

                // 3. Compare with oracle
                let expected_oracle_byte = oracle_line_to_byte(oracle_slice, line_idx as usize).unwrap() as u32;
                prop_assert_eq!(local_byte_val, expected_oracle_byte);
            }

            //
            // Explicit Iteration/Coordinate Out-of-Bounds
            //
            prop_assert!(sub_slice.try_line(slice_lines as u32).is_none());
            prop_assert_eq!(sub_slice.try_line_to_byte(slice_lines as u32), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_group_atomicity() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Start"); // 1st undo entry: "Start"

        // Perform a group of operations
        tree.begin_undo_group(5);
        tree.insert(5, " A");
        tree.insert(7, " B");
        tree.insert(9, " C");
        tree.end_undo_group(); // Should have pushed exactly one entry for the whole sequence

        // The state should now be "Start A B C"
        assert_eq!(tree.to_string(), "Start A B C");

        // Undo should revert all three insertions at once
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "Start");

        // Redo should bring all three back at once
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "Start A B C");
    }

    #[test]
    fn test_nested_undo_groups() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Base");

        tree.begin_undo_group(4);
        tree.insert(4, "1");

        // Start a nested group
        tree.begin_undo_group(5);
        tree.insert(5, "2");
        tree.end_undo_group(); // Depth becomes 1

        tree.insert(6, "3");
        tree.end_undo_group(); // Depth becomes 0, commits the state

        assert_eq!(tree.to_string(), "Base123");

        // A single undo should revert everything added since the top-level begin
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "Base");
    }

    #[test]
    fn test_mixed_grouped_and_ungrouped() {
        let mut tree = PieceTree::new();

        // Ungrouped
        tree.insert(0, "A"); // Undo 1
        tree.insert(1, "B"); // Undo 2

        // Grouped
        tree.begin_undo_group(2);
        tree.insert(2, "C");
        tree.insert(3, "D");
        tree.end_undo_group(); // Undo 3

        assert_eq!(tree.to_string(), "ABCD");

        // Undo the group
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "AB");

        // Undo the individual operations
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "A");

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "");
    }

    #[test]
    fn test_redo_group_atomicity() {
        let mut tree = PieceTree::new();
        tree.insert(0, "A");

        // Grouped changes
        tree.begin_undo_group(1);
        tree.insert(1, "B");
        tree.insert(2, "C");
        tree.end_undo_group();

        // Undo the group
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "A");

        // Redo the group: should restore both 'B' and 'C' at once
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "ABC");
    }

    #[test]
    fn test_redo_history_clearing() {
        let mut tree = PieceTree::new();
        tree.insert(0, "A");
        tree.insert(1, "B");

        tree.try_undo(0); // Back to "A"
        assert_eq!(tree.to_string(), "A");

        // Crucial: Performing a NEW action after an undo must clear the redo stack
        tree.insert(1, "C");
        assert_eq!(tree.to_string(), "AC");

        // Redo should now be empty because we branched the history
        assert!(tree.try_redo(0).is_none());
    }
}
