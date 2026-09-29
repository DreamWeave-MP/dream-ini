// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `dream.ini` Luau extension: module `@dream/ini` for [l3i](https://crates.io/crates/l3i)
//! runtime plans.
//!
//! Enable the `luau` feature and compose [`IniExtension`] into a plan; the host decides whether
//! the module is also a global (`RuntimePolicy::compat_global("@dream/ini", "dreamIni")`). The
//! crate never creates a VM, registers no userdata, and takes no tags: the surface is tables.
//!
//! ```no_run
//! use l3i::extension::{RuntimePlan, RuntimePolicy};
//! use l3i::Runtime;
//!
//! # fn main() -> l3i::Result<()> {
//! let plan = RuntimePlan::builder()
//!     .policy(RuntimePolicy::new().compat_global(dream_ini::luau::MODULE, "dreamIni"))
//!     .extension(dream_ini::luau::IniExtension)
//!     .finalize()?;
//! let runtime = Runtime::from_plan(&plan)?;
//! runtime.exec("local ini = require('@dream/ini') assert(ini.version == dreamIni.version)")?;
//! # Ok(())
//! # }
//! ```
//!
//! # Module shape
//!
//! - `version: string`: the crate version (a compiler-folded constant when the module is a
//!   known global).
//! - `parseIni(text: string | buffer, options?) -> { entries: multimap, warnings: { warning } }`
//! - `parseCfg(text) -> multimap`
//! - `serializeCfg(multimap) -> string`
//! - `importMaps(cfg, ini, options?) -> { cfg: multimap, text: string, warnings: { warning }, events: { event } }`
//! - `importPaths(options) -> { cfg: multimap, text: string, warnings: { warning }, events: { event } }`
//!
//! Multimaps are tables where each key maps to an array of strings, for example
//! `{ encoding = { "win1252" }, content = { "Morrowind.esm" } }`; every value must be a string
//! (nothing is coerced). `parseIni` takes the INI's raw bytes as a Luau string or buffer and
//! decodes them with `options.encoding` (default `"win1252"`). Option tables are strict: an
//! unknown or misspelt key is an error naming the known keys, and every function's full Luau
//! type is part of the plan's definitions (`RuntimePlan::type_definitions`).
//!
//! Path strings are UTF-8. Hard failures (missing files, unsupported encodings, malformed
//! multimaps, failed imports) raise Luau errors prefixed `dream.ini:`; recoverable problems are
//! returned in `warnings`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use l3i::bind::{Call, StackResults};
use l3i::convert::BytesView;
use l3i::extension::{Extension, ExtensionDescriptor};
use l3i::options::{FromOptions, Options};
use l3i::source::CompileConstant;
use l3i::stack::{Frame, Scope, TableView, ValueView};
use l3i::value::Value;
use l3i::{Error, Result};

use crate::{
    Game, ImportError, ImportEvent, ImportOptions, ImportWarning, IniImporter, MultiMap,
    TextEncoding, parse_cfg_str, parse_ini_bytes_with_warnings, serialize_cfg,
};

/// The extension id.
pub const EXTENSION_ID: &str = "dream.ini";
/// The module path.
pub const MODULE: &str = "@dream/ini";

/// The `dream.ini` extension: composes into an `l3i` [`RuntimePlan`](l3i::extension::RuntimePlan).
#[derive(Clone, Copy, Debug, Default)]
pub struct IniExtension;

/// The extension, for hosts that prefer a function to a unit struct.
#[must_use]
pub fn extension() -> IniExtension {
    IniExtension
}

const MULTIMAP: &str = "{ [string]: { string } }";
const WARNING: &str =
    "{ kind: string, message: string, key: string?, line: string?, file: string? }";
const EVENT: &str = "{ kind: string, path: string, modified: number? }";
const IMPORT_OPTIONS: &str = "game: string?, gameFiles: boolean?, fonts: boolean?, archives: boolean?, \
     verbose: boolean?, encoding: string?, dataDirs: { string }?, dataLocal: string?, \
     resources: string?, userData: string?, cfgDir: string?";

impl Extension for IniExtension {
    fn id(&self) -> &'static str {
        EXTENSION_ID
    }

    fn describe(&self, d: &mut ExtensionDescriptor) -> Result<()> {
        let result = format!(
            "{{ cfg: {MULTIMAP}, text: string, warnings: {{ {WARNING} }}, events: {{ {EVENT} }} }}"
        );
        d.module(MODULE)
            .doc("Morrowind.ini import into OpenMW cfg data, on multimap tables.")
            .constant("version", CompileConstant::String(env!("CARGO_PKG_VERSION").to_owned()))
            .doc("The dream-ini crate version.")
            .function("parseIni", parse_ini)
            .signature(format!(
                "(text: string | buffer, options: {{ encoding: string? }}?) -> {{ entries: {MULTIMAP}, warnings: {{ {WARNING} }} }}"
            ))
            .doc("Parses a Morrowind.ini's raw bytes, decoded with `encoding` (default win1252).")
            .function("parseCfg", parse_cfg)
            .signature(format!("(text: string) -> {MULTIMAP}"))
            .doc("Parses openmw.cfg text into a multimap.")
            .function("serializeCfg", serialize)
            .signature(format!("(cfg: {MULTIMAP}) -> string"))
            .doc("Serializes a multimap as normalized cfg text.")
            .function("importMaps", import_maps)
            .signature(format!(
                "(cfg: {MULTIMAP}, ini: {MULTIMAP}, options: {{ {IMPORT_OPTIONS}, iniPath: string? }}?) -> {result}"
            ))
            .doc("Imports a parsed INI into a copy of `cfg`; `iniPath` (default \"Morrowind.ini\") locates the fallback `Data Files` folder.")
            .function("importPaths", import_paths)
            .signature(format!("(options: {{ ini: string, cfg: string?, {IMPORT_OPTIONS} }}) -> {result}"))
            .doc("Reads `ini` and the optional `cfg` from disk and imports; it writes nothing.");
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------------------------

/// `parseIni`'s options.
struct ParseIniOptions {
    encoding: Option<TextEncoding>,
}

impl FromOptions for ParseIniOptions {
    fn from_options(o: &mut Options<'_, '_>) -> Result<Self> {
        Ok(ParseIniOptions {
            encoding: read_encoding(o)?,
        })
    }
}

/// `importMaps`' options: the import options plus the INI's nominal location.
struct ImportMapsOptions {
    options: ImportOptions,
    ini_path: PathBuf,
}

impl Default for ImportMapsOptions {
    fn default() -> Self {
        ImportMapsOptions {
            options: ImportOptions::default(),
            ini_path: PathBuf::from("Morrowind.ini"),
        }
    }
}

impl FromOptions for ImportMapsOptions {
    fn from_options(o: &mut Options<'_, '_>) -> Result<Self> {
        let ini_path = o
            .optional::<String>("iniPath")?
            .map_or_else(|| PathBuf::from("Morrowind.ini"), PathBuf::from);
        Ok(ImportMapsOptions {
            options: read_import_options(o)?,
            ini_path,
        })
    }
}

/// `importPaths`' options: the import options plus the files.
struct ImportPathsOptions {
    options: ImportOptions,
    ini: PathBuf,
    cfg: Option<PathBuf>,
}

impl FromOptions for ImportPathsOptions {
    fn from_options(o: &mut Options<'_, '_>) -> Result<Self> {
        let ini = PathBuf::from(o.required::<String>("ini")?);
        let cfg = o.optional::<String>("cfg")?.map(PathBuf::from);
        let mut options = read_import_options(o)?;
        if !options.data_dirs.is_empty() {
            // Relative `dataDirs` resolve from the cfg's folder when there is a cfg.
            options.data_dir_base = cfg
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .or(options.data_dir_base);
        }
        Ok(ImportPathsOptions { options, ini, cfg })
    }
}

/// The fields `importMaps` and `importPaths` share.
fn read_import_options(o: &mut Options<'_, '_>) -> Result<ImportOptions> {
    let mut options = ImportOptions::default();
    if let Some(game) = o.optional_str("game", |game| {
        if game.eq_ignore_ascii_case("morrowind") {
            Ok(Game::Morrowind)
        } else {
            Err(Error::runtime(format!("unsupported game: {game}")))
        }
    })? {
        options.game = game;
    }
    options.import_game_files = o.or("gameFiles", options.import_game_files)?;
    options.import_fonts = o.or("fonts", options.import_fonts)?;
    options.import_archives = o.or("archives", options.import_archives)?;
    options.verbose = o.or("verbose", options.verbose)?;
    options.encoding = read_encoding(o)?;
    options.data_dirs = read_paths(o, "dataDirs")?;
    options.data_local = o.optional::<String>("dataLocal")?.map(PathBuf::from);
    options.resources = o.optional::<String>("resources")?.map(PathBuf::from);
    options.user_data = o.optional::<String>("userData")?.map(PathBuf::from);
    options.cfg_dir = o.optional::<String>("cfgDir")?.map(PathBuf::from);
    if !options.data_dirs.is_empty() && options.data_dir_base.is_none() {
        options.data_dir_base.clone_from(&options.cfg_dir);
    }
    Ok(options)
}

/// The optional `encoding` label, parsed in place: the option reader names the field on error.
fn read_encoding(o: &mut Options<'_, '_>) -> Result<Option<TextEncoding>> {
    o.optional_str("encoding", |label| {
        TextEncoding::parse(label).map_err(|error| Error::runtime(error.to_string()))
    })
}

/// An optional array of path strings.
fn read_paths(o: &mut Options<'_, '_>, key: &str) -> Result<Vec<PathBuf>> {
    let Some(list) = o.optional::<Value>(key)? else {
        return Ok(Vec::new());
    };
    let context = format!("{}.{key}", o.context());
    o.frame().with_frame(|frame| {
        let view = list.push_to(frame)?;
        let table = view.as_table().map_err(|_| {
            Error::runtime(format!(
                "{context}: expected an array of strings, got {}",
                view.type_of().name()
            ))
        })?;
        let mut paths = Vec::with_capacity(table.raw_len());
        for_each_array_value(frame, &table, |index, value| {
            let text = value.read::<&str>().map_err(|_| {
                Error::runtime(format!(
                    "{context}[{index}]: expected a string, got {}",
                    value.type_of().name()
                ))
            })?;
            paths.push(PathBuf::from(text));
            Ok(())
        })?;
        Ok(paths)
    })
}

// ---------------------------------------------------------------------------------------------
// Module functions
// ---------------------------------------------------------------------------------------------

fn parse_ini(
    call: &Call<'_>,
    text: BytesView<'_>,
    options: Option<ValueView<'_>>,
) -> Result<StackResults> {
    let encoding = match options {
        Some(view) => ParseIniOptions::read(call, view, "ini.parseIni")?.encoding,
        None => None,
    };
    // SAFETY: the bytes are consumed by the decoder before anything below touches the Lua API,
    // and nothing else runs on this thread meanwhile, so a buffer argument cannot be written
    // while the slice lives; a string argument is immutable anyway.
    let bytes = unsafe { text.bytes_unchecked() };
    let parsed = parse_ini_bytes_with_warnings(bytes, encoding.unwrap_or(TextEncoding::Win1252));
    let mut frame = call.frame();
    {
        let result = frame.push_table(0, 2)?;
        push_multimap(&frame, &parsed.entries)?;
        result.raw_set(&frame, "entries")?;
        push_warnings(&frame, &parsed.warnings)?;
        result.raw_set(&frame, "warnings")?;
    }
    frame.release();
    Ok(StackResults)
}

fn parse_cfg(call: &Call<'_>, text: &str) -> Result<StackResults> {
    let map = parse_cfg_str(text);
    let mut frame = call.frame();
    push_multimap(&frame, &map)?;
    frame.release();
    Ok(StackResults)
}

fn serialize(call: &Call<'_>, cfg: ValueView<'_>) -> Result<String> {
    Ok(serialize_cfg(&multimap_from_view(
        call,
        cfg,
        "ini.serializeCfg",
    )?))
}

fn import_maps(
    call: &Call<'_>,
    cfg: ValueView<'_>,
    ini: ValueView<'_>,
    options: Option<ValueView<'_>>,
) -> Result<StackResults> {
    let ImportMapsOptions { options, ini_path } = match options {
        Some(view) => ImportMapsOptions::read(call, view, "ini.importMaps")?,
        None => ImportMapsOptions::default(),
    };
    let mut cfg = multimap_from_view(call, cfg, "ini.importMaps")?;
    let ini = multimap_from_view(call, ini, "ini.importMaps")?;
    let report = IniImporter::new(options)
        .import_maps(&mut cfg, &ini, &ini_path)
        .map_err(|error| import_error(&error))?;
    push_import_result(call, &cfg, &report.warnings, &report.events)
}

fn import_paths(call: &Call<'_>, options: ValueView<'_>) -> Result<StackResults> {
    let ImportPathsOptions { options, ini, cfg } =
        ImportPathsOptions::read(call, options, "ini.importPaths")?;
    let result = IniImporter::new(options)
        .import_optional_cfg_path(&ini, cfg.as_deref())
        .map_err(|error| import_error(&error))?;
    push_import_result(call, &result.cfg, &result.warnings, &result.events)
}

fn import_error(error: &ImportError) -> Error {
    Error::runtime(format!("dream.ini: {error}"))
}

/// A 1-based array index as Luau's raw integer key.
fn index_key(index: usize) -> Result<i64> {
    i64::try_from(index).map_err(|_| Error::logic("Table index exceeds the supported range"))
}

/// Array values read per stack reservation: one nested frame and one `lua_checkstack` per
/// batch instead of a frame per element, bounded so a long array never exhausts Luau's stack.
const ARRAY_BATCH: usize = 64;

/// Visits `table[1]..table[#table]` in order; each value's view lives for its visit only.
fn for_each_array_value(
    frame: &Frame<'_>,
    table: &TableView<'_>,
    mut visit: impl FnMut(usize, ValueView<'_>) -> Result<()>,
) -> Result<()> {
    let count = table.raw_len();
    let mut first = 1;
    while first <= count {
        let batch = ARRAY_BATCH.min(count - first + 1);
        frame.with_frame(|inner| {
            inner.check(batch)?;
            for index in first..first + batch {
                visit(index, table.raw_get_index(inner, index_key(index)?)?)?;
            }
            Ok(())
        })?;
        first += batch;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Tables in and out
// ---------------------------------------------------------------------------------------------

/// Leaves `{ cfg, text, warnings, events }` on the call's stack as the single result.
fn push_import_result(
    call: &Call<'_>,
    cfg: &MultiMap,
    warnings: &[ImportWarning],
    events: &[ImportEvent],
) -> Result<StackResults> {
    let text = serialize_cfg(cfg);
    let mut frame = call.frame();
    {
        let result = frame.push_table(0, 4)?;
        push_multimap(&frame, cfg)?;
        result.raw_set(&frame, "cfg")?;
        frame.push_string(&text);
        result.raw_set(&frame, "text")?;
        push_warnings(&frame, warnings)?;
        result.raw_set(&frame, "warnings")?;
        push_events(&frame, events)?;
        result.raw_set(&frame, "events")?;
    }
    frame.release();
    Ok(StackResults)
}

/// Pushes `map` as `{ [key] = { values... } }`, sized exactly, one string push per value.
fn push_multimap(frame: &Frame<'_>, map: &MultiMap) -> Result<()> {
    let table = frame.push_table(0, map.len())?;
    for (key, values) in map {
        let array = frame.push_table(values.len(), 0)?;
        for (index, value) in values.iter().enumerate() {
            frame.push_string(value);
            array.raw_set_index(frame, index_key(index + 1)?)?;
        }
        table.raw_set(frame, key)?;
    }
    Ok(())
}

fn push_warnings(frame: &Frame<'_>, warnings: &[ImportWarning]) -> Result<()> {
    let array = frame.push_table(warnings.len(), 0)?;
    for (index, warning) in warnings.iter().enumerate() {
        let entry = frame.push_table(0, 3)?;
        let (kind, field, value) = match warning {
            ImportWarning::IgnoredEmptyValue { key } => ("ignoredEmptyValue", "key", key),
            ImportWarning::MalformedIniLine { line } => ("malformedIniLine", "line", line),
            ImportWarning::MissingGameFile { file } => ("missingGameFile", "file", file),
        };
        entry.raw_set_value(frame, "kind", kind)?;
        entry.raw_set_value(frame, field, value.as_str())?;
        entry.raw_set_value(frame, "message", &warning.to_string())?;
        array.raw_set_index(frame, index_key(index + 1)?)?;
    }
    Ok(())
}

fn push_events(frame: &Frame<'_>, events: &[ImportEvent]) -> Result<()> {
    let array = frame.push_table(events.len(), 0)?;
    for (index, event) in events.iter().enumerate() {
        let entry = frame.push_table(0, 3)?;
        let (kind, path) = match event {
            ImportEvent::ContentFileResolved { path, modified } => {
                entry.raw_set_value(frame, "modified", &system_time_seconds(*modified))?;
                ("contentFileResolved", path)
            }
            ImportEvent::DataDirAddedForContent { path } => ("dataDirAddedForContent", path),
            ImportEvent::ArchiveResolved { path } => ("archiveResolved", path),
            ImportEvent::DataDirAddedForArchive { path } => ("dataDirAddedForArchive", path),
        };
        entry.raw_set_value(frame, "kind", kind)?;
        entry.raw_set_value(frame, "path", &*path.to_string_lossy())?;
        array.raw_set_index(frame, index_key(index + 1)?)?;
    }
    Ok(())
}

/// Unix seconds as a Lua number (exact for every realistic timestamp).
fn system_time_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Reads `{ [string] = { string... } }` into a [`MultiMap`], naming the offending key on error.
fn multimap_from_view(scope: &impl Scope, view: ValueView<'_>, context: &str) -> Result<MultiMap> {
    let Ok(table) = view.as_table() else {
        return Err(Error::runtime(format!(
            "{context}: expected a multimap table, got {}",
            view.type_of().name()
        )));
    };
    let mut map = MultiMap::new();
    scope.with_frame(|frame| {
        table.for_each(frame, |step, key, value| {
            let key = key.read::<&str>().map_err(|_| {
                Error::runtime(format!(
                    "{context}: multimap keys must be strings, got {}",
                    key.type_of().name()
                ))
            })?;
            let values = value.as_table().map_err(|_| {
                Error::runtime(format!(
                    "{context}: expected array of strings for key '{key}', got {}",
                    value.type_of().name()
                ))
            })?;
            let mut strings = Vec::with_capacity(values.raw_len());
            for_each_array_value(step, &values, |index, value| {
                let text = value.read::<&str>().map_err(|_| {
                    Error::runtime(format!(
                        "{context}: expected a string at key '{key}' index {index}, got {}",
                        value.type_of().name()
                    ))
                })?;
                strings.push(text.to_owned());
                Ok(())
            })?;
            map.insert(key.to_owned(), strings);
            Ok(())
        })
    })?;
    Ok(map)
}
