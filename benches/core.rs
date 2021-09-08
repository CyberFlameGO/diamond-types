// This benchmark interacts with the automerge-perf data set from here:
// https://github.com/automerge/automerge-perf/
// mod testdata;

mod utils;

use criterion::{criterion_group, criterion_main, black_box, Criterion, BenchmarkId, Throughput};
use crdt_testdata::{load_testing_data, TestData};
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

fn testing_data(name: &str) -> TestData {
    let filename = format!("benchmark_data/{}.json.gz", name);
    load_testing_data(&filename)
}

fn list_with_data(test_data: &TestData) -> ListCRDT {
    assert_eq!(test_data.start_content.len(), 0);

    let mut doc = ListCRDT::new();
    apply_edits(&mut doc, &test_data.txns);
    doc
}

fn remote_benchmarks(c: &mut Criterion) {
    for name in &["automerge-paper", "rustcode", "sveltecomponent"] {
        let mut group = c.benchmark_group("remote");
        let test_data = testing_data(name);
        let src_doc = list_with_data(&test_data);

        group.throughput(Throughput::Elements(test_data.len() as u64));

        group.bench_function(BenchmarkId::new( "generate", name), |b| {
            b.iter(|| {
                let remote_edits: Vec<_> = src_doc.get_all_txns();
                black_box(remote_edits);
            })
        });

        let remote_edits: Vec<_> = src_doc.get_all_txns();
        group.bench_function(BenchmarkId::new( "apply", name), |b| {
            b.iter(|| {
                let mut doc = ListCRDT::new();
                for txn in remote_edits.iter() {
                    doc.apply_remote_txn(&txn);
                }
                assert_eq!(doc.len(), src_doc.len());
                // black_box(doc.len());
            })
        });

        group.finish();
    }
}

fn ot_benchmarks(c: &mut Criterion) {
    for name in &["automerge-paper", "rustcode", "sveltecomponent"] {
        let mut group = c.benchmark_group("ot");
        let test_data = testing_data(name);
        let doc = list_with_data(&test_data);
        group.throughput(Throughput::Elements(test_data.len() as u64));

        group.bench_function(BenchmarkId::new("traversal_since", name), |b| {
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