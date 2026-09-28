// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Rust importer on the synthetic fixture: parse, import, serialize.

mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use dream_ini::{ImportOptions, IniImporter, serialize_cfg};
use support::Fixture;

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

criterion_group!(benches, round_trip);
criterion_main!(benches);
