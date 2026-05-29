# Piece Tree

Purely functional (immutable) implementation of Piece Tree, inspired by [fredbuf](https://github.com/cdacamar/fredbuf).

## Usage

> You can access full documentation at: https://docs.rs/piece-tree/0.1.2/piece_tree.

### Basic editing

```rust
use piece_tree::PieceTree;

let mut tree = PieceTree::new();

// Insert text at a byte offset
tree.insert(0, "Hello, World!");
assert_eq!(tree.to_string(), "Hello, World!");

// Insert in the middle
tree.insert(7, "beautiful ");
assert_eq!(tree.to_string(), "Hello, beautiful World!");

// Remove a range by byte offset and length
tree.remove(7..17);
assert_eq!(tree.to_string(), "Hello, World!");
```

### Coordinate queries

All coordinate functions work in O(log n) time.

```rust
use piece_tree::PieceTree;

let mut tree = PieceTree::new();
tree.insert(0, "Hello\n🌐world\r\nПривет");

// Char index <-> byte offset
assert_eq!(tree.char_to_byte(6), Some(6));   // '🌐' starts     at byte 6
assert_eq!(tree.char_to_byte(7), Some(10));  // 'w' after 🌐 is at byte 10
assert_eq!(tree.byte_to_char(10), Some(7));

// Line number -> byte offset of line start (0-indexed)
assert_eq!(tree.line_to_byte(0), Some(0));
assert_eq!(tree.line_to_byte(1), Some(6));
assert_eq!(tree.line_to_byte(2), Some(17));

// Byte offset -> (line, col) where col is a char count
assert_eq!(tree.byte_to_line_col(0),  Some((0, 0)));
assert_eq!(tree.byte_to_line_col(10), Some((1, 1))); // 'w', line 1, col 1
assert_eq!(tree.byte_to_line_col(17), Some((2, 0))); // 'П', line 2, col 0
```

### Undo and redo

Each call to `insert` or `remove` automatically pushes an undo entry.
`try_undo` and `try_redo` return the cursor offset that was saved with the entry.

```rust
use piece_tree::PieceTree;

let mut tree = PieceTree::new();
tree.insert(0, "Hello");
tree.insert(5, ", World!");
assert_eq!(tree.to_string(), "Hello, World!");

tree.try_undo(0);
assert_eq!(tree.to_string(), "Hello");

tree.try_redo(0);
assert_eq!(tree.to_string(), "Hello, World!");
```

Use `begin_undo_group` and `end_undo_group` to batch multiple mutations into a single undo step. For manual control, use the `*_no_commit` variants to make silent changes, followed by `commit_head` call to record a checkpoint.

```rust
fn main() {
    use piece_tree::PieceTree;

    let mut tree = PieceTree::new();
    tree.insert(0, "Hello");

    tree.begin_undo_group(5);

    // Type ", World!" character by character without flooding undo history
    tree.insert(5, ",");
    tree.insert(6, " ");
    tree.insert(7, "World!");

    tree.end_undo_group();

    tree.try_undo(0);
    assert_eq!(tree.to_string(), "Hello");
}
```

## Tree Snapshotting

```
fn main() {
    use piece_tree::PieceTree;

    let mut tree = PieceTree::new();
    let mut cursor = 0;

    // Basic typing and taking a snapshot
    tree.insert(cursor, "Hello ");   cursor += 6;
    tree.insert(cursor, "World!");   cursor += 6;
    let snap = tree.take_snapshot(cursor); // Saves "Hello World!" state

    // Batch mutations into a single undo group
    tree.begin_undo_group(cursor);
    tree.insert_no_commit(cursor, " Everything"); cursor += 11;
    tree.insert_no_commit(cursor, " is great!");  cursor += 10;
    tree.end_undo_group();
    assert_eq!(tree.to_string(), "Hello World! Everything is great!");

    // Undo and Redo the transaction group
    cursor = tree.try_undo(cursor).unwrap();
    assert_eq!(tree.to_string(), "Hello World!");

    cursor = tree.try_redo(cursor).unwrap();
    assert_eq!(tree.to_string(), "Hello World! Everything is great!");

    // Revert to snapshot (and show that the jump itself can be undone)
    cursor = tree.snap_to(snap, cursor);
    assert_eq!(tree.to_string(), "Hello World!");

    cursor = tree.try_undo(cursor).unwrap();
    assert_eq!(tree.to_string(), "Hello World! Everything is great!");
}
```

## Vendoring

To keep `piece-tree` highly optimized and tailored for its specific use case, portions of the following third-party crates have been integrated directly into the source code:

* **`cranelift-entity`** (Apache-2.0 with LLVM Exception).
* **`smallvec`** (MIT).
* **`bytecount`** (MIT).

This copy-pasted code remains under its original respective licenses. Full attribution notices are maintained at the top of the relevant source files, and the complete license texts can be found in [THIRD-PARTY-LICENSES.md](./THIRD-PARTY-LICENSES.md).

If you prefer to use the upstream, non-vendored versions of these crates via Cargo, you can enable the `dont_vendor` feature flag.
