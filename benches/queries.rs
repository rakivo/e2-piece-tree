#![allow(unused_must_use)]

use criterion::{criterion_group, criterion_main, Criterion};
use piece_tree::PieceTree;

const TEXT: &str = include_str!("large.txt");
const SMALL_TEXT: &str = include_str!("small.txt");

//----

fn index_convert(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_convert");

    group.bench_function("byte_to_char", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_bytes();
        bench.iter(|| {
            tree.byte_to_char(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("byte_to_line", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_bytes();
        bench.iter(|| {
            tree.byte_to_line(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("char_to_byte", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            tree.char_to_byte(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("char_to_line", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            tree.char_to_line(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("line_to_byte", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_lines();
        bench.iter(|| {
            tree.line_to_byte(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("line_to_char", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_lines();
        bench.iter(|| {
            tree.line_to_char(rng.u32(0..(len + 1)));
        })
    });
}

fn get(c: &mut Criterion) {
    let mut group = c.benchmark_group("get");

    group.bench_function("byte", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_bytes();
        bench.iter(|| {
            tree.byte(rng.u32(0..len));
        })
    });

    group.bench_function("char", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            tree.char(rng.u32(0..len));
        })
    });

    group.bench_function("line", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_lines();
        bench.iter(|| {
            tree.line(rng.u32(0..len));
        })
    });

    group.bench_function("chunk_at_byte", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_bytes();
        bench.iter(|| {
            tree.chunk_at_byte(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("chunk_at_byte_slice", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let slice = tree.slice(324..(tree.len_chars() - 213));
        let len = slice.len_bytes();
        bench.iter(|| {
            slice.chunk_at_byte(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("chunk_at_char", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            tree.chunk_at_char(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("chunk_at_char_slice", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let slice = tree.slice(324..(tree.len_chars() - 213));
        let len = slice.len_chars();
        bench.iter(|| {
            slice.chunk_at_char(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("chunk_at_line_break", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_lines();
        bench.iter(|| {
            tree.chunk_at_line_break(rng.u32(0..(len + 1)));
        })
    });

    group.bench_function("chunk_at_line_break_slice", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let slice = tree.slice(324..(tree.len_chars() - 213));
        let len = slice.len_lines();
        bench.iter(|| {
            slice.chunk_at_line_break(rng.u32(0..(len + 1)));
        })
    });
}

fn slice(c: &mut Criterion) {
    let mut group = c.benchmark_group("slice");

    group.bench_function("slice", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            let mut start = rng.u32(0..(len + 1));
            let mut end = rng.u32(0..(len + 1));
            if start > end {
                std::mem::swap(&mut start, &mut end);
            }
            tree.slice(start..end);
        })
    });

    group.bench_function("slice_small", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            let mut start = rng.u32(0..(len + 1));
            if start > (len - 65) {
                start = len - 65;
            }
            let end = start + 64;
            tree.slice(start..end);
        })
    });

    group.bench_function("slice_from_small_tree", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(SMALL_TEXT);
        let len = tree.len_chars();
        bench.iter(|| {
            let mut start = rng.u32(0..(len + 1));
            let mut end = rng.u32(0..(len + 1));
            if start > end {
                std::mem::swap(&mut start, &mut end);
            }
            tree.slice(start..end);
        })
    });

    group.bench_function("slice_whole_tree", |bench| {
        let tree = PieceTree::from(TEXT);
        bench.iter(|| {
            tree.slice(..);
        })
    });

    group.bench_function("slice_whole_slice", |bench| {
        let tree = PieceTree::from(TEXT);
        let len = tree.len_chars();
        let slice = tree.slice(1..len - 1);
        bench.iter(|| {
            slice.slice(..);
        })
    });
}

//----

criterion_group!(benches, index_convert, get, slice,);
criterion_main!(benches);
