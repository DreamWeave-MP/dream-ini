// SPDX-License-Identifier: MIT OR Apache-2.0

//! Luau bindings for `dream-ini`.
//!
//! Enable the `lua` feature to build an `mlua` interface for embedding applications that choose an
//! `mlua` runtime at their own top level. Enable `standalone-lua` only for this crate's tests and
//! documentation builds; it selects `mlua`'s [Luau](https://luau.org) backend. `DreamWeave` hosts
//! run Luau, and these bindings are tested against it. Building this crate with `lua` but no `mlua`
//! runtime selected is intentionally incomplete.
//!
//! Version 0.3.0 moved the bindings from `LuaJIT` to Luau and renamed the Lua-facing surface to
//! Luau conventions: functions, option fields, result fields, and `kind` values are camelCase
//! (`importPaths`, `gameFiles`, `dataDirs`, `"contentFileResolved"`). The conventional global name
//! is `dreamIni`.
//!
//! # Registration
//!
//! [`create_module`] returns a table; [`register`] assigns it to the `dreamIni` global. The crate
//! does not export a C Lua module or install a `require` loader; embedders that want
//! `require("@dreamIni")` register the table themselves.
//!
//! # Module shape
//!
//! - `version: string`: the crate version.
//! - `parseIni(text, options?) -> { entries: multimap, warnings: { warning } }`
//! - `parseCfg(text) -> multimap`
//! - `serializeCfg(multimap) -> string`
//! - `importMaps(cfg, ini, options?) -> { cfg: multimap, text: string, warnings: { warning }, events: { event } }`
//! - `importPaths(options) -> { cfg: multimap, text: string, warnings: { warning }, events: { event } }`
//!
//! Multimaps are tables where each key maps to an array of strings, for example
//! `{ encoding = { "win1252" }, content = { "Morrowind.esm" } }`. `parseIni` takes the INI's raw
//! bytes as a Luau string and decodes them with `options.encoding` (default `"win1252"`). Option
//! tables reject unknown fields, so a misspelled or `snake_case` option is an error instead of being
//! silently ignored.
//!
//! Path strings are UTF-8. Hard failures (missing files, unsupported encodings, malformed multimaps,
//! failed imports) raise Luau errors; recoverable problems are returned in `warnings`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mlua::{Error as LuaError, Lua, LuaString, Result as LuaResult, Table, Value};

use crate::{
    Game, ImportEvent, ImportOptions, ImportWarning, IniImporter, MultiMap, TextEncoding,
    parse_cfg_str, parse_ini_bytes_with_warnings, serialize_cfg,
};

/// Option fields shared by `importMaps` and `importPaths`.
const IMPORT_OPTIONS: &[&str] = &[
    "game",
    "gameFiles",
    "fonts",
    "archives",
    "verbose",
    "encoding",
    "dataDirs",
    "dataLocal",
    "resources",
    "userData",
    "cfgDir",
];

/// Creates the `dreamIni` Luau module table.
///
/// See the [module documentation](self) for the table's shape. The crate does not provide a
/// `cdylib` or `require` module; callers assign the returned table into their Lua environment.
///
/// # Errors
/// Returns a Lua error if module functions cannot be created.
pub fn create_module(lua: &Lua) -> LuaResult<Table> {
    let module = lua.create_table()?;
    module.set("version", env!("CARGO_PKG_VERSION"))?;
    module.set(
        "parseIni",
        lua.create_function(|lua, (text, options): (LuaString, Option<Table>)| {
            check_fields(options.as_ref(), "parseIni", &[&["encoding"]])?;
            let options = options_from_table(options.as_ref())?;
            let parsed = parse_ini_bytes_with_warnings(
                text.as_bytes().as_ref(),
                options.encoding.unwrap_or(TextEncoding::Win1252),
            );
            let result = lua.create_table()?;
            result.set("entries", multimap_to_table(lua, &parsed.entries)?)?;
            result.set("warnings", warnings_to_array(lua, &parsed.warnings)?)?;
            Ok(result)
        })?,
    )?;
    module.set(
        "parseCfg",
        lua.create_function(|lua, text: String| multimap_to_table(lua, &parse_cfg_str(&text)))?,
    )?;
    module.set(
        "serializeCfg",
        lua.create_function(|_, cfg: Table| Ok(serialize_cfg(&table_to_multimap(&cfg)?)))?,
    )?;
    module.set(
        "importMaps",
        lua.create_function(|lua, (cfg, ini, options): (Table, Table, Option<Table>)| {
            check_fields(
                options.as_ref(),
                "importMaps",
                &[IMPORT_OPTIONS, &["iniPath"]],
            )?;
            let ini_path = option_string(options.as_ref(), "iniPath")?
                .map_or_else(|| PathBuf::from("Morrowind.ini"), PathBuf::from);
            let options = options_from_table(options.as_ref())?;
            let mut cfg = table_to_multimap(&cfg)?;
            let ini = table_to_multimap(&ini)?;
            let report = IniImporter::new(options)
                .import_maps(&mut cfg, &ini, &ini_path)
                .map_err(LuaError::external)?;
            import_result_to_table(lua, &cfg, &report.warnings, &report.events)
        })?,
    )?;
    module.set(
        "importPaths",
        lua.create_function(|lua, options: Table| {
            check_fields(
                Some(&options),
                "importPaths",
                &[IMPORT_OPTIONS, &["ini", "cfg"]],
            )?;
            let ini_path = required_string(&options, "ini")?;
            let cfg_path = option_string(Some(&options), "cfg")?;
            let mut import_options = options_from_table(Some(&options))?;
            if !import_options.data_dirs.is_empty() {
                import_options.data_dir_base = cfg_path
                    .as_deref()
                    .map(Path::new)
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .or(import_options.data_dir_base);
            }
            let result = IniImporter::new(import_options)
                .import_optional_cfg_path(Path::new(&ini_path), cfg_path.as_deref().map(Path::new))
                .map_err(LuaError::external)?;
            import_result_to_table(lua, &result.cfg, &result.warnings, &result.events)
        })?,
    )?;
    Ok(module)
}

/// Registers the module from [`create_module`] as the `dreamIni` global.
///
/// It does not modify package loaders or enable `require("@dreamIni")`.
///
/// # Errors
/// Returns a Lua error if the module cannot be created or assigned.
pub fn register(lua: &Lua) -> LuaResult<()> {
    let module = create_module(lua)?;
    lua.globals().set("dreamIni", module)
}

/// Rejects option fields that aren't in any of the `allowed` lists.
fn check_fields(table: Option<&Table>, function: &str, allowed: &[&[&str]]) -> LuaResult<()> {
    let Some(table) = table else {
        return Ok(());
    };
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let name = match &key {
            Value::String(name) => name.to_string_lossy(),
            other => other.to_string()?,
        };
        if !allowed.iter().any(|fields| fields.contains(&name.as_str())) {
            return Err(LuaError::external(format!(
                "{function}: unknown option '{name}'"
            )));
        }
    }
    Ok(())
}

fn options_from_table(table: Option<&Table>) -> LuaResult<ImportOptions> {
    let mut options = ImportOptions::default();
    let Some(table) = table else {
        return Ok(options);
    };

    if let Some(game) = table.get::<Option<String>>("game")? {
        if game.eq_ignore_ascii_case("morrowind") {
            options.game = Game::Morrowind;
        } else {
            return Err(LuaError::external(format!("unsupported game: {game}")));
        }
    }
    if let Some(value) = table.get::<Option<bool>>("gameFiles")? {
        options.import_game_files = value;
    }
    if let Some(value) = table.get::<Option<bool>>("fonts")? {
        options.import_fonts = value;
    }
    if let Some(value) = table.get::<Option<bool>>("archives")? {
        options.import_archives = value;
    }
    if let Some(value) = table.get::<Option<bool>>("verbose")? {
        options.verbose = value;
    }
    if let Some(value) = table.get::<Option<String>>("encoding")? {
        options.encoding = Some(TextEncoding::parse(&value).map_err(LuaError::external)?);
    }
    if let Some(data_dirs) = table.get::<Option<Table>>("dataDirs")? {
        options.data_dirs = data_dirs
            .sequence_values::<String>()
            .map(|value| value.map(PathBuf::from))
            .collect::<LuaResult<Vec<_>>>()?;
    }
    if let Some(value) = table.get::<Option<String>>("dataLocal")? {
        options.data_local = Some(PathBuf::from(value));
    }
    if let Some(value) = table.get::<Option<String>>("resources")? {
        options.resources = Some(PathBuf::from(value));
    }
    if let Some(value) = table.get::<Option<String>>("userData")? {
        options.user_data = Some(PathBuf::from(value));
    }
    if let Some(value) = table.get::<Option<String>>("cfgDir")? {
        options.cfg_dir = Some(PathBuf::from(value));
    }
    if !options.data_dirs.is_empty() && options.data_dir_base.is_none() {
        options.data_dir_base.clone_from(&options.cfg_dir);
    }

    Ok(options)
}

fn option_string(table: Option<&Table>, key: &str) -> LuaResult<Option<String>> {
    table.map_or(Ok(None), |table| table.get(key))
}

fn required_string(table: &Table, key: &str) -> LuaResult<String> {
    table
        .get::<Option<String>>(key)?
        .ok_or_else(|| LuaError::external(format!("missing required option: {key}")))
}

fn import_result_to_table(
    lua: &Lua,
    cfg: &MultiMap,
    warnings: &[ImportWarning],
    events: &[ImportEvent],
) -> LuaResult<Table> {
    let result = lua.create_table()?;
    result.set("cfg", multimap_to_table(lua, cfg)?)?;
    result.set("text", serialize_cfg(cfg))?;
    result.set("warnings", warnings_to_array(lua, warnings)?)?;
    result.set("events", events_to_array(lua, events)?)?;
    Ok(result)
}

fn warnings_to_array(lua: &Lua, warnings: &[ImportWarning]) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for (index, warning) in warnings.iter().enumerate() {
        table.set(index + 1, warning_to_table(lua, warning)?)?;
    }
    Ok(table)
}

fn warning_to_table(lua: &Lua, warning: &ImportWarning) -> LuaResult<Table> {
    let table = lua.create_table()?;
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            table.set("kind", "ignoredEmptyValue")?;
            table.set("key", key.as_str())?;
        }
        ImportWarning::MalformedIniLine { line } => {
            table.set("kind", "malformedIniLine")?;
            table.set("line", line.as_str())?;
        }
        ImportWarning::MissingGameFile { file } => {
            table.set("kind", "missingGameFile")?;
            table.set("file", file.as_str())?;
        }
    }
    table.set("message", warning.to_string())?;
    Ok(table)
}

fn events_to_array(lua: &Lua, events: &[ImportEvent]) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for (index, event) in events.iter().enumerate() {
        table.set(index + 1, event_to_table(lua, event)?)?;
    }
    Ok(table)
}

fn event_to_table(lua: &Lua, event: &ImportEvent) -> LuaResult<Table> {
    let table = lua.create_table()?;
    let (kind, path) = match event {
        ImportEvent::ContentFileResolved { path, modified } => {
            table.set("modified", system_time_seconds(*modified))?;
            ("contentFileResolved", path)
        }
        ImportEvent::DataDirAddedForContent { path } => ("dataDirAddedForContent", path),
        ImportEvent::ArchiveResolved { path } => ("archiveResolved", path),
        ImportEvent::DataDirAddedForArchive { path } => ("dataDirAddedForArchive", path),
    };
    table.set("kind", kind)?;
    table.set("path", path.to_string_lossy().as_ref())?;
    Ok(table)
}

fn system_time_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn multimap_to_table(lua: &Lua, map: &MultiMap) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for (key, values) in map {
        table.set(key.as_str(), strings_to_array(lua, values)?)?;
    }
    Ok(table)
}

fn table_to_multimap(table: &Table) -> LuaResult<MultiMap> {
    let mut map = MultiMap::new();
    for pair in table.pairs::<String, Value>() {
        let (key, value) = pair?;
        let Value::Table(values) = value else {
            return Err(LuaError::external(format!(
                "expected array of strings for key '{key}'"
            )));
        };
        let values = values
            .sequence_values::<String>()
            .collect::<LuaResult<Vec<_>>>()?;
        map.insert(key, values);
    }
    Ok(map)
}

fn strings_to_array(lua: &Lua, values: &[String]) -> LuaResult<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.set(index + 1, value.as_str())?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn lua_parse_cfg_preserves_duplicate_keys() {
        let lua = Lua::new();
        register(&lua).unwrap();

        lua.load(
            r#"
            local cfg = dreamIni.parseCfg("key=one\nkey=two\n")
            assert(cfg.key[1] == "one")
            assert(cfg.key[2] == "two")
            "#,
        )
        .exec()
        .unwrap();
    }

    #[test]
    fn lua_serializes_cfg_tables() {
        let lua = Lua::new();
        register(&lua).unwrap();

        let text: String = lua
            .load(r#"return dreamIni.serializeCfg({ key = { "one", "two" } })"#)
            .eval()
            .unwrap();

        assert_eq!(text, "key=one\nkey=two\n");
    }

    #[test]
    fn lua_parse_ini_returns_warnings() {
        let lua = Lua::new();
        register(&lua).unwrap();

        lua.load(
            r#"
            local result = dreamIni.parseIni("[General]\nEmpty=\n", { encoding = "win1252" })
            assert(result.warnings[1].kind == "ignoredEmptyValue")
            assert(result.warnings[1].key == "General:Empty")
            assert(result.warnings[1].message == "ignored empty value for key 'General:Empty'.")
            "#,
        )
        .exec()
        .unwrap();
    }

    #[test]
    fn lua_import_maps_returns_cfg_text_and_report() {
        let lua = Lua::new();
        register(&lua).unwrap();

        lua.load(
            r#"
            local cfg = { encoding = { "win1252" } }
            local ini = { ["General:Disable Audio"] = { "1" } }
            local result = dreamIni.importMaps(cfg, ini, { archives = false })
            assert(result.cfg["no-sound"][1] == "1")
            assert(result.text:find("no%-sound=1\n") ~= nil)
            assert(#result.warnings == 0)
            assert(#result.events == 0)
            "#,
        )
        .exec()
        .unwrap();
    }

    #[test]
    fn lua_import_paths_uses_explicit_data_dirs() {
        let dir = unique_test_dir("import-paths");
        let cfg_dir = dir.join("config");
        let data_dir = cfg_dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        let cfg = cfg_dir.join("openmw.cfg");
        fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
        fs::write(&cfg, "encoding=win1252\n").unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let lua = Lua::new();
        register(&lua).unwrap();
        let module = lua.globals().get::<Table>("dreamIni").unwrap();
        let options = lua.create_table().unwrap();
        options.set("ini", ini.to_string_lossy().as_ref()).unwrap();
        options.set("cfg", cfg.to_string_lossy().as_ref()).unwrap();
        options
            .set("cfgDir", dir.join("wrong-base").to_string_lossy().as_ref())
            .unwrap();
        options.set("gameFiles", true).unwrap();
        options.set("archives", false).unwrap();
        let data_dirs = lua.create_table().unwrap();
        data_dirs.set(1, "Data Files").unwrap();
        options.set("dataDirs", data_dirs).unwrap();

        let result: Table = module
            .get::<mlua::Function>("importPaths")
            .unwrap()
            .call(options)
            .unwrap();
        let text: String = result.get("text").unwrap();
        assert!(text.contains("content=Base.esm\n"));
        assert!(text.contains("data=Data Files\n"));
        let events: Table = result.get("events").unwrap();
        let event: Table = events.get(1).unwrap();
        assert_eq!(
            event.get::<String>("kind").unwrap(),
            "dataDirAddedForContent"
        );
        assert_eq!(
            event.get::<String>("path").unwrap(),
            data_dir.to_string_lossy()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lua_import_maps_uses_cfg_dir_for_relative_data_paths() {
        let dir = unique_test_dir("import-maps-cfg-dir");
        let cfg_dir = dir.join("config");
        let data_dir = cfg_dir.join("Data Files");
        let local_dir = cfg_dir.join("Local Data");
        let resources_dir = cfg_dir.join("resources");
        let user_data_dir = cfg_dir.join("user-data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&local_dir).unwrap();
        fs::create_dir_all(&resources_dir).unwrap();
        fs::create_dir_all(&user_data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let lua = Lua::new();
        register(&lua).unwrap();
        let module = lua.globals().get::<Table>("dreamIni").unwrap();
        let cfg = lua.create_table().unwrap();
        let data_local_values = lua.create_table().unwrap();
        data_local_values.set(1, "Local Data").unwrap();
        cfg.set("data-local", data_local_values).unwrap();
        let resources_values = lua.create_table().unwrap();
        resources_values.set(1, "resources").unwrap();
        cfg.set("resources", resources_values).unwrap();
        let user_data_values = lua.create_table().unwrap();
        user_data_values.set(1, "user-data").unwrap();
        cfg.set("user-data", user_data_values).unwrap();
        let ini = lua.create_table().unwrap();
        let game_files = lua.create_table().unwrap();
        game_files.set(1, "Base.esm").unwrap();
        ini.set("Game Files:GameFile0", game_files).unwrap();
        let options = lua.create_table().unwrap();
        options.set("gameFiles", true).unwrap();
        options.set("archives", false).unwrap();
        let option_data_dirs = lua.create_table().unwrap();
        option_data_dirs.set(1, "Data Files").unwrap();
        options.set("dataDirs", option_data_dirs).unwrap();
        options
            .set("cfgDir", cfg_dir.to_string_lossy().as_ref())
            .unwrap();

        let result: Table = module
            .get::<mlua::Function>("importMaps")
            .unwrap()
            .call((cfg, ini, options))
            .unwrap();
        let text: String = result.get("text").unwrap();
        assert!(text.contains("content=Base.esm\n"));
        assert!(text.contains("data=Data Files\n"));
        assert!(text.contains("data-local=Local Data\n"));
        assert!(text.contains("resources=resources\n"));
        assert!(text.contains("user-data=user-data\n"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lua_rejects_unknown_and_snake_case_options() {
        let lua = Lua::new();
        register(&lua).unwrap();

        for (call, option) in [
            (
                r#"dreamIni.importMaps({}, {}, { archives = false, user_data = "x" })"#,
                "importMaps: unknown option 'user_data'",
            ),
            (
                r#"dreamIni.importPaths({ ini = "Morrowind.ini", game_files = true })"#,
                "importPaths: unknown option 'game_files'",
            ),
            (
                r#"dreamIni.parseIni("", { gameFiles = true })"#,
                "parseIni: unknown option 'gameFiles'",
            ),
        ] {
            let error = lua.load(call).exec().unwrap_err().to_string();
            assert!(error.contains(option), "{error}");
        }
    }

    #[test]
    fn lua_module_reports_version_and_camel_case_kinds() {
        let lua = Lua::new();
        register(&lua).unwrap();
        lua.globals()
            .set("expectedVersion", env!("CARGO_PKG_VERSION"))
            .unwrap();

        lua.load(
            r#"
            assert(dreamIni.version == expectedVersion)
            local result = dreamIni.parseIni("[Game Files\n")
            local warning: { kind: string, line: string?, message: string } = result.warnings[1]
            assert(warning.kind == "malformedIniLine")
            assert(warning.line == "[Game Files")
            "#,
        )
        .exec()
        .unwrap();
    }

    #[test]
    fn lua_rejects_scalar_multimap_values() {
        let lua = Lua::new();
        register(&lua).unwrap();

        let error = lua
            .load(r#"return dreamIni.serializeCfg({ key = "value" })"#)
            .eval::<String>()
            .unwrap_err()
            .to_string();

        assert!(error.contains("expected array of strings for key 'key'"));
    }

    fn tes3_bytes(masters: &[&str]) -> Vec<u8> {
        let mut record = Vec::new();
        subrecord(&mut record, *b"HEDR", &[0; 300]);
        for master in masters {
            let mut name = master.as_bytes().to_vec();
            name.push(0);
            subrecord(&mut record, *b"MAST", &name);
            subrecord(&mut record, *b"DATA", &0u64.to_le_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        bytes
    }

    fn subrecord(output: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
        output.extend_from_slice(&name);
        output.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        output.extend_from_slice(data);
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir();
        let temp_dir = temp_dir.canonicalize().unwrap_or(temp_dir);
        temp_dir.join(format!(
            "dream-ini-lua-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
