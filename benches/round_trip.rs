// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Rust importer on the synthetic fixture: the whole import round trip, and the parser and
//! serializer on their own.

mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use dream_ini::{ImportOptions, IniImporter, parse_cfg_str, parse_ini_str, serialize_cfg};
use support::{Fixture, large_morrowind_ini, large_openmw_cfg};

fn round_trip(c: &mut Criterion) {
    let fixture = Fixture::create("round-trip");
    let importer = IniImporter::new(ImportOptions::default());
    c.bench_function("large_ini_round_trip", |b| {
        b.iter(|| {
            let result = importer
                .import_paths(black_box(&fixture.ini), black_box(&fixture.cfg))
                .expect("import benchmark files");
            black_box(serialize_cfg(&result.cfg));
        });
    });
}

fn parsers(c: &mut Criterion) {
    let ini = large_morrowind_ini();
    let cfg_text = large_openmw_cfg();
    let cfg = parse_cfg_str(&cfg_text);
    c.bench_function("parse_ini_str", |b| {
        b.iter(|| parse_ini_str(black_box(&ini)));
    });
    c.bench_function("parse_cfg_str", |b| {
        b.iter(|| parse_cfg_str(black_box(&cfg_text)));
    });
    c.bench_function("serialize_cfg", |b| {
        b.iter(|| serialize_cfg(black_box(&cfg)));
    });
}

criterion_group!(benches, round_trip, parsers);
criterion_main!(benches);
