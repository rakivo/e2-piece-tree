extern crate criterion;
extern crate fastrand;

use criterion::{criterion_group, criterion_main, Criterion};
use piece_tree::PieceTree;

const TEXT: &str = include_str!("large.txt");
const TEXT_SMALL: &str = include_str!("small.txt");

fn mul_string_length(text: &str, n: usize) -> String {
    let mut mtext = String::new();
    for _ in 0..n {
        mtext.push_str(text);
    }
    mtext
}

//----

const LEN_MUL_SMALL: usize = 1;

fn remove_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_small");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let text = mul_string_length(TEXT, LEN_MUL_SMALL);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = rng.u32(0..(len + 1));
            let end = (start + 1).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("start", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_SMALL);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = 0;
            let end = (start + 1).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("middle", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_SMALL);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = len / 2;
            let end = (start + 1).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("end", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_SMALL);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let end = len;
            let start = end - (1).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });
}

const LEN_MUL_MEDIUM: usize = 1;

fn remove_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_medium");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let text = mul_string_length(TEXT, LEN_MUL_MEDIUM);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = rng.u32(0..(len + 1));
            let end = (start + 15).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("start", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_MEDIUM);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = 0;
            let end = (start + 15).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("middle", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_MEDIUM);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = len / 2;
            let end = (start + 15).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("end", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_MEDIUM);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let end = len;
            let start = end - (15).min(len);
            tree.remove(start..end);

            if tree.len_bytes() < TEXT.len() as u32 / 2 {
                tree = PieceTree::from(&text);
            }
        })
    });
}

const LEN_MUL_LARGE: usize = 4;

fn remove_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_large");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let text = mul_string_length(TEXT, LEN_MUL_LARGE);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = rng.u32(0..(len + 1));
            let end = (start + TEXT_SMALL.len() as u32).min(len);
            tree.remove(start..end);

            if tree.len_bytes() == 0 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("start", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_LARGE);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = 0;
            let end = (start + TEXT_SMALL.len() as u32).min(len);
            tree.remove(start..end);

            if tree.len_bytes() == 0 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("middle", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_LARGE);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let start = len / 2;
            let end = (start + TEXT_SMALL.len() as u32).min(len);
            tree.remove(start..end);

            if tree.len_bytes() == 0 {
                tree = PieceTree::from(&text);
            }
        })
    });

    group.bench_function("end", |bench| {
        let text = mul_string_length(TEXT, LEN_MUL_LARGE);
        let mut tree = PieceTree::from(&text);

        bench.iter(|| {
            let len = tree.len_chars();
            let end = len;
            let start = end - (TEXT_SMALL.len() as u32).min(len);
            tree.remove(start..end);

            if tree.len_bytes() == 0 {
                tree = PieceTree::from(&text);
            }
        })
    });
}

fn remove_initial_after_clone(c: &mut Criterion) {
    c.bench_function("remove_initial_after_clone", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from(TEXT);
        let mut tree_clone = tree.clone();
        let mut i = 0;
        bench.iter(|| {
            if i > 32 {
                i = 0;
                tree_clone = tree.clone();
            }
            let len = tree_clone.len_chars();
            let start = rng.u32(0..(len + 1));
            let end = (start + 1).min(len);
            tree_clone.remove(start..end);
            i += 1;
        })
    });
}

//----

criterion_group!(
    benches,
    remove_small,
    remove_medium,
    remove_large,
    remove_initial_after_clone
);
criterion_main!(benches);
