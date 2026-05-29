use piece_tree::{PieceTree, ReverseTreeWalker, TreeWalker};

// Some of these are stolen from: https://github.com/cdacamar/fredbuf/blob/main/fredbuf-test.cpp,
// and others are AI-generated, but yeah I'm not that crazy to write tests myself.

#[cfg(test)]
mod tests_char_api {
    use super::*;

    #[test]
    fn test_lines_iterator() {
        let mut tree = PieceTree::new();
        tree.insert(0, "First line\nSecond line\nThird line");

        assert_eq!(tree.len_lines(), 3);

        let mut lines = tree.lines();
        assert_eq!(lines.next().unwrap(), "First line\n");
        assert_eq!(lines.next().unwrap(), "Second line\n");
        assert_eq!(lines.next().unwrap(), "Third line");
        assert!(lines.next().is_none());
    }

    #[test]
    fn test_chars_iterator() {
        let mut tree = PieceTree::new();
        tree.insert(0, "abc 🦀");

        let chars: Vec<char> = tree.chars().collect();
        assert_eq!(chars, vec!['a', 'b', 'c', ' ', '🦀']);
    }

    #[test]
    fn test_tree_walker_utf8() {
        let mut tree = PieceTree::new();
        // 🦀 is 4 bytes long
        tree.insert(0, "Rust 🦀");
        tree.insert(9, " is fast."); // 9 because "Rust " (5) + 🦀 (4)

        let mut walker = TreeWalker::new(&tree);
        assert_eq!(walker.next(), Some('R'));

        // Seek directly to the emoji (byte offset 5)
        walker.seek(5);
        assert_eq!(walker.next(), Some('🦀'));
        assert_eq!(walker.offset, 9); // Offset should advance by 4 bytes
        assert_eq!(walker.next(), Some(' '));

        let full_text: String = TreeWalker::new(&tree).collect();
        assert_eq!(full_text, "Rust 🦀 is fast.");
    }

    #[test]
    fn test_line_col_resolution() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Line 0\nLine 1 🦀\nLine 2");

        // Document byte analysis:
        // "Line 0\n" -> 7 bytes
        // "Line 1 🦀\n" -> 12 bytes (7 + 4 + 1)
        // "Line 2" -> 6 bytes
        // Total = 25 bytes

        // Check line starts
        assert_eq!(tree.line_to_byte(0), Some(0));
        assert_eq!(tree.line_to_byte(1), Some(7));
        assert_eq!(tree.line_to_byte(2), Some(19)); // The culprit! Fixed from 20

        // Check offset to (line, col)
        // Offset 0 = L0, C0
        assert_eq!(tree.byte_to_line_col(0), Some((0, 0)));

        // Middle of Line 0
        assert_eq!(tree.byte_to_line_col(3), Some((0, 3)));

        // Offset 14 = right before the emoji in Line 1
        assert_eq!(tree.byte_to_line_col(14), Some((1, 7)));

        // Offset 18 = right after the emoji in Line 1 (on the newline)
        assert_eq!(tree.byte_to_line_col(18), Some((1, 8)));

        // End of document
        assert_eq!(tree.byte_to_line_col(25), Some((2, 6)));
    }

    #[test]
    fn test_garbage_collection() {
        let mut tree = PieceTree::new();

        // 1. Initial State
        tree.insert(0, "A");
        let initial_node_count = tree.pieces.nodes.len();

        // 2. Thrash the arena to generate dead nodes
        for _ in 0..100 {
            tree.insert(1, "X");
            tree.remove_at(1, 1);
        }

        let bloated_node_count = tree.pieces.nodes.len();
        assert!(bloated_node_count > initial_node_count + 100);

        // 3. Compact the tree
        tree.compact();

        let compacted_node_count = tree.pieces.nodes.len();
        assert!(compacted_node_count < bloated_node_count);

        // Ensure data integrity survived the re-indexing
        assert_eq!(tree.to_string(), "A");

        // Ensure the undo stack survived the re-indexing
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "AX"); // The last state before the final remove
    }

    #[test]
    fn test_reverse_walker() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Hello");
        tree.insert(5, " World");

        let mut walker = ReverseTreeWalker::new(&tree);
        assert_eq!(walker.next(), Some('d'));
        assert_eq!(walker.next(), Some('l'));

        // Seek to index 5 (Right between 'Hello' and ' World')
        walker.seek(5);
        // The character preceding index 5 is 'o'
        assert_eq!(walker.next(), Some('o'));
        assert_eq!(walker.next(), Some('l'));
    }

    #[test]
    fn test_line_extractors() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Line 0\nLine 1\nLine 2");

        // get_line_range
        assert_eq!(tree.get_line_range(1), Some((7, 14)));

        // get_line_content
        assert_eq!(tree.get_line_content_allocating(0).unwrap(), "Line 0\n");
        assert_eq!(tree.get_line_content_allocating(1).unwrap(), "Line 1\n");
        assert_eq!(tree.get_line_content_allocating(2).unwrap(), "Line 2");
    }

    #[test]
    fn test_range_removal() {
        let mut tree = PieceTree::new();
        tree.insert(0, "The quick brown fox");

        // Use standard Rust exclusive range
        tree.remove(4..10);
        assert_eq!(tree.to_string(), "The brown fox");

        // Use standard Rust inclusive range
        tree.remove(4..=9);
        assert_eq!(tree.to_string(), "The fox");

        // Use standard Rust unbounded range (clear to end)
        tree.remove(3..);
        assert_eq!(tree.to_string(), "The");
    }

    #[test]
    fn test_char_and_byte_conversion() {
        let mut tree = PieceTree::new();
        // '🦀' is 4 bytes, 1 char. 'é' is 2 bytes, 1 char.
        tree.insert(0, "a🦀cé");

        // Total stats: 4 chars. 1 + 4 + 1 + 2 = 8 bytes.
        assert_eq!(tree.len_chars(), 4);
        assert_eq!(tree.len_bytes(), 8);

        // Byte -> Char mapping
        assert_eq!(tree.byte_to_char(0), Some(0)); // 'a'
        assert_eq!(tree.byte_to_char(1), Some(1)); // '🦀'
        assert_eq!(tree.byte_to_char(5), Some(2)); // 'c'
        assert_eq!(tree.byte_to_char(6), Some(3)); // 'é'
        assert_eq!(tree.byte_to_char(8), Some(4)); // End of string

        // Char -> Byte mapping
        assert_eq!(tree.char_to_byte(0), Some(0));
        assert_eq!(tree.char_to_byte(1), Some(1));
        assert_eq!(tree.char_to_byte(2), Some(5));
        assert_eq!(tree.char_to_byte(3), Some(6));
        assert_eq!(tree.char_to_byte(4), Some(8));
    }

    #[test]
    fn test_optimized_offset_col() {
        let mut tree = PieceTree::new();
        tree.insert(0, "L0\nL1 🦀\nL2");

        // Offset 8 falls directly after the emoji in L1
        // L0\n (3 bytes) + L1 (3 bytes) + 🦀 (4 bytes) = 10 bytes total.
        // Wait, "L1 🦀" string: 'L', '1', ' ', '🦀'.
        // So offset right after emoji is 3 + 3 + 4 = 10.
        let pos = tree.byte_to_line_col(10).unwrap();
        assert_eq!(pos, (1, 4)); // Line 1, Column 4 (since emoji is just 1 column wide)
    }
}

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
mod tests_stress {
    use piece_tree::{Edit, NIL, NodeRef};

    use super::*;

    struct Lcg {
        state: u64,
    }
    impl Lcg {
        fn new(seed: u64) -> Self { Self { state: seed } }
        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
            self.state
        }
        fn next_range(&mut self, max: u32) -> u32 {
            (self.next() % (max as u64)) as u32
        }
    }

    // Helper for the depth stress test
    fn get_max_depth(tree: &PieceTree, node: NodeRef) -> usize {
        if node == NIL {
            return 0;
        }
        let n = tree.pieces.get(node);
        1 + core::cmp::max(
            get_max_depth(tree, n.left),
            get_max_depth(tree, n.right),
        )
    }

    #[test]
    fn test_massive_append_and_metrics() {
        let mut tree = PieceTree::new();
        let iters = 50_000;

        for _ in 0..iters {
            let len = tree.len_bytes();
            tree.insert(len, "A");
        }

        assert_eq!(tree.len_bytes(), iters);
        assert_eq!(tree.len_chars(), iters);
        assert_eq!(tree.len_lines(), 1);

        for _ in 0..(iters / 2) {
            tree.remove_at(0, 1);
        }

        assert_eq!(tree.len_bytes(), iters / 2);
    }

    #[test]
    fn test_fuzz_random_edits() {
        let mut tree = PieceTree::new();
        tree.insert(0, "INITIAL");

        let mut rng = Lcg::new(42);
        let mut expected_len = 7;

        for _ in 0..10_000 {
            let op = rng.next() % 3;
            let current_len = tree.len_bytes();

            if op < 2 || current_len == 0 {
                let offset = rng.next_range(current_len + 1);
                tree.insert(offset, "X");
                expected_len += 1;
            } else {
                let offset = rng.next_range(current_len);
                let remove_len = core::cmp::min(3, current_len - offset);
                tree.remove_at(offset, remove_len);
                expected_len -= remove_len;
            }

            // --- THE EDITOR MEMORY LOOP ---

            // 1. Cap the undo stack. This un-pins old roots so their
            // exclusive nodes become truly unreachable.
            if tree.undo_stack.len() > 100 {
                tree.undo_stack.drain(0..50);
            }

            // 2. Dynamically compact the arena when it gets bloated
            if tree.pieces.nodes.len() > 20_000 {
                tree.compact();
            }
        }

        assert_eq!(tree.len_bytes(), expected_len);

        // Because we cap history and compact, this will easily pass
        assert!(tree.pieces.nodes.len() < 30_000);
    }

    #[test]
    fn test_deep_history_yoyo() {
        let mut tree = PieceTree::new();
        let commits = 10_000;

        // Replaced `i` with `_` to fix the unused variable warning
        for _ in 0..commits {
            tree.insert(tree.len_bytes(), "A");
        }

        assert_eq!(tree.len_bytes(), commits);
        assert_eq!(tree.undo_stack.len() as u32, commits);
        assert!(tree.redo_stack.is_empty());

        for _ in 0..(commits / 2) {
            tree.try_undo(0).unwrap();
        }

        assert_eq!(tree.len_bytes(), commits / 2);
        assert_eq!(tree.undo_stack.len() as u32, commits / 2);
        assert_eq!(tree.redo_stack.len() as u32, commits / 2);

        for _ in 0..(commits / 4) {
            tree.try_redo(0).unwrap();
        }

        assert_eq!(tree.len_bytes(), (commits / 2) + (commits / 4));
        assert_eq!(tree.undo_stack.len() as u32, (commits / 2) + (commits / 4));
        assert_eq!(tree.redo_stack.len() as u32, commits / 4);
    }

    #[test]
    fn test_history_branching_timeline_burn() {
        let mut tree = PieceTree::new();

        tree.insert(0, "A");
        tree.insert(1, "B");
        tree.insert(2, "C");
        tree.insert(3, "D");
        tree.insert(4, "E");

        assert_eq!(tree.to_string(), "ABCDE");
        assert_eq!(tree.undo_stack.len(), 5);

        tree.try_undo(0);
        tree.try_undo(0);
        tree.try_undo(0);

        assert_eq!(tree.to_string(), "AB");
        assert_eq!(tree.undo_stack.len(), 2);
        assert_eq!(tree.redo_stack.len(), 3);

        tree.insert(2, "X");

        assert_eq!(tree.to_string(), "ABX");
        assert_eq!(tree.undo_stack.len(), 3);
        assert!(tree.redo_stack.is_empty(), "Redo stack was not cleared on diverging edit!");

        assert!(tree.try_redo(0).is_none());

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "AB");
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "A");
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "");
    }

    #[test]
    fn test_transaction_batching_undo() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Line 1\nLine 2\nLine 3");

        let pre_tx_undos = tree.undo_stack.len();

        let mut edits = vec![
            Edit::Insert { offset: 0, text: "> " },
            Edit::Insert { offset: 7, text: "> " },
            Edit::Insert { offset: 14, text: "> " },
        ];

        tree.apply_edits(0, &mut edits);

        assert_eq!(tree.to_string(), "> Line 1\n> Line 2\n> Line 3");
        assert_eq!(tree.undo_stack.len(), pre_tx_undos + 1);

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_compact_preserves_history() {
        let mut tree = PieceTree::new();

        tree.insert(0, "Phase 1");
        tree.insert(7, " -> Phase 2");
        tree.remove_at(0, 5);
        tree.insert(0, "Step");

        assert_eq!(tree.to_string(), "Step 1 -> Phase 2");
        let active_nodes_before = tree.pieces.nodes.len();

        tree.compact();

        let active_nodes_after = tree.pieces.nodes.len();
        assert!(active_nodes_after < active_nodes_before, "Compaction didn't reclaim nodes");

        tree.try_undo(0);
        assert_eq!(tree.to_string(), " 1 -> Phase 2");

        tree.try_undo(0);
        assert_eq!(tree.to_string(), "Phase 1 -> Phase 2");

        tree.try_redo(0);
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "Step 1 -> Phase 2");
    }

    #[test]
    fn test_diabolical_out_of_bounds_and_empty() {
        let mut tree = PieceTree::new();

        // 1. Removing from an empty document shouldn't panic
        tree.remove_at(0, 100);
        assert_eq!(tree.to_string(), "");

        // 2. Inserting empty strings shouldn't create phantom pieces
        tree.insert(0, "");
        assert_eq!(tree.len_bytes(), 0);

        tree.insert(0, "A");
        // 3. Removing past the end of the document should clamp automatically
        tree.remove_at(0, 1000);
        assert_eq!(tree.to_string(), "");

        // 4. Multi-cursor overlapping deletes (Transaction drift edge case)
        tree.insert(0, "123456789");
        let mut edits = vec![
            Edit::Remove { offset: 2, length: 4 }, // Deletes "3456"
            Edit::Remove { offset: 4, length: 4 }, // Conceptually overlaps!
        ];
        // Applying overlapping transaction deletes in reverse order should handle gracefully
        // without panicking on out-of-bounds.
        tree.apply_edits(0, &mut edits);
        // Result depends on your exact clamping logic, but it MUST NOT panic.
    }

    #[test]
    fn test_diabolical_history_yoyo() {
        let mut tree = PieceTree::new();

        tree.insert(0, "A"); // Undo 1
        tree.insert(1, "B"); // Undo 2
        tree.insert(2, "C"); // Undo 3

        assert_eq!(tree.to_string(), "ABC");

        // Yo-yo down
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "AB");
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "A");
        tree.try_undo(0);
        assert_eq!(tree.to_string(), "");

        // Yo-yo up
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "A");
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "AB");
        tree.try_redo(0);
        assert_eq!(tree.to_string(), "ABC");

        // The alternate timeline test: Undo once, then type, destroying the Redo future
        tree.try_undo(0); // Text is now "AB"
        tree.insert(2, "X"); // Text is now "ABX"

        // Try to redo "C". It should be None because typing "X" burned the timeline.
        assert!(tree.try_redo(0).is_none());
        assert_eq!(tree.to_string(), "ABX");
    }

    #[test]
    fn test_reverse_iterator_utf8_fracture() {
        let mut tree = PieceTree::new();
        // Insert a string with multibyte characters that get fractured across pieces
        tree.insert(0, "Hello 🦀");
        tree.insert(4, "🌎");
        // Tree is now: [Hell][🌎][o 🦀]

        let rev_chars: String = tree.chars_rev().collect();
        // Should perfectly reconstruct the string backwards despite piece boundaries
        assert_eq!(rev_chars, "🦀 o🌎lleH");
    }

    #[test]
    fn test_red_black_tree_depth_stress() {
        let mut tree = PieceTree::new();

        // Insert 10,000 characters ONE AT A TIME at the end of the file.
        // If the Red-Black tree balancing logic is broken, it will devolve into a linked list
        // and cause a Stack Overflow on traversal.
        for _ in 0..10_000 {
            let len = tree.len_bytes();
            tree.insert(len, "x");
        }

        assert_eq!(tree.len_bytes(), 10_000);

        // Verify $O(\log N)$ depth. A perfectly balanced RB tree of 10,000 nodes
        // has a max depth of ~2 * log2(10000) = ~28.
        // If it returns something like 10,000, your tree balancing is completely broken.
        let depth = get_max_depth(&tree, tree.root);
        assert!(depth < 40, "Tree is unbalanced! Depth is {}, expected < 40", depth);
    }

    #[test]
    fn test_basic_bounds() {
        let mut tree = PieceTree::new();
        // Insert at 0
        tree.insert(0, "ABC");
        // Insert at end
        tree.insert(3, "GHI");
        // Insert directly in the middle
        tree.insert(3, "DEF");
        assert_eq!(tree.to_string(), "ABCDEFGHI");

        // Remove from the very start
        tree.remove_at(0, 3);
        assert_eq!(tree.to_string(), "DEFGHI");

        // Remove from the very end
        tree.remove_at(3, 3);
        assert_eq!(tree.to_string(), "DEF");
    }

    #[test]
    fn test_delete_exact_piece() {
        let mut tree = PieceTree::new();
        // Force the creation of 3 distinct pieces
        tree.insert(0, "Left");
        tree.insert(4, "Center");
        tree.insert(10, "Right");

        // Delete the exact bounds of the center piece
        tree.remove_at(4, 6);
        assert_eq!(tree.to_string(), "LeftRight");
        assert_eq!(tree.pieces().count(), 2); // Should perfectly fuse the tree to 2 pieces
    }

    #[test]
    fn test_clear_entire_document() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Data Oriented Design");
        tree.remove(..); // Using our ropey API
        assert_eq!(tree.to_string(), "");
        assert_eq!(tree.len_bytes(), 0);
        assert_eq!(tree.len_chars(), 0);
        assert_eq!(tree.len_lines(), 1); // An empty document is always 1 line
    }

    #[test]
    fn test_multicursor_typing() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Apple\nBanana\nCherry");

        // Imagine 3 cursors at the start of every line, and the user types "- "
        let mut edits = vec![
            Edit::Insert { offset: 0, text: "- " },
            Edit::Insert { offset: 6, text: "- " },
            Edit::Insert { offset: 13, text: "- " },
        ];

        // Apply with primary cursor at offset 0
        tree.apply_edits(0, &mut edits);

        assert_eq!(tree.to_string(), "- Apple\n- Banana\n- Cherry");
    }

    #[test]
    fn test_multicursor_wrapping_html() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Word1 Word2");

        // User highlights "Word1" (0..5) and "Word2" (6..11) and hits a "wrap in <b>" shortcut.
        // We need 4 disjoint edits.
        let mut edits = vec![
            Edit::Insert { offset: 0, text: "<b>" },
            Edit::Insert { offset: 5, text: "</b>" },
            Edit::Insert { offset: 6, text: "<b>" },
            Edit::Insert { offset: 11, text: "</b>" },
        ];

        tree.apply_edits(0, &mut edits);
        assert_eq!(tree.to_string(), "<b>Word1</b> <b>Word2</b>");
    }

    #[test]
    fn test_multicursor_transaction_undo() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Line 1\nLine 2\nLine 3");

        // Transaction: delete the numbers from each line
        let mut edits = vec![
            Edit::Remove { offset: 5, length: 1 },
            Edit::Remove { offset: 12, length: 1 },
            Edit::Remove { offset: 19, length: 1 },
        ];

        tree.apply_edits(19, &mut edits);
        assert_eq!(tree.to_string(), "Line \nLine \nLine ");

        // The user realizes they made a mistake and hits Undo.
        // Because apply_edits grouped them, one try_undo() should revert all 3 deletes.
        let undo_jump_offset = tree.try_undo(0).unwrap();

        assert_eq!(tree.to_string(), "Line 1\nLine 2\nLine 3");
        assert_eq!(undo_jump_offset, 19); // The viewport should jump back to the primary cursor!
    }

    #[test]
    fn test_mixed_edits_transaction() {
        let mut tree = PieceTree::new();
        tree.insert(0, "Hello World!");

        // Transaction: Replace "Hello" with "Goodbye"
        // This consists of a Remove and an Insert at the same conceptual location
        let mut edits = vec![
            Edit::Remove { offset: 0, length: 5 },
            Edit::Insert { offset: 0, text: "Goodbye" },
        ];

        tree.apply_edits(0, &mut edits);
        assert_eq!(tree.to_string(), "Goodbye World!");
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
        assert_eq!(tree.char_to_byte(0), Some(0));
        assert_eq!(tree.byte_to_char(0), Some(0));

        // '🦀' start
        assert_eq!(tree.char_to_byte(6), Some(6));
        assert_eq!(tree.byte_to_char(6), Some(6));

        // 'w' after 🦀
        assert_eq!(tree.char_to_byte(7), Some(10));
        assert_eq!(tree.byte_to_char(10), Some(7));

        // EOF
        assert_eq!(tree.char_to_byte(20), Some(29));
        assert_eq!(tree.byte_to_char(29), Some(20));

        // Out of bounds
        assert_eq!(tree.char_to_byte(21), None);
        assert_eq!(tree.byte_to_char(30), None);

        // --- line_to_offset ---
        assert_eq!(tree.line_to_byte(0), Some(0));
        assert_eq!(tree.line_to_byte(1), Some(6));
        assert_eq!(tree.line_to_byte(2), Some(17));

        // Out of bounds line
        assert_eq!(tree.line_to_byte(3), None);

        // --- offset_to_line_col ---
        assert_eq!(tree.byte_to_line_col(0), Some((0, 0)));
        assert_eq!(tree.byte_to_line_col(5), Some((0, 5)));

        // '🦀' on line 1
        assert_eq!(tree.byte_to_line_col(6), Some((1, 0)));

        // 'w' on line 1
        assert_eq!(tree.byte_to_line_col(10), Some((1, 1)));

        // 'П' on line 2
        assert_eq!(tree.byte_to_line_col(17), Some((2, 0)));

        // EOF
        assert_eq!(tree.byte_to_line_col(29), Some((2, 6)));

        // Out of bounds offset
        assert_eq!(tree.byte_to_line_col(30), None);
    }

    #[test]
    fn test_crlf_vs_lf_line_endings() {
        let mut tree = PieceTree::new();
        tree.insert(0, "A\r\nB\nC\r\n");

        assert_eq!(tree.line_to_byte(0), Some(0));
        assert_eq!(tree.line_to_byte(1), Some(3));
        assert_eq!(tree.line_to_byte(2), Some(5));
        assert_eq!(tree.line_to_byte(3), Some(8));

        assert_eq!(tree.byte_to_line_col(3), Some((1, 0)));
        assert_eq!(tree.byte_to_line_col(5), Some((2, 0)));
    }
}

#[cfg(test)]
mod tests_proptest_coordinates {
    use super::*;
    use proptest::prelude::*;

    // (Assume string_offset_to_line_col and string_line_to_byte_offset from previous message)
    fn string_offset_to_line_col(s: &str, byte_offset: usize) -> (usize, usize) {
        let slice = &s[..byte_offset];
        let line = slice.chars().filter(|&c| c == '\n').count();
        let last_newline_byte = slice.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = s[last_newline_byte..byte_offset].chars().count();
        (line, col)
    }

    fn string_line_to_byte_offset(s: &str, target_line: usize) -> usize {
        if target_line == 0 { return 0; }
        let mut current_line = 0;
        for (i, c) in s.char_indices() {
            if c == '\n' {
                current_line += 1;
                if current_line == target_line {
                    return i + 1; // skip the \n itself
                }
            }
        }
        s.len()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(5_000))]
        #[test]
        fn piece_tree_coordinates_match_string_oracle(
            chunks in prop::collection::vec("[a-zA-Z0-9 \n\r\t🦀🌎Привет]{1,15}", 1..50)
        ) {
            let mut tree = PieceTree::new();
            let mut oracle = String::new();

            for chunk in chunks {
                let insert_pos = if tree.len_bytes() == 0 { 0 } else { tree.char_to_byte(tree.len_chars() / 2).unwrap() };

                let mut safe_insert_byte = insert_pos;
                while safe_insert_byte > 0 && !oracle.is_char_boundary(safe_insert_byte as usize) {
                    safe_insert_byte -= 1;
                }

                tree.insert(safe_insert_byte, &chunk);
                oracle.insert_str(safe_insert_byte as usize, &chunk);
            }

            for (byte_idx, _) in oracle.char_indices() {
                let char_idx = oracle[..byte_idx].chars().count() as u32;
                let byte_idx_u32 = byte_idx as u32;

                prop_assert_eq!(tree.char_to_byte(char_idx), Some(byte_idx_u32));
                prop_assert_eq!(tree.byte_to_char(byte_idx_u32), Some(char_idx));

                let expected_line_col = string_offset_to_line_col(&oracle, byte_idx);
                let actual_line_col = tree.byte_to_line_col(byte_idx_u32).unwrap();

                prop_assert_eq!(
                    (actual_line_col.0 as usize, actual_line_col.1 as usize),
                    expected_line_col
                );
            }

            let total_lines = oracle.chars().filter(|&c| c == '\n').count() + 1;
            for line_idx in 0..total_lines {
                let expected_byte_offset = string_line_to_byte_offset(&oracle, line_idx) as u32;
                prop_assert_eq!(
                    tree.line_to_byte(line_idx as u32),
                    Some(expected_byte_offset)
                );
            }

            // --- Explicit None / Out of Bounds Checks ---
            prop_assert_eq!(tree.char_to_byte(oracle.chars().count() as u32 + 1), None);
            prop_assert_eq!(tree.byte_to_char(oracle.len() as u32 + 1), None);
            prop_assert_eq!(tree.line_to_byte(total_lines as u32), None);
            prop_assert_eq!(tree.byte_to_line_col(oracle.len() as u32 + 1), None);
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
