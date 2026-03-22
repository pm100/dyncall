//! Benchmark: native Rust file copy vs dyncall file copy (line by line).
//!
//! Also includes a microbenchmark isolating pure per-call dispatch overhead
//! by calling `abs` in a tight loop both ways.
//!
//! Run with:
//!   cargo bench
//! HTML report: target/criterion/report/index.html

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dyncall::DynCaller;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

#[cfg(target_os = "windows")]
const LIBC: &str = "msvcrt.dll";
#[cfg(target_os = "macos")]
const LIBC: &str = "libSystem.B.dylib";
#[cfg(target_os = "linux")]
const LIBC: &str = "libc.so.6";

const LINE_COUNT: usize = 50_000;
const LINE: &str =
    "The quick brown fox jumps over the lazy dog. 0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij\n";

fn make_input_file(path: &PathBuf) {
    let mut f = BufWriter::new(fs::File::create(path).unwrap());
    for _ in 0..LINE_COUNT {
        f.write_all(LINE.as_bytes()).unwrap();
    }
}

// ── File copy benchmarks ──────────────────────────────────────────────────────

fn bench_file_copy(c: &mut Criterion) {
    let input = std::env::temp_dir().join("dyncall_bench_in.txt");
    let out_native = std::env::temp_dir().join("dyncall_bench_out_native.txt");
    let out_dyncall = std::env::temp_dir().join("dyncall_bench_out_dyncall.txt");
    make_input_file(&input);

    let input_str = input.to_str().unwrap().to_string();
    let out_dyncall_str = out_dyncall.to_str().unwrap().to_string();

    // Pre-compile FuncDefs outside the timed loop
    let fopen_def =
        DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|")).unwrap();
    let fgets_def = DynCaller::define_function(&format!(
        "{LIBC}|fgets|ocstr=arg1,i32,ptr|ptr|"
    ))
    .unwrap();
    let fputs_def =
        DynCaller::define_function(&format!("{LIBC}|fputs|cstr,ptr|i32|")).unwrap();
    let fclose_def =
        DynCaller::define_function(&format!("{LIBC}|fclose|ptr|i32|")).unwrap();

    let buf_size = 1024i32;

    let mut group = c.benchmark_group("file_copy_50k_lines");

    group.bench_function("native", |b| {
        b.iter(|| {
            let reader = BufReader::new(fs::File::open(&input).unwrap());
            let mut writer = BufWriter::new(fs::File::create(&out_native).unwrap());
            for line in reader.lines() {
                writeln!(writer, "{}", line.unwrap()).unwrap();
            }
        });
    });

    group.bench_function("dyncall", |b| {
        b.iter(|| {
            let mut inv = fopen_def.prep();
            inv.push_arg(&input_str).unwrap();
            inv.push_arg(&"r".to_string()).unwrap();
            let in_fp = inv.call().unwrap();

            let mut inv = fopen_def.prep();
            inv.push_arg(&out_dyncall_str).unwrap();
            inv.push_arg(&"w".to_string()).unwrap();
            let out_fp = inv.call().unwrap();

            loop {
                let mut buf = String::new();
                let mut inv = fgets_def.prep();
                inv.push_mut_arg(&mut buf).unwrap();
                inv.push_arg(&buf_size).unwrap();
                inv.push_arg(&in_fp).unwrap();
                let ret = inv.call().unwrap();
                if ret.as_pointer().unwrap().is_null() {
                    break;
                }

                let mut inv = fputs_def.prep();
                inv.push_arg(&buf).unwrap();
                inv.push_arg(&out_fp).unwrap();
                inv.call().unwrap();
            }

            let mut inv = fclose_def.prep();
            inv.push_arg(&in_fp).unwrap();
            inv.call().unwrap();

            let mut inv = fclose_def.prep();
            inv.push_arg(&out_fp).unwrap();
            inv.call().unwrap();
        });
    });

    group.finish();
}

// ── Call overhead microbenchmark (abs × 10_000) ──────────────────────────────

fn bench_call_overhead(c: &mut Criterion) {
    let abs_def =
        DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();

    let mut group = c.benchmark_group("call_overhead_10k_abs");

    // Native: Rust integer abs — establishes the absolute floor
    group.bench_function("native_rust_abs", |b| {
        b.iter(|| {
            let mut sum = 0i32;
            for i in -5000i32..5000 {
                sum = sum.wrapping_add(black_box(i).abs());
            }
            black_box(sum)
        });
    });

    // Dyncall: same calls via FFI dispatch
    group.bench_function("dyncall_abs", |b| {
        b.iter(|| {
            let mut sum = 0i32;
            for i in -5000i32..5000 {
                let mut inv = abs_def.prep();
                inv.push_arg(&black_box(i)).unwrap();
                let ret = inv.call().unwrap();
                sum = sum.wrapping_add(*ret.as_i32().unwrap());
            }
            black_box(sum)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_file_copy, bench_call_overhead);
criterion_main!(benches);
