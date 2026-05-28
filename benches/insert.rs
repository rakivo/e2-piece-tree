use criterion::{criterion_group, criterion_main, Criterion};
use piece_tree::PieceTree;

const TEXT: &str = include_str!("large.txt");

//----

fn insert_char(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_char");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars() as u32;
            tree.insert_char(rng.u32(0..len), 'a')
        })
    });

    group.bench_function("start", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            tree.insert_char(0, 'a');
        })
    });

    group.bench_function("middle", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert_char(len / 2, 'a');
        })
    });

    group.bench_function("end", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert_char(len, 'a');
        })
    });
}

fn insert_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_small");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(rng.u32(0..len), "a");
        })
    });

    group.bench_function("start", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            tree.insert(0, "a");
        })
    });

    group.bench_function("middle", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(len / 2, "a");
        })
    });

    group.bench_function("end", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(len, "a");
        })
    });
}

fn insert_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_medium");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(rng.u32(0..len), "This is some text.");
        })
    });

    group.bench_function("start", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            tree.insert(0, "This is some text.");
        })
    });

    group.bench_function("middle", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(len / 2, "This is some text.");
        })
    });

    group.bench_function("end", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(len, "This is some text.");
        })
    });
}

const INSERT_TEXT: &str = include_str!("small.txt");

fn insert_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_large");

    group.bench_function("random", |bench| {
        let mut rng = fastrand::Rng::new();
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(rng.u32(0..len), INSERT_TEXT);
        })
    });

    group.bench_function("start", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            tree.insert(0, INSERT_TEXT);
        })
    });

    group.bench_function("middle", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(len / 2, INSERT_TEXT);
        })
    });

    group.bench_function("end", |bench| {
        let mut tree = PieceTree::from_str(TEXT);
        bench.iter(|| {
            let len = tree.len_chars();
            tree.insert(len, INSERT_TEXT);
        })
    });
}

//----

fn insert_after_clone(c: &mut Criterion) {
    c.bench_function("insert_after_clone", |bench| {
        let mut rng = fastrand::Rng::new();
        let tree = PieceTree::from_str(TEXT);
        let mut tree_clone = tree.clone();
        let mut i = 0;
        bench.iter(|| {
            if i > 32 {
                i = 0;
                tree_clone = tree.clone();
            }
            let len = tree_clone.len_chars();
            tree_clone.insert(rng.u32(0..len), "a");
            i += 1;
        })
    });
}

//----

criterion_group!(
    benches,
    insert_char,
    insert_small,
    insert_medium,
    insert_large,
    insert_after_clone
);
criterion_main!(benches);
