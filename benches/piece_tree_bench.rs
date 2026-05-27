use piece_tree::PieceTree;

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};

const LOREM_IPSUM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

fn bench_inserts(c: &mut Criterion) {
    let mut group = c.benchmark_group("PieceTree Inserts");

    //
    // Sequential typing
    // Tell Criterion we are processing N characters per iteration
    //
    let char_count = LOREM_IPSUM.chars().count() as u64;
    group.throughput(Throughput::Elements(char_count));

    group.bench_function("sequential_append_chars", |b| {
        b.iter_batched(
            || PieceTree::new(),
            |mut tree| {
                let mut offset = 0;
                for ch in LOREM_IPSUM.chars() {
                    let mut buf = [0; 4];
                    let s = ch.encode_utf8(&mut buf);
                    tree.insert(offset, s);
                    offset += s.len() as u32;
                }
                black_box(tree)
            },
            BatchSize::SmallInput,
        )
    });

    //
    // Random edits
    // Tell Criterion we are doing exactly 100 insert operations per iteration
    //
    group.throughput(Throughput::Elements(100));

    group.bench_function("random_inserts_100_ops", |b| {
        b.iter_batched(
            || {
                let mut tree = PieceTree::new();
                tree.insert(0, LOREM_IPSUM.repeat(100).as_str());
                let rng = StdRng::seed_from_u64(42);
                (tree, rng)
            },

            |(mut tree, mut rng)| {
                for _ in 0..100 {
                    let offset = rng.gen_range(0..=tree.total_length());
                    tree.insert(offset, "hello ");
                }

                black_box(tree)
            },

            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_removes(c: &mut Criterion) {
    let mut group = c.benchmark_group("PieceTree Removes");

    //
    // Tell Criterion we are doing exactly 100 remove operations per iteration
    //
    group.throughput(Throughput::Elements(100));

    group.bench_function("random_removes_100_ops", |b| {
        b.iter_batched(
            || {
                let mut tree = PieceTree::new();
                tree.insert(0, LOREM_IPSUM.repeat(200).as_str());
                let rng = StdRng::seed_from_u64(42);
                (tree, rng)
            },

            |(mut tree, mut rng)| {
                for _ in 0..100 {
                    let total = tree.total_length();
                    if total < 5 { break; }
                    let offset = rng.gen_range(0..total - 5);
                    tree.remove_at(offset, 5);
                }
                black_box(tree)
            },

            BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("PieceTree Reads");

    //
    // Setup the tree outside the benchmark closure to get its exact byte length
    //
    let mut tree = PieceTree::new();
    tree.insert(0, LOREM_IPSUM.repeat(200).as_str());
    let mut rng = StdRng::seed_from_u64(42);
    for _ in 0..500 {
        let offset = rng.gen_range(0..=tree.total_length());
        tree.insert(offset, "frag");
    }

    //
    // Tell Criterion exactly how many BYTES we are reading per iteration
    //
    let tree_byte_len = tree.total_length() as u64;
    group.throughput(Throughput::Bytes(tree_byte_len));

    group.bench_function("to_string_allocating_fragmented", |b| {
        b.iter(|| {
            black_box(tree.to_string_allocating())
        })
    });

    group.finish();
}

criterion_group!(benches, bench_inserts, bench_removes, bench_reads);
criterion_main!(benches);
