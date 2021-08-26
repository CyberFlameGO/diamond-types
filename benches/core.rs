// This benchmark interacts with the automerge-perf data set from here:
// https://github.com/automerge/automerge-perf/
// mod testdata;

mod utils;

use criterion::{criterion_group, criterion_main, black_box, Criterion, BenchmarkId};
use crdt_testdata::{load_testing_data};
use diamond_types::list::*;
use utils::apply_edits;

fn local_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("local edits");
    for name in &["automerge-paper", "rustcode", "sveltecomponent"] {
        group.bench_with_input(BenchmarkId::new("yjs", name), name, |b, name| {
            let filename = format!("benchmark_data/{}.json.gz", name);
            let test_data = load_testing_data(&filename);
            assert_eq!(test_data.start_content.len(), 0);

            b.iter(|| {
                let mut doc = ListCRDT::new();
                apply_edits(&mut doc, &test_data.txns);
                assert_eq!(doc.len(), test_data.end_content.len());
                black_box(doc.len());
            })
        });
    }

    group.finish();

    c.bench_function("kevin", |b| {
        b.iter(|| {
            let mut doc = ListCRDT::new();

            let agent = doc.get_or_create_agent_id("seph");

            for _i in 0..5000000 {
                doc.local_insert(agent, 0, " ".into());
            }
            black_box(doc.len());
        })
    });
}

fn list_with_data(name: &str) -> ListCRDT {
    let filename = format!("benchmark_data/{}.json.gz", name);
    let test_data = load_testing_data(&filename);
    assert_eq!(test_data.start_content.len(), 0);

    let mut doc = ListCRDT::new();
    apply_edits(&mut doc, &test_data.txns);
    doc
}

fn remote_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate remote edits");
    for name in &["automerge-paper", "rustcode", "sveltecomponent"] {
        group.bench_with_input(BenchmarkId::new("dataset", name), name, |b, name| {
            let src_doc = list_with_data(name);
            b.iter(|| {
                let remote_edits: Vec<_> = src_doc.get_all_txns();
                black_box(remote_edits);
            })
        });
    }

    group.finish();

    let mut group = c.benchmark_group("apply remote edits");
    for name in &["automerge-paper", "rustcode", "sveltecomponent"] {
        group.bench_with_input(BenchmarkId::new("dataset", name), name, |b, name| {
            let src_doc = list_with_data(name);
            let remote_edits: Vec<_> = src_doc.get_all_txns();

            b.iter(|| {
                let mut doc = ListCRDT::new();
                for txn in remote_edits.iter() {
                    doc.apply_remote_txn(&txn);
                }
                assert_eq!(doc.len(), src_doc.len());
                // black_box(doc.len());
            })
        });
    }

    group.finish();
}

fn ot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate ot");
    for name in &["automerge-paper", "rustcode", "sveltecomponent"] {
        group.bench_with_input(BenchmarkId::new("dataset", name), name, |b, name| {
            let doc = list_with_data(name);

            b.iter(|| {
                let changes = doc.traversal_changes_since(0);
                black_box(changes);
            })
        });
    }
}


criterion_group!(benches,
    local_benchmarks,
    remote_benchmarks,
    ot_benchmarks,
);
criterion_main!(benches);