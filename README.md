# dream-ini

`dream-ini` imports settings from `Morrowind.ini` into an `openmw.cfg`-style file. It is a standalone Rust importer compatible with OpenMW's Morrowind.ini import needs, with deliberate UX improvements over the original C++ tool.

## Build

Desktop builds use the default feature set, which includes the native GUI and the CLI:

```bash
cargo build --release
```

Release platform support is intentionally split by build shape:

| Platform | Release build shape | Availability |
| --- | --- | --- |
| Linux x64, Windows x64, macOS x64/ARM64 | default features | Desktop GUI and CLI |
| PortMaster ARM64 | `--no-default-features --features portmaster-gui` | Framebuffer GUI and CLI |
| Android ARM64 | `--no-default-features` | CLI/importer only |

Other targets may build the library or CLI, but GUI, launcher installation, and controller support are only promised for the platform rows above.

## Usage

```bash
dream-ini --ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]
```

`--ini` is required for imports. By default, the imported cfg text is written to stdout and diagnostics go to stderr, so shell redirection is safe. Use `--output` to write a separate cfg file or `--in-place` with `--cfg` to update the base cfg. If `--cfg` is provided, it is read first and intentionally imported keys are replaced. In-place and same-directory output preserve unrelated comments, entries, chain controls, and relative/token path spelling through `openmw-config`'s preservation-oriented serializer. Relocated `--cfg` + `--output` writes a resolved export so relative paths do not silently change meaning. If `--cfg` is omitted, import starts from an empty config.

Import paths are flag-based. Positional `Morrowind.ini` or `openmw.cfg` arguments are intentionally unsupported; use `--ini`, optional `--cfg`, and optional `--output` or `--in-place`.

```bash
dream-ini --ini Morrowind.ini > openmw.cfg
dream-ini --ini Morrowind.ini --cfg openmw.cfg > preview.cfg
dream-ini --ini Morrowind.ini --cfg openmw.cfg --in-place
dream-ini --ini Morrowind.ini --output imported.cfg
dream-ini --ini Morrowind.ini --cfg openmw.cfg --output imported.cfg
dream-ini --ini Morrowind.ini --cfg openmw.cfg --game-files --in-place
dream-ini --ini Morrowind.ini --game-files --data "/games/Morrowind/Data Files" > openmw.cfg
dream-ini --ini Morrowind.ini --cfg openmw.cfg --game-files --verbose --in-place
dream-ini --ini Morrowind.ini --cfg openmw.cfg --fonts --encoding win1252 --in-place
dream-ini --ini Morrowind.ini --cfg openmw.cfg -l local-data -r /usr/share/openmw/resources -u user-data --in-place
dream-ini --ini Morrowind.ini --cfg openmw.cfg --no-archives --in-place
dream-ini -C bash > dream-ini.bash
dream-ini -M > dream-ini.1
```

## GUI

When built with the default `gui` feature, running `dream-ini` with no arguments opens the desktop graphical importer in a native window. The GUI uses the same explicit import model as the CLI: choose `Morrowind.ini`, optionally choose an existing `openmw.cfg`, pick import options, then either preview, save as a separate cfg, or update the existing cfg.

PortMaster builds use the separate `portmaster-gui` feature with default features disabled. That build draws the same importer UI directly to the Linux framebuffer instead of using desktop windowing. Clipboard support is unavailable in the PortMaster shell. Diagnostic logs, when enabled by the launcher or environment, are written by the launcher/script beside the executable or to the configured log path.

Controller navigation is available in the desktop GUI on Linux, Windows, and macOS, and in the PortMaster framebuffer GUI:

- D-pad or left stick: move between fields, picker entries, and result tabs.
- A / South: activate the selected field, toggle checkboxes, open directories, or choose files.
- B / East or Select: cancel the picker; on the main form it exits the GUI.
- X / West: clear the selected path field.
- Start: import from the main form, or choose the current/expected picker path.
- Left / Right: adjust options; in the picker, Left enters the parent directory and Right enters the selected directory or chooses the selected file.
- LB / left shoulder: toggle hidden directories in the picker. This is useful for OpenMW paths under `~/.config/openmw` or `~/.local/share/openmw`.
- RB / right shoulder: page down through the generated cfg preview.
- Right stick: scroll the generated cfg preview vertically and horizontally.

On desktop Linux and PortMaster, controller support reads `/dev/input` event devices directly. If your desktop session does not grant read permission for those devices, the GUI will still run but controller navigation will not appear. Windows and macOS use `gilrs` for controller input. PortMaster uses the Linux controller backend with a handheld-specific menu/trigger remap for Anbernic-style evdev button codes; the on-screen help shows the behavior used by that build.

Path reminder: `Data Files directory` is the Morrowind content/archive search path used during import. Classic Morrowind usually points at one `Data Files` folder; OpenMW can later use many `data=` directories. `data-local`, `resources`, and `user-data` are OpenMW cfg singleton outputs; they are not used as importer search paths.

## Options

- `-c, --cfg <FILE>`: optional openmw.cfg input/base path. It is only overwritten when `--in-place` is supplied.
- `-d, --data <DIR>`: explicit Data Files directory searched before cfg/default data paths. Relative paths are resolved from the output cfg directory, from `--cfg` for stdout preview, or from the current directory and written absolute when stdout has no cfg context.
- `-l, --data-local <DIR>`: set the singleton `data-local` cfg key, replacing any existing value. The value is written as supplied and is not used as an importer search path.
- `-e, --encoding <ENCODING>`: character encoding for imported content-file names; `win1250`, `win1251`, or `win1252`.
- `-f, --fonts`: import bitmap font fallback settings.
- `-g, --game-files`: import `.esm` and `.esp` content files.
- `-h, --help`: print help.
- `-i, --ini <FILE>`: Morrowind.ini input path.
- `-n, --no-archives`: disable BSA archive import.
- `-r, --resources <DIR>`: set the singleton `resources` cfg key, replacing any existing value. The value is written as supplied.
- `-u, --user-data <DIR>`: set the singleton `user-data` cfg key, replacing any existing value. The value is written as supplied; this is OpenMW's saves/screenshots/navmesh-cache location, not a mod data directory.
- `-v, --verbose`: print content-file timestamp messages during `--game-files` import.
- `-C, --generate-completion <SHELL>`: write a completion script for `bash`, `zsh`, `fish`, `powershell`, or `elvish` to stdout.
- `-w, --in-place`: update `--cfg` in place. Requires `--cfg` and conflicts with `--output`.
- `-M, --generate-manpage`: write a roff manpage to stdout.
- `-o, --output <FILE>`: output cfg path.
- `-V, --version`: print version information.

### Commands

- `install-launcher`: install a desktop launcher and icon for the current user. This is intended for Linux and Windows desktop builds. On Linux this writes a `.desktop` file and hicolor PNG icon to `$XDG_DATA_HOME` when it is absolute, otherwise to `~/.local/share`. On Windows this writes a Start Menu shortcut and `.ico` icon under `%APPDATA%`. Non-Linux/non-Windows targets, including macOS and Android, return an unsupported-platform error. PortMaster is a Linux build, but launcher installation is not part of the supported PortMaster release flow.

## Behavior

- Existing cfg output is updated through `openmw-config`'s preservation-oriented serialization when the output remains in the same cfg directory. Comments, unrelated entries, and relative/token path spelling are preserved unless a key is intentionally replaced by the import.
- Existing cfg output written to a different directory uses resolved flattened serialization so relative paths keep their resolved meaning instead of becoming relative to the new output directory.
- Output generated without `--cfg` is new `openmw.cfg` text built from imported/authored values. It has no source comments or formatting to preserve.
- When no `--output` or `--in-place` mode is selected, cfg text is written to stdout. Diagnostics are written to stderr in stdout mode.
- Missing cfg files are treated as empty configs and are not created unless they are also the `--output` path or `--in-place` target.
- Omitting cfg starts from an empty config.
- Missing INI files fail with shell exit code `253`, matching the C++ importer's `return -3` behavior.
- Existing cfg entries are preserved unless replaced by imported keys such as `encoding`, `no-sound`, `fallback`, `fallback-archive`, or `content`, or by explicit singleton path options such as `--data-local`, `--resources`, and `--user-data`.
- Content-file and fallback-archive import searches explicit `--data` paths first, then existing `data` cfg paths, then `<Morrowind.ini parent>/Data Files` as a fallback. `data-local` is OpenMW's highest-precedence runtime data directory, but this importer treats it as an output-only singleton rather than a Morrowind.ini content/archive source. Every `.esm`/`.esp` and `.bsa` entry imported from the INI must be found or the import fails. Any used explicit or fallback data directory is written as `data=...` if an equivalent `data` entry is not already present.
- Relative `--data` with `--output`, `--cfg`, or `--in-place` is interpreted relative to that cfg context and written as supplied. Relative `--data` with stdout and no `--cfg` is interpreted relative to the current directory and written as an absolute path.
- Explicit singleton options (`--data-local`, `--resources`, and `--user-data`) are output-only and are applied after content/archive resolution. Use `--data` to add an importer search path.
- Directory-valued keys read from an existing cfg are interpreted by `openmw-config` for filesystem lookup. Their authored spelling is not rewritten for normal cfg output.
- Config, Luau, and event path values are UTF-8 text. Non-UTF-8 operating-system paths are outside the supported API contract and may be represented lossy when converted for cfg/Lua output.

## Deliberate Differences From OpenMW's C++ Importer

- Warnings are written to stderr instead of stdout.
- Game-file import requires filenames ending in `.esm` or `.esp`; the C++ importer accepts any suffix ending in `esm` or `esp`.
- Unreadable input files are reported as errors instead of silently importing from an empty stream.
- Game-file timestamp sorting uses Rust's full `SystemTime` precision instead of C++ `time_t` seconds.
- `--verbose` gates content-file timestamp messages. The C++ importer accepts `--verbose` but prints those messages unconditionally during game-file import.

## Luau API

The optional `luau` feature (`lua` is the former spelling) builds the `dream.ini` extension for
[l3i](https://github.com/DreamWeave-MP/dream-binder), the DreamWeave Luau binder. The crate never
creates a VM: the host composes `dream_ini::luau::IniExtension` into an l3i `RuntimePlan`, and
every runtime made from that plan has the module `@dream/ini`. Whether it is also a global is the
host's policy:

```rust
use l3i::extension::{RuntimePlan, RuntimePolicy};
use l3i::Runtime;

let plan = RuntimePlan::builder()
    .policy(RuntimePolicy::new().compat_global(dream_ini::luau::MODULE, "dreamIni"))
    .extension(dream_ini::luau::IniExtension)
    .finalize()?;
let runtime = Runtime::from_plan(&plan)?;
runtime.exec("local ini = require('@dream/ini') print(ini.version)")?;
```

The extension declares no userdata and takes no tags: the surface is tables. Its full Luau type
is part of the plan (`plan.type_definitions()` renders the `.d.luau`, and `plan.check_definitions()`
proves it with Luau's own frontend), so strict scripts type check against `require("@dream/ini")`
without hand-kept declarations.

Luau usage:

```luau
local ini = require("@dream/ini")

local result = ini.importPaths({
  ini = "Morrowind.ini",
  cfg = "openmw.cfg",
  gameFiles = true,
  archives = true,
  fonts = false,
  dataDirs = { "/games/Morrowind/Data Files" },
  userData = "/home/user/.local/share/openmw",
  encoding = "win1252",
})

print(result.text)
for _, warning in result.warnings do
  print(warning.message)
end
for _, event in result.events do
  if event.kind == "contentFileResolved" then
    print(event.path, event.modified)
  elseif event.kind == "archiveResolved" then
    print(event.path)
  end
end
```

Functions, option fields, result fields, and `kind` values are camelCase. Option tables are
strict: an unknown or misspelled key is an error that names the known keys.

- `version`: the crate version.
- `parseIni(text, options?)`: parses a Morrowind INI's raw bytes (a string or a `buffer`, decoded
  with `options.encoding`, default `"win1252"`) and returns `{ entries = multimap, warnings = { ... } }`.
- `parseCfg(text)`: parses OpenMW cfg text and returns a multimap.
- `serializeCfg(multimap)`: serializes a multimap to normalized cfg text.
- `importMaps(cfg, ini, options?)`: imports parsed multimap data and returns `{ cfg = multimap, text = string, warnings = { ... }, events = { ... } }`.
- `importPaths(options)`: imports from `options.ini` and optional `options.cfg`, returning the same shape as `importMaps`.

Import options: `game`, `gameFiles`, `archives`, `fonts`, `verbose`, `encoding`, `dataDirs`,
`dataLocal`, `resources`, `userData`, and `cfgDir`; `importMaps` adds `iniPath` (default
`"Morrowind.ini"`, which locates the fallback `Data Files` folder). For `importPaths`, relative
`dataDirs` resolve from the `cfg` file's folder when `cfg` is given; `cfgDir` supplies that context
otherwise, and is mainly for `importMaps`.

Hard failures (missing required options, unknown options, unsupported encodings, malformed
multimaps, missing files, failed imports) raise Luau errors; import errors are prefixed
`dream.ini:`. Recoverable problems come back in `warnings`, each with a formatted `message`:

- `{ kind = "ignoredEmptyValue", key = string, message = string }`
- `{ kind = "malformedIniLine", line = string, message = string }`
- `{ kind = "missingGameFile", file = string, message = string }`

Import events:

- `{ kind = "contentFileResolved", path = string, modified = unixSeconds }`
- `{ kind = "dataDirAddedForContent", path = string }`
- `{ kind = "archiveResolved", path = string }`
- `{ kind = "dataDirAddedForArchive", path = string }`

Multimaps map each key to an array of strings, preserving duplicate keys. Every value must be a
string; nothing is coerced:

```luau
{
  encoding = { "win1252" },
  content = { "Morrowind.esm", "Tribunal.esm" },
}
```

### Changes from the mlua bindings (0.3)

- The module is reached through `require("@dream/ini")`; `dreamIni` exists only when the host
  policy exposes it. `dream_ini::lua::{create_module, register}` are gone.
- Multimap values must be strings (mlua coerced numbers).
- Option errors carry the function's context and list the known keys
  (`ini.importMaps: unknown option 'user_data'; known options are ...`); import errors read
  `dream.ini: <message>` with no `runtime error:` prefix.
- `parseIni` also accepts a Luau `buffer`.

### Measured

`cargo bench --features luau --bench luau_boundary` runs the frozen scripts of
`benches/luau_boundary.rs` on the fixture `benches/round_trip.rs` uses (4226 INI entries in
1543 sections, 514 cfg entries, 129 archives, 256 stale `data=` dirs). Medians, release with thin
LTO, one machine: "mlua" is the 0.3.1 binding at the commit that added the bench, "l3i" the port
with the importer untouched, "0.4.0" this version. The 0.4.0 column was taken while other
builds shared the machine (load average 12 to 89); only `importPaths` had a quiet window, and
the binding did not change between the l3i and 0.4.0 columns, so the l3i column is the
per-call cost to expect.

| script       | mlua 0.3.1 | l3i (port only) | 0.4.0                      |
|--------------|-----------:|----------------:|---------------------------:|
| importPaths  | 205.9 ms   | 208.8 ms        | 4.87 ms                    |
| importMaps   | 2.277 ms   | 2.126 ms        | 7.2 ms (5.4 to 8.9, loaded) |
| parseIni     | 2.816 ms   | 2.452 ms        | 4.9 ms (4.3 to 5.7, loaded) |
| parseCfg     | 131.9 µs   | 141.7 µs        | 234 µs (loaded)            |
| serializeCfg | 54.4 µs    | 55.0 µs         | 71 µs (loaded)             |

The importer is where the time went (`cargo bench --bench round_trip`, quiet machine):
`large_ini_round_trip` 198.5 ms to 4.97 ms (search paths that are not directories are never
probed, and the cfg's `data=` dirs are canonicalised once per import instead of once per resolved
file), `parse_ini_str` 1.465 ms to 1.029 ms, `parse_cfg_str` 68.3 µs to 58.1 µs,
`serialize_cfg` 3.20 µs unchanged within noise.

## Rust API

Generate crate documentation with:

```bash
cargo doc --open
```

The library exposes the same multimap model used by the CLI and Luau API. Start with `IniImporter`, `ImportOptions`, `ImportEvent`, `ImportWarning`, `parse_cfg_str`, `parse_ini_bytes_with_warnings`, and `serialize_cfg`. Path values serialized into cfg text, Luau tables, or import events are UTF-8 strings.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -W clippy::pedantic -D warnings
cargo test
cargo bench
```

Luau feature checks (l3i's toolchain policy applies: clang, lld, and cross-language thin LTO,
set by `.cargo/config.toml`):

```bash
cargo clippy --all-targets --features luau -- -W clippy::pedantic -D warnings
cargo test --features luau
cargo bench --no-run --features luau
```

The Criterion benchmark measures a large synthetic parse/import/serialize round trip. It does not include plugin header IO from `--game-files`. Use `cargo bench --no-run` to verify the benchmark builds without running measurements.

## License

`dream-ini` is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
