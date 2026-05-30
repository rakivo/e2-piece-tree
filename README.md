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

```rust
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

## Benchmarks vs ropey's Rope

While these structures address different use cases, this crate is a purely functional Red-Black piece tree offering `O(1)` snapshotting and slicing, whereas `ropey` is a mutable B-tree optimized for cache-local, in-place editing, we can still compare their performance profiles.

TL;DR: Slicing is 100-300x faster, and line-based access (including byte/line conversions) is roughly 10-45x faster. However, insertions **at the start/middle** are 6-10x slower: peaking at ~2-5µs for 1.5MB inserts, while insertions **at the end** are faster and removes remain roughly equivalent.

| Benchmark Group | e2-piece-tree | ropey's Rope |
| :--- | :--- | :--- |
| **from_str** | | |
| `from_str/large` | 1886.9 ± 1.33 µs | 1002.6 ± 412.22 µs |
| `from_str/linefeeds` | 36.2 ± 9.60 µs | 6.0 ± 1.39 µs |
| `from_str/medium` | 264.7 ± 0.24 µs | 139.7 ± 10.71 µs |
| `from_str/small` | 2.6 ± 0.01 µs | 1186.1 ± 8.36 ns |
| **get** | | |
| `get/byte` | 7.0 ± 0.00 ns | 70.9 ± 2.38 ns |
| `get/char` | 122.3 ± 0.67 ns | 144.9 ± 3.77 ns |
| `get/chunk_at_byte` | 4.9 ± 0.00 ns | 62.7 ± 6.42 ns |
| `get/chunk_at_byte_slice`| 7.6 ± 0.01 ns | 70.4 ± 0.27 ns |
| `get/chunk_at_char` | 117.6 ± 0.41 ns | 67.8 ± 0.83 ns |
| `get/chunk_at_char_slice`| 316.1 ± 0.96 ns | 70.2 ± 4.07 ns |
| `get/chunk_at_line_break`| 5.8 ± 0.01 ns | 66.8 ± 4.98 ns |
| `get/chunk_at_line_break_slice`| 119.3 ± 4.18 ns | 71.9 ± 4.20 ns |
| `get/line` | 14.9 ± 0.01 ns | 453.4 ± 35.59 ns |
| **index_convert** | | |
| `index_convert/byte_to_char`| 81.2 ± 0.34 ns | 74.2 ± 0.68 ns |
| `index_convert/byte_to_line`| 123.5 ± 21.21 ns | 83.8 ± 11.82 ns |
| `index_convert/char_to_byte`| 113.8 ± 0.75 ns | 126.3 ± 6.31 ns |
| `index_convert/char_to_line`| 249.6 ± 1.19 ns | 153.1 ± 17.01 ns |
| `index_convert/line_to_byte`| 6.6 ± 0.02 ns | 292.7 ± 13.54 ns |
| `index_convert/line_to_char`| 94.5 ± 0.56 ns | 294.2 ± 1.77 ns |
| **insert** | | |
| `insert_after_clone` | 537.7 ± 5.19 ns | 1562.7 ± 100.31 ns |
| `insert_char/start` | 1112.7 ± 121.29 ns | 112.3 ± 3.74 ns |
| `insert_char/middle` | 622.4 ± 52.70 ns | 155.4 ± 4.91 ns |
| `insert_char/end` | 48.1 ± 2.82 ns | 162.9 ± 2.23 ns |
| `insert_char/random` | 1944.6 ± 378.40 ns| 332.4 ± 74.61 ns |
| `insert_small/start` | 1143.2 ± 99.71 ns | 95.6 ± 16.34 ns |
| `insert_small/middle` | 650.0 ± 49.08 ns | 155.9 ± 10.94 ns |
| `insert_small/end` | 49.2 ± 3.95 ns | 166.5 ± 5.60 ns |
| `insert_small/random` | 1930.7 ± 369.78 ns| 333.8 ± 55.31 ns |
| `insert_medium/start` | 1195.3 ± 106.69 ns| 186.0 ± 5.83 ns |
| `insert_medium/middle` | 1324.6 ± 100.89 ns| 250.3 ± 2.97 ns |
| `insert_medium/end` | 68.9 ± 11.38 ns | 243.0 ± 3.71 ns |
| `insert_medium/random` | 2.0 ± 0.38 µs | 395.3 ± 73.67 ns |
| `insert_large/start` | 3.9 ± 0.55 µs | 1773.7 ± 474.86 ns|
| `insert_large/middle` | 4.2 ± 0.53 µs | 2.3 ± 0.13 µs |
| `insert_large/end` | 2.3 ± 0.13 µs | 2.3 ± 0.38 µs |
| `insert_large/random` | 4.7 ± 0.83 µs | 2.7 ± 0.40 µs |
| **remove** | | |
| `remove_initial_after_clone`| 744.5 ± 1.90 ns | 862.4 ± 107.87 ns |
| `remove_small/start` | 230.0 ± 18.89 ns | 145.8 ± 5.28 ns |
| `remove_small/middle` | 293.9 ± 18.56 ns | 180.0 ± 7.63 ns |
| `remove_small/end` | 229.6 ± 13.70 ns | 181.9 ± 19.80 ns |
| `remove_small/random` | 2.9 ± 0.59 µs | 276.8 ± 4.09 ns |
| `remove_medium/start` | 296.0 ± 10.74 ns | 242.6 ± 15.83 ns |
| `remove_medium/middle` | 560.0 ± 30.23 ns | 347.3 ± 15.83 ns |
| `remove_medium/end` | 306.6 ± 13.19 ns | 299.2 ± 44.31 ns |
| `remove_medium/random`| 3.1 ± 0.61 µs | 372.8 ± 14.77 ns |
| `remove_large/start` | 1912.1 ± 510.75 ns| 2.1 ± 0.31 µs |
| `remove_large/middle` | 2.2 ± 0.54 µs | 2.6 ± 0.32 µs |
| `remove_large/end` | 1852.8 ± 524.83 ns| 1930.4 ± 302.20 ns|
| `remove_large/random` | 4.2 ± 0.90 µs | 2.6 ± 0.45 µs |
| **slice** | | |
| `slice/slice` | 2.6 ± 0.00 ns | 866.6 ± 6.02 ns |
| `slice/slice_small` | 1.6 ± 0.38 ns | 245.6 ± 24.54 ns |
| `slice/slice_whole_slice` | 0.0 ± 0.02 ns | 0.3 ± 0.01 ns |
| `slice/slice_from_small_*`| 2.6 ± 0.00 ns | 424.8 ± 56.02 ns |
| `slice/slice_whole_*` | 0.1 ± 0.02 ns | 29.8 ± 7.83 ns |
