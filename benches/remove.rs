use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
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
    let text = mul_string_length(TEXT, LEN_MUL_SMALL);

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = rng.u32(0..(len + 1));
                let end = (start + 1).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("start", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = 0;
                let end = (start + 1).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("middle", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = len / 2;
                let end = (start + 1).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("end", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let end = len;
                let start = end.saturating_sub(1);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });
}

const LEN_MUL_MEDIUM: usize = 1;

fn remove_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_medium");
    let text = mul_string_length(TEXT, LEN_MUL_MEDIUM);

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = rng.u32(0..(len + 1));
                let end = (start + 15).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("start", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = 0;
                let end = (start + 15).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("middle", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = len / 2;
                let end = (start + 15).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("end", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let end = len;
                let start = end.saturating_sub(15);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });
}

const LEN_MUL_LARGE: usize = 4;

fn remove_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_large");
    let text = mul_string_length(TEXT, LEN_MUL_LARGE);
    let remove_len = TEXT_SMALL.len() as u32;

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = rng.u32(0..(len + 1));
                let end = (start + remove_len).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("start", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = 0;
                let end = (start + remove_len).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("middle", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let start = len / 2;
                let end = (start + remove_len).min(len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });

    group.bench_function("end", |bench| {
        bench.iter_batched_ref(
            || PieceTree::from_str(&text),
            |tree| {
                let len = tree.len_chars();
                let end = len;
                let start = end.saturating_sub(remove_len);
                tree.remove(start..end);
            },
            BatchSize::SmallInput,
        )
    });
}

fn remove_initial_after_clone(c: &mut Criterion) {
    let text = mul_string_length(TEXT, 1);

    c.bench_function("remove_initial_after_clone", |bench| {
        let mut rng = fastrand::Rng::new();

        bench.iter_batched_ref(
            // We time how long it takes to remove from a freshly cloned tree.
            // Creating the initial tree and cloning it happens in setup.
            || {
                let tree = PieceTree::from_str(&text);
                tree.clone()
            },
            |tree_clone| {
                let len = tree_clone.len_chars();
                let start = rng.u32(0..(len + 1));
                let end = (start + 1).min(len);
                tree_clone.remove(start..end);
            },
            BatchSize::SmallInput,
        )
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
