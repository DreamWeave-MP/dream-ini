// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `dream.ini` extension through an l3i runtime plan: the plan composes, every member is
//! typed, and the module behaves. `tests/luau_typed.rs` proves the types in Luau's frontend.

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use dream_ini::luau::{EXTENSION_ID, IniExtension, MODULE};
use l3i::Runtime;
use l3i::extension::{RuntimePlan, RuntimePolicy};

fn plan() -> Rc<RuntimePlan> {
    RuntimePlan::builder()
        .extension(IniExtension)
        .finalize()
        .expect("the plan finalizes")
}

fn runtime() -> Runtime {
    Runtime::from_plan(&plan()).expect("the runtime instantiates")
}

/// Runs `script` with `ini` bound to the module.
fn exec(runtime: &Runtime, script: &str) -> l3i::Result<()> {
    runtime.exec(&format!("local ini = require('{MODULE}')\n{script}"))
}

fn error_of(runtime: &Runtime, script: &str) -> String {
    exec(runtime, script).unwrap_err().to_string()
}

#[test]
fn the_plan_declares_a_typed_module() {
    let plan = plan();
    assert_eq!(plan.installation_order(), [EXTENSION_ID, "dream.net"]);
    assert!(
        plan.userdata().iter().all(|u| u.owner != EXTENSION_ID),
        "the surface is tables: no userdata, no tags"
    );
    let definitions = plan.type_definitions();
    assert!(
        definitions.contains("export type Module__dream_ini = {"),
        "{definitions}"
    );
    assert!(
        definitions.contains("    version: string,"),
        "{definitions}"
    );
    for fallback in ["(...any) -> ...any", ": any,\n"] {
        assert!(
            !definitions.contains(fallback),
            "every member is typed:\n{definitions}"
        );
    }
}

#[test]
fn the_module_is_frozen_and_the_global_is_host_policy() {
    let runtime = runtime();
    let error = error_of(&runtime, "ini.parseCfg = nil");
    assert!(error.contains("readonly"), "{error}");
    exec(
        &runtime,
        "assert(dreamIni == nil, 'no global unless the host asks')",
    )
    .unwrap();

    let plan = RuntimePlan::builder()
        .policy(RuntimePolicy::new().compat_global(MODULE, "dreamIni"))
        .extension(IniExtension)
        .finalize()
        .unwrap();
    for _ in 0..2 {
        let runtime = Runtime::from_plan(&plan).unwrap();
        runtime
            .exec("assert(dreamIni == require('@dream/ini')) assert(type(dreamIni.version) == 'string')")
            .unwrap();
    }
}

#[test]
fn parse_cfg_preserves_duplicate_keys() {
    exec(
        &runtime(),
        r#"
        local cfg = ini.parseCfg("key=one\nkey=two\n")
        assert(cfg.key[1] == "one")
        assert(cfg.key[2] == "two")
        assert(#cfg.key == 2)
        "#,
    )
    .unwrap();
}

#[test]
fn serializes_cfg_tables() {
    exec(
        &runtime(),
        r#"assert(ini.serializeCfg({ key = { "one", "two" } }) == "key=one\nkey=two\n")"#,
    )
    .unwrap();
}

#[test]
fn parse_ini_returns_warnings_and_reads_buffers() {
    exec(
        &runtime(),
        r#"
        local result = ini.parseIni("[General]\nEmpty=\n", { encoding = "win1252" })
        assert(result.warnings[1].kind == "ignoredEmptyValue")
        assert(result.warnings[1].key == "General:Empty")
        assert(result.warnings[1].message == "ignored empty value for key 'General:Empty'.")
        local text = "[General]\nDisable Audio=1\n"
        local bytes = buffer.fromstring(text)
        local fromBuffer = ini.parseIni(bytes)
        assert(fromBuffer.entries["General:Disable Audio"][1] == "1")
        assert(#fromBuffer.warnings == 0)
        "#,
    )
    .unwrap();
}

#[test]
fn parse_ini_decodes_with_the_requested_encoding() {
    exec(
        &runtime(),
        r#"
        local latin = ini.parseIni("[General]\nName=caf\xE9\n")
        assert(latin.entries["General:Name"][1] == "caf\u{E9}")
        local cyrillic = ini.parseIni("[General]\nName=\xC4\n", { encoding = "win1251" })
        assert(cyrillic.entries["General:Name"][1] == "\u{414}")
        "#,
    )
    .unwrap();
}

#[test]
fn import_maps_returns_cfg_text_and_report() {
    exec(
        &runtime(),
        r#"
        local cfg = { encoding = { "win1252" } }
        local ini_map = { ["General:Disable Audio"] = { "1" } }
        local result = ini.importMaps(cfg, ini_map, { archives = false })
        assert(result.cfg["no-sound"][1] == "1")
        assert(result.text:find("no%-sound=1\n") ~= nil)
        assert(result.text == ini.serializeCfg(result.cfg))
        assert(#result.warnings == 0)
        assert(#result.events == 0)
        assert(cfg["no-sound"] == nil, "the input table is not modified")
        "#,
    )
    .unwrap();
}

#[test]
fn import_maps_writes_the_requested_encoding() {
    exec(
        &runtime(),
        r#"
        local imported = ini.importMaps({}, {}, { archives = false })
        assert(imported.cfg.encoding[1] == "win1252")
        local cyrillic = ini.importMaps({ encoding = { "win1252" } }, {}, { archives = false, encoding = "win1251" })
        assert(cyrillic.cfg.encoding[1] == "win1251")
        assert(cyrillic.text:find("encoding=win1251\n", 1, true) ~= nil)
        "#,
    )
    .unwrap();
}

#[test]
fn import_paths_uses_explicit_data_dirs() {
    let dir = unique_test_dir("import-paths");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = cfg_dir.join("openmw.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
    fs::write(&cfg, "encoding=win1252\n").unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes()).unwrap();

    let runtime = runtime();
    runtime
        .set_global("INI", &ini.to_string_lossy().into_owned())
        .unwrap();
    runtime
        .set_global("CFG", &cfg.to_string_lossy().into_owned())
        .unwrap();
    runtime
        .set_global(
            "WRONG_BASE",
            &dir.join("wrong-base").to_string_lossy().into_owned(),
        )
        .unwrap();
    runtime
        .set_global("DATA_DIR", &data_dir.to_string_lossy().into_owned())
        .unwrap();
    exec(
        &runtime,
        r#"
        local result = ini.importPaths({
            ini = INI, cfg = CFG, cfgDir = WRONG_BASE, gameFiles = true, archives = false, dataDirs = { "Data Files" },
        })
        assert(result.text:find("content=Base.esm\n", 1, true) ~= nil, result.text)
        assert(result.text:find("data=Data Files\n", 1, true) ~= nil, result.text)
        assert(result.events[1].kind == "dataDirAddedForContent")
        assert(result.events[1].path == DATA_DIR, result.events[1].path)
        assert(result.events[1].modified == nil)
        "#,
    )
    .unwrap();
    // `verbose` reports the resolved content file with its timestamp as a number.
    exec(
        &runtime,
        r#"
        local result = ini.importPaths({ ini = INI, cfg = CFG, gameFiles = true, archives = false, verbose = true, dataDirs = { "Data Files" } })
        assert(result.events[1].kind == "contentFileResolved", result.events[1].kind)
        assert(type(result.events[1].modified) == "number")
        assert(result.events[1].modified > 0)
        "#,
    )
    .unwrap();

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_maps_uses_cfg_dir_for_relative_data_paths() {
    let dir = unique_test_dir("import-maps-cfg-dir");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    for sub in ["Data Files", "Local Data", "resources", "user-data"] {
        fs::create_dir_all(cfg_dir.join(sub)).unwrap();
    }
    fs::write(data_dir.join("Base.esm"), tes3_bytes()).unwrap();

    let runtime = runtime();
    runtime
        .set_global("CFG_DIR", &cfg_dir.to_string_lossy().into_owned())
        .unwrap();
    exec(
        &runtime,
        r#"
        local cfg = { ["data-local"] = { "Local Data" }, resources = { "resources" }, ["user-data"] = { "user-data" } }
        local ini_map = { ["Game Files:GameFile0"] = { "Base.esm" } }
        local result = ini.importMaps(cfg, ini_map, { gameFiles = true, archives = false, dataDirs = { "Data Files" }, cfgDir = CFG_DIR })
        for _, line in { "content=Base.esm\n", "data=Data Files\n", "data-local=Local Data\n", "resources=resources\n", "user-data=user-data\n" } do
            assert(result.text:find(line, 1, true) ~= nil, line .. " in " .. result.text)
        end
        "#,
    )
    .unwrap();

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn options_are_strict_and_name_the_known_keys() {
    let runtime = runtime();
    for (call, expected) in [
        (
            r#"ini.importMaps({}, {}, { archives = false, user_data = "x" })"#,
            "ini.importMaps: unknown option 'user_data'; known options are archives, cfgDir, dataDirs, dataLocal, encoding, fonts, game, gameFiles, iniPath, resources, userData, verbose",
        ),
        (
            r#"ini.importPaths({ ini = "Morrowind.ini", game_files = true })"#,
            "ini.importPaths: unknown option 'game_files'; known options are archives, cfg, cfgDir, dataDirs, dataLocal, encoding, fonts, game, gameFiles, ini, resources, userData, verbose",
        ),
        (
            r#"ini.parseIni("", { gameFiles = true })"#,
            "ini.parseIni: unknown option 'gameFiles'; known options are encoding",
        ),
        (
            r"ini.importPaths({})",
            "ini.importPaths: missing required option 'ini'",
        ),
        (
            r#"ini.importPaths({ ini = "Morrowind.ini", encoding = "utf-8" })"#,
            "ini.importPaths.encoding: unsupported encoding: utf-8",
        ),
        (
            r#"ini.importMaps({}, {}, { game = "oblivion" })"#,
            "ini.importMaps.game: unsupported game: oblivion",
        ),
        // Table options are walked in place by l3i's option reader: an element error spells the
        // field's full path and passes through flat, a non-table is the reader's own wording.
        (
            r#"ini.importMaps({}, {}, { dataDirs = { "a", 2 } })"#,
            "ini.importMaps.dataDirs[2]: expected a string, got number",
        ),
        (
            r#"ini.importMaps({}, {}, { dataDirs = "a" })"#,
            "ini.importMaps.dataDirs: expected table, got string",
        ),
        (
            r"ini.importMaps({}, {}, 5)",
            "ini.importMaps: options must be a table, got number",
        ),
    ] {
        assert_eq!(error_of(&runtime, call), expected, "{call}");
    }
    // The system's words follow the path, so only the prefix is ours to assert.
    let call = r#"ini.importPaths({ ini = "/nonexistent/Morrowind.ini" })"#;
    let error = error_of(&runtime, call);
    assert!(
        error.starts_with("dream.ini: /nonexistent/Morrowind.ini: "),
        "{call}: {error}"
    );
}

#[test]
fn multimaps_must_be_tables_of_string_arrays() {
    let runtime = runtime();
    for (call, expected) in [
        (
            r#"ini.serializeCfg({ key = "value" })"#,
            "ini.serializeCfg: expected array of strings for key 'key', got string",
        ),
        (
            r"ini.serializeCfg({ key = { 1 } })",
            "ini.serializeCfg: expected a string at key 'key' index 1, got number",
        ),
        (
            r#"ini.serializeCfg({ [1] = { "x" } })"#,
            "ini.serializeCfg: multimap keys must be strings, got number",
        ),
        (
            r#"ini.serializeCfg("key=value")"#,
            "ini.serializeCfg: expected a multimap table, got string",
        ),
        (
            r#"ini.importMaps({ key = { "x" } }, 7)"#,
            "ini.importMaps: expected a multimap table, got number",
        ),
    ] {
        assert_eq!(error_of(&runtime, call), expected, "{call}");
    }
}

#[test]
fn reports_version_and_camel_case_kinds() {
    let runtime = runtime();
    runtime
        .set_global("expectedVersion", &env!("CARGO_PKG_VERSION"))
        .unwrap();
    exec(
        &runtime,
        r#"
        assert(ini.version == expectedVersion)
        local result = ini.parseIni("[Game Files\n")
        local warning: { kind: string, line: string?, message: string } = result.warnings[1]
        assert(warning.kind == "malformedIniLine")
        assert(warning.line == "[Game Files")
        assert(warning.message == "ini file wrongly formatted ([Game Files). Line ignored.")
        "#,
    )
    .unwrap();
}

#[test]
fn missing_game_files_stop_the_content_list_with_a_warning() {
    let dir = unique_test_dir("missing-game-file");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes()).unwrap();
    let ini = dir.join("Morrowind.ini");
    fs::write(
        &ini,
        "[Game Files]\nGameFile0=Base.esm\nGameFile1=Missing.esp\nGameFile2=Base.esm\n",
    )
    .unwrap();

    let runtime = runtime();
    runtime
        .set_global("INI", &ini.to_string_lossy().into_owned())
        .unwrap();
    exec(
        &runtime,
        r#"
        local result = ini.importPaths({ ini = INI, gameFiles = true, archives = false })
        assert(#result.cfg.content == 1 and result.cfg.content[1] == "Base.esm")
        assert(result.warnings[1].kind == "missingGameFile")
        assert(result.warnings[1].file == "Missing.esp")
        assert(result.warnings[1].message:find("Missing.esp", 1, true) ~= nil)
        "#,
    )
    .unwrap();

    fs::remove_dir_all(dir).unwrap();
}

fn tes3_bytes() -> Vec<u8> {
    let mut record = Vec::new();
    record.extend_from_slice(b"HEDR");
    record.extend_from_slice(&300u32.to_le_bytes());
    record.extend_from_slice(&[0; 300]);

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TES3");
    bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&record);
    bytes
}

fn unique_test_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    let temp_dir = temp_dir.canonicalize().unwrap_or(temp_dir);
    temp_dir.join(format!(
        "dream-ini-luau-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
