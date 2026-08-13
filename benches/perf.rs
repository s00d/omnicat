//! Local / nightly performance benches (not run in default CI).
//!
//!   cargo bench --bench perf

use std::hint::black_box;
use std::io::Cursor;
use std::io::Write;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use omnicat::bench_support::{
    open_reader, parse_dump_select, query_sql_dump, LineIter, OmnicatConfig, ProgressReporter,
};
use omnicat::detect::HandlerKind;
use omnicat::inspect::find;
use tempfile::NamedTempFile;

fn line_scan_throughput(c: &mut Criterion) {
    let mut data = Vec::with_capacity(32 * 1024 * 1024);
    for i in 0..500_000 {
        writeln!(data, "line-{i}-abcdefghijklmnopqrstuvwxyz").unwrap();
    }
    let bytes = data.len() as u64;
    let mut g = c.benchmark_group("line_scan_throughput");
    g.throughput(Throughput::Bytes(bytes));
    g.bench_function("visit_lines_500k", |b| {
        b.iter(|| {
            let mut iter = LineIter::new(Cursor::new(black_box(&data[..])), None);
            let mut n = 0u64;
            iter.visit_lines(|_, line| {
                n += line.len() as u64;
                std::ops::ControlFlow::Continue(())
            })
            .unwrap();
            black_box(n);
        });
    });
    g.finish();
}

fn find_substring_lines(c: &mut Criterion) {
    let mut file = NamedTempFile::new().unwrap();
    for i in 0..200_000 {
        writeln!(file, "plain log line {i} status=ok").unwrap();
    }
    writeln!(file, "needle-hit-unique-xyz").unwrap();
    file.flush().unwrap();
    let path = file.path().to_path_buf();
    let mut cfg = OmnicatConfig::default();
    cfg.inspect.max_matches = 1;
    cfg.inspect.no_limit = true;
    cfg.inspect.max_bytes = 0;

    c.bench_function("find_substring_1_of_200k", |b| {
        b.iter(|| {
            let report = find::build_find(
                &path,
                path.to_str().unwrap_or("bench"),
                HandlerKind::Code,
                "needle-hit-unique-xyz",
                &cfg,
                false,
            )
            .unwrap();
            black_box(report);
        });
    });
}

fn parse_insert_streaming(c: &mut Criterion) {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "CREATE TABLE t (id INT);\nINSERT INTO t VALUES ").unwrap();
    for i in 0..100_000 {
        if i > 0 {
            write!(file, ",").unwrap();
        }
        write!(file, "({i})").unwrap();
    }
    writeln!(file, ";").unwrap();
    file.flush().unwrap();

    let mut cfg = OmnicatConfig::default();
    cfg.inspect.no_limit = true;
    cfg.inspect.max_bytes = 0;
    let sel = parse_dump_select("SELECT * FROM t LIMIT 5").unwrap();

    c.bench_function("dump_query_limit_vs_100k_tuples", |b| {
        b.iter(|| {
            let mut progress = ProgressReporter::new("bench", false);
            let out = query_sql_dump(file.path(), &sel, &cfg, 1000, &mut progress).unwrap();
            assert_eq!(out.rows.len(), 5);
            black_box(out);
        });
    });
}

fn open_gz_wrap(c: &mut Criterion) {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut raw = Vec::new();
    for i in 0..200_000 {
        writeln!(raw, "gz-line-{i}").unwrap();
    }
    let mut file = NamedTempFile::with_suffix(".txt.gz").unwrap();
    {
        let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(&raw).unwrap();
        let gz = enc.finish().unwrap();
        file.write_all(&gz).unwrap();
        file.flush().unwrap();
    }

    c.bench_function("open_gz_decode_lines", |b| {
        b.iter(|| {
            let reader = open_reader(file.path(), true).unwrap();
            let mut iter = LineIter::new(reader, None);
            let mut n = 0u64;
            iter.visit_lines(|_, _| {
                n += 1;
                std::ops::ControlFlow::Continue(())
            })
            .unwrap();
            black_box(n);
        });
    });
}

criterion_group!(
    benches,
    line_scan_throughput,
    find_substring_lines,
    parse_insert_streaming,
    open_gz_wrap
);
criterion_main!(benches);
