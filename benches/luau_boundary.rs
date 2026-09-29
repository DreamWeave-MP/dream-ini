// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Luau boundary: every module function driven from a script, on the synthetic import
//! fixture `round_trip` uses. The scripts are frozen; the harness is the only thing that changes
//! between binder generations, so the numbers compare.
//!
//! Each script is a factory: it receives the module, the fixture paths, and the file texts once,
//! and returns the closure the benchmark iterates.

mod support;

use std::fs;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use dream_ini::luau::{IniExtension, MODULE};
use l3i::Runtime;
use l3i::extension::RuntimePlan;
use l3i::value::{Function, Table};
use support::Fixture;

/// `(name, factory source)`; the factory takes `(ini, iniPath, cfgPath, iniText, cfgText)`.
const SCRIPTS: &[(&str, &str)] = &[
    (
        "importPaths",
        "return function(ini, iniPath, cfgPath, iniText, cfgText)\n\
         return function() return ini.importPaths({ ini = iniPath, cfg = cfgPath }) end\n\
         end",
    ),
    (
        "importMaps",
        "return function(ini, iniPath, cfgPath, iniText, cfgText)\n\
         local cfg = ini.parseCfg(cfgText)\n\
         local parsed = ini.parseIni(iniText)\n\
         return function() return ini.importMaps(cfg, parsed.entries, { archives = false }) end\n\
         end",
    ),
    (
        "parseIni",
        "return function(ini, iniPath, cfgPath, iniText, cfgText)\n\
         return function() return ini.parseIni(iniText) end\n\
         end",
    ),
    (
        "parseCfg",
        "return function(ini, iniPath, cfgPath, iniText, cfgText)\n\
         return function() return ini.parseCfg(cfgText) end\n\
         end",
    ),
    (
        "serializeCfg",
        "return function(ini, iniPath, cfgPath, iniText, cfgText)\n\
         local cfg = ini.parseCfg(cfgText)\n\
         return function() return ini.serializeCfg(cfg) end\n\
         end",
    ),
];

fn luau_boundary(c: &mut Criterion) {
    let fixture = Fixture::create("luau");
    let ini_text = fs::read_to_string(&fixture.ini).expect("read benchmark ini");
    let cfg_text = fs::read_to_string(&fixture.cfg).expect("read benchmark cfg");
    let ini_path = fixture.ini.to_string_lossy().into_owned();
    let cfg_path = fixture.cfg.to_string_lossy().into_owned();

    let plan = RuntimePlan::builder()
        .extension(IniExtension)
        .finalize()
        .expect("finalize the plan");
    let runtime = Runtime::from_plan(&plan).expect("create the runtime");
    // Evaluating and loading lease the root stack themselves, so nothing holds it until the
    // closures exist.
    let module: Table = runtime
        .eval(&format!("return require('{MODULE}')"))
        .expect("require the module");
    let bodies: Vec<(&str, Function)> = SCRIPTS
        .iter()
        .map(|(name, source)| {
            let factory = runtime
                .load_function(source)
                .expect("load the benchmark script");
            let body: Function = factory
                .invoke(
                    &runtime.stack(),
                    (
                        module.clone(),
                        ini_path.as_str(),
                        cfg_path.as_str(),
                        ini_text.as_str(),
                        cfg_text.as_str(),
                    ),
                )
                .expect("build the benchmark closure");
            (*name, body)
        })
        .collect();

    let mut group = c.benchmark_group("luau_boundary");
    group
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(4))
        .sample_size(30);
    let stack = runtime.stack();
    for (name, body) in &bodies {
        group.bench_function(*name, |b| {
            b.iter(|| {
                body.invoke::<(), ()>(&stack, ())
                    .expect("run the benchmark closure");
            });
        });
    }
    group.finish();
}

criterion_group!(benches, luau_boundary);
criterion_main!(benches);
