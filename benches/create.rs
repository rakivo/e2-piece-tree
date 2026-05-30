#![allow(unused_must_use)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use piece_tree::PieceTree;

const TEXT_SMALL: &str = include_str!("small.txt");
const TEXT_MEDIUM: &str = include_str!("medium.txt");
const TEXT_LARGE: &str = include_str!("large.txt");
const TEXT_LF: &str = include_str!("lf.txt");

//----

fn from_str(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_str");

    group.bench_function("small", |bench| {
        bench.iter(|| {
            PieceTree::from(black_box(TEXT_SMALL));
        })
    });

    group.bench_function("medium", |bench| {
        bench.iter(|| {
            PieceTree::from(black_box(TEXT_MEDIUM));
        })
    });

    group.bench_function("large", |bench| {
        bench.iter(|| {
            PieceTree::from(black_box(TEXT_LARGE));
        })
    });

    group.bench_function("linefeeds", |bench| {
        bench.iter(|| {
            PieceTree::from(black_box(TEXT_LF));
        })
    });
}

//----

criterion_group!(benches, from_str);
criterion_main!(benches);
