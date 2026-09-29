// SPDX-License-Identifier: MIT OR Apache-2.0

//! The declared types in Luau's own frontend (feature `luau-analysis`): the definitions gate and
//! a strict script over `require("@dream/ini")`. `cargo test --features luau-analysis` runs it;
//! `cargo test --features luau` keeps l3i's analysis frontend out of the build.

use std::collections::HashMap;

use dream_ini::luau::IniExtension;
use l3i::analysis::{
    Analysis, AnalysisOptions, Definitions, Mode, ModuleConfig, SourceCode, SourceProvider,
};
use l3i::extension::RuntimePlan;

struct Scripts(HashMap<&'static str, String>);

impl SourceProvider for Scripts {
    fn read_source(&self, name: &str) -> Option<SourceCode> {
        self.0.get(name).map(|text| SourceCode {
            text: text.clone(),
            is_script: true,
        })
    }
    fn resolve_module(&self, _requirer: &str, _required: &str) -> Option<String> {
        None
    }
    fn module_config(&self, _name: &str) -> ModuleConfig {
        ModuleConfig {
            mode: Mode::Strict,
            ..ModuleConfig::default()
        }
    }
}

const STRICT_SCRIPT: &str = "--!strict
local ini = require('@dream/ini')
local parsed = ini.parseIni('[General]\\nDisable Audio=1\\n', { encoding = 'win1252' })
local entries: { [string]: { string } } = parsed.entries
local first: string? = if #parsed.warnings > 0 then parsed.warnings[1].message else nil
local cfg = ini.parseCfg('content=Morrowind.esm\\n')
local text: string = ini.serializeCfg(cfg)
local imported = ini.importMaps(cfg, entries, { archives = false, fonts = true, iniPath = 'Morrowind.ini' })
local fromDisk = ini.importPaths({ ini = 'Morrowind.ini', cfg = 'openmw.cfg', gameFiles = true, dataDirs = { 'Data Files' } })
for _, event in fromDisk.events do
    local stamp: number? = event.modified
    print(event.kind, event.path, stamp)
end
local version: string = ini.version
print(first, text, imported.text, imported.cfg['no-sound'], #imported.warnings, version)
";

#[test]
fn the_definitions_check_and_a_strict_script_type_checks() {
    let plan = RuntimePlan::builder()
        .extension(IniExtension)
        .finalize()
        .expect("the plan finalizes");
    plan.check_definitions().unwrap();
    let definitions = plan.type_definitions();
    let sources = plan.analysis_sources(Scripts(HashMap::from([(
        "ini_script",
        STRICT_SCRIPT.to_owned(),
    )])));
    let options = AnalysisOptions {
        definitions: vec![Definitions {
            name: "dream-ini.d.luau".to_owned(),
            source: definitions.clone(),
        }],
        ..AnalysisOptions::default()
    };
    let analysis = Analysis::new(sources, options).unwrap_or_else(|error| panic!("{error}"));
    let report = analysis.check("ini_script", false);
    let text: Vec<String> = report
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "ini_script:{}:{}: {}",
                d.span.begin_line + 1,
                d.span.begin_column + 1,
                d.text
            )
        })
        .collect();
    assert!(report.is_clean(), "{}\n---\n{definitions}", text.join("\n"));
}
