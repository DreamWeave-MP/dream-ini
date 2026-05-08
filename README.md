# dream-ini

`dream-ini` imports settings from `Morrowind.ini` into an `openmw.cfg`-style file. It is a standalone Rust importer compatible with OpenMW's Morrowind.ini import needs, with deliberate UX improvements over the original C++ tool.

## Build

```bash
cargo build --release
```

## Usage

```bash
dream-ini --ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]
```

`--ini` is required for imports. By default, the imported cfg text is written to stdout and diagnostics go to stderr, so shell redirection is safe. Use `--output` to write a separate cfg file or `--in-place` with `--cfg` to overwrite the base cfg with a resolved flattened export. If `--cfg` is provided, it is read first, imported keys are replaced, and unrelated settings are preserved as cfg entries, but comments, `config=`/`replace=` chain controls, and relative/token path spelling are not preserved in resolved output. If `--cfg` is omitted, import starts from an empty config.

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

## Options

- `-c, --cfg <FILE>`: optional openmw.cfg input/base path. It is only overwritten when `--in-place` is supplied.
- `-d, --data <DIR>`: explicit Data Files directory searched before cfg/default data paths.
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

## Behavior

- Existing cfg output is updated through `openmw-config`'s preservation-oriented serialization. Comments, unrelated entries, and relative/token path spelling are preserved unless a key is intentionally replaced by the import.
- Output generated without `--cfg` is new `openmw.cfg` text built from imported/authored values. It has no source comments or formatting to preserve.
- When no `--output` or `--in-place` mode is selected, cfg text is written to stdout. Diagnostics are written to stderr in stdout mode.
- Missing cfg files are treated as empty configs and are not created unless they are also the `--output` path or `--in-place` target.
- Omitting cfg starts from an empty config.
- Missing INI files fail with shell exit code `253`, matching the C++ importer's `return -3` behavior.
- Existing cfg entries are preserved unless replaced by imported keys such as `encoding`, `no-sound`, `fallback`, `fallback-archive`, or `content`, or by explicit singleton path options such as `--data-local`, `--resources`, and `--user-data`.
- Content-file and fallback-archive import searches existing `data-local` cfg paths first, then the explicit `--data` path, then existing `data` cfg paths, then `<Morrowind.ini parent>/Data Files` as a fallback. `data-local` always wins because it is OpenMW's highest-precedence data directory. Every `.esm`/`.esp` and `.bsa` entry imported from the INI must be found or the import fails. Any used explicit or fallback data directory is written as `data=...` if an equivalent `data`/`data-local` entry is not already present.
- Explicit singleton options (`--data-local`, `--resources`, and `--user-data`) are output-only and are applied after content/archive resolution. Use `--data` to add an importer search path.
- Directory-valued keys read from an existing cfg are interpreted by `openmw-config` for filesystem lookup. Their authored spelling is not rewritten for normal cfg output.
- Config, Lua, and event path values are UTF-8 text. Non-UTF-8 operating-system paths are outside the supported API contract and may be represented lossy when converted for cfg/Lua output.

## Deliberate Differences From OpenMW's C++ Importer

- Warnings are written to stderr instead of stdout.
- Game-file import requires filenames ending in `.esm` or `.esp`; the C++ importer accepts any suffix ending in `esm` or `esp`.
- Unreadable input files are reported as errors instead of silently importing from an empty stream.
- Game-file timestamp sorting uses Rust's full `SystemTime` precision instead of C++ `time_t` seconds.
- `--verbose` gates content-file timestamp messages. The C++ importer accepts `--verbose` but prints those messages unconditionally during game-file import.

## Lua API

Enable the optional Lua embedding API with the `lua` feature. It uses `mlua` with vendored LuaJIT 2.1 in Lua 5.2 compatibility mode.

```bash
cargo test --features lua
```

The crate does not build a Lua `require` module. Embedders create or register the API table explicitly:

```rust
let lua = mlua::Lua::new();
let module = dream_ini::lua::create_module(&lua)?;
lua.globals().set("dream_ini", module)?;
```

Lua usage:

```lua
local result = dream_ini.import_paths({
  ini = "Morrowind.ini",
  cfg = "openmw.cfg",
  game_files = true,
  archives = true,
  fonts = false,
  data_dirs = { "/games/Morrowind/Data Files" },
  cfg_dir = "/home/user/.config/openmw",
  user_data = "/home/user/.local/share/openmw",
  encoding = "win1252",
})

print(result.text)
for _, warning in ipairs(result.warnings) do
  print(warning.message)
end
for _, event in ipairs(result.events) do
  if event.kind == "content_file_resolved" then
    print(event.path, event.modified)
  elseif event.kind == "data_dir_added_for_content" then
    print(event.path)
  end
end
```

Available functions:

- `parse_ini(text, opts)`: parses a Morrowind INI byte string and returns `{ entries = multimap, warnings = { ... } }`.
- `parse_cfg(text)`: parses OpenMW cfg text and returns a multimap.
- `serialize_cfg(multimap)`: serializes a multimap to normalized cfg text.
- `import_maps(cfg, ini, opts)`: imports parsed multimap data and returns `{ cfg = multimap, text = string, warnings = { ... }, events = { ... } }`.
- `import_paths(opts)`: imports from `opts.ini` and optional `opts.cfg`, returning the same result shape as `import_maps`.

Import events are structured tables. Current event kinds are:

- `{ kind = "content_file_resolved", path = string, modified = unix_seconds }`
- `{ kind = "data_dir_added_for_content", path = string }`

Import warnings are structured tables with a formatted `message`. Current warning kinds are:

- `{ kind = "ignored_empty_value", key = string, message = string }`
- `{ kind = "malformed_ini_line", line = string, message = string }`

Multimaps are represented as `key -> array of strings` to preserve duplicate keys:

```lua
{
  encoding = { "win1252" },
  content = { "Morrowind.esm", "Tribunal.esm" },
}
```

## Rust API

Generate crate documentation with:

```bash
cargo doc --open
```

The library exposes the same multimap model used by the CLI and Lua API. Start with `IniImporter`, `ImportOptions`, `ImportEvent`, `ImportWarning`, `parse_cfg_str`, `parse_ini_bytes_with_warnings`, and `serialize_cfg`. Path values serialized into cfg text, Lua tables, or import events are UTF-8 strings.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -W clippy::pedantic -D warnings
cargo test
cargo bench
```

Lua feature checks:

```bash
cargo clippy --all-targets --features lua -- -W clippy::pedantic -D warnings
cargo test --features lua
cargo bench --no-run --features lua
```

The Criterion benchmark measures a large synthetic parse/import/serialize round trip. It does not include plugin header IO from `--game-files`. Use `cargo bench --no-run` to verify the benchmark builds without running measurements.

## License

`dream-ini` is licensed under GPL-3.0-or-later.
