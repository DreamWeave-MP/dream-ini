# rome-ini

`rome-ini` imports settings from `Morrowind.ini` into an `openmw.cfg`-style file. It is a standalone Rust importer compatible with OpenMW's Morrowind.ini import needs, with deliberate UX improvements over the original C++ tool.

## Build

```bash
cargo build --release
```

## Usage

```bash
rome-ini [OPTIONS] <inifile> [configfile]
```

If a cfg path is provided, it is read first, imported keys are replaced, unrelated settings are preserved, and the result is written back to the cfg path unless `--output`, `--stdout`, or `--dry-run` is supplied. If `--output`, `--stdout`, or `--dry-run` is supplied without a cfg path, import starts from an empty config.

```bash
rome-ini Morrowind.ini openmw.cfg
rome-ini --ini Morrowind.ini --output imported.cfg
rome-ini --output imported.cfg Morrowind.ini openmw.cfg
rome-ini --game-files Morrowind.ini openmw.cfg
rome-ini --game-files --data-dir "/games/Morrowind/Data Files" --stdout > openmw.cfg
rome-ini --game-files --verbose Morrowind.ini openmw.cfg
rome-ini --fonts --encoding win1252 Morrowind.ini openmw.cfg
rome-ini --no-archives Morrowind.ini openmw.cfg
rome-ini --dry-run Morrowind.ini openmw.cfg
```

## Options

- `-i, --ini <FILE>`: Morrowind.ini input path.
- `-c, --cfg <FILE>`: optional openmw.cfg input/base path. Required only when `--output`, `--stdout`, and `--dry-run` are omitted.
- `-o, --output <FILE>`: output cfg path.
- `--data-dir <DIR>` / `--data <DIR>`: explicit Data Files directory for `--game-files`. Can be repeated and is searched before cfg/default data paths.
- `--dry-run`: parse, import, and print diagnostics without writing an output file.
- `--stdout` / `--print`: write the resulting cfg to stdout instead of a file. Diagnostics are written to stderr so stdout is redirect-safe.
- `-g, --game-files`: import `.esm` and `.esp` content files.
- `-f, --fonts`: import bitmap font fallback settings.
- `-A, --no-archives`: disable BSA archive import.
- `-e, --encoding <ENCODING>`: `win1250`, `win1251`, or `win1252`.
- `-v, --verbose`: print content-file timestamp messages during `--game-files` import.
- `--version`: print version information.
- `-h, --help`: print help.

## Behavior

- Output is normalized `key=value` data sorted by key. Comments and original formatting are not preserved.
- Missing cfg files are treated as empty configs and are not created unless they are also the output path.
- Omitting cfg is allowed when `--output`, `--stdout`, or `--dry-run` is provided; this starts from an empty config.
- Missing INI files fail with shell exit code `253`, matching the C++ importer's `return -3` behavior.
- Existing cfg settings are preserved unless replaced by imported keys such as `encoding`, `no-sound`, `fallback`, `fallback-archive`, or `content`.
- `--game-files` searches explicit `--data-dir` paths first, then existing `data` and `data-local` cfg paths, then `<Morrowind.ini parent>/Data Files` as a fallback. If content is resolved from an explicit `--data-dir` and an equivalent path is not already present, `rome-ini` writes it as a `data=...` entry. The default `Data Files` fallback is searched but is not written automatically.
- If `--game-files` sees game-file entries but resolves none of them, a summary warning suggests providing a cfg with `data=...` or using `--data-dir`.

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
let module = rome_ini::lua::create_module(&lua)?;
lua.globals().set("rome_ini", module)?;
```

Lua usage:

```lua
local result = rome_ini.import_paths({
  ini = "Morrowind.ini",
  cfg = "openmw.cfg",
  game_files = true,
  archives = true,
  fonts = false,
  data_dirs = { "/games/Morrowind/Data Files" },
  encoding = "win1252",
})

print(result.text)
for _, warning in ipairs(result.warnings) do
  print(warning)
end
```

Available functions:

- `parse_ini(text, opts)`: parses a Morrowind INI byte string and returns `{ entries = multimap, warnings = { ... } }`.
- `parse_cfg(text)`: parses OpenMW cfg text and returns a multimap.
- `serialize_cfg(multimap)`: serializes a multimap to normalized cfg text.
- `import_maps(cfg, ini, opts)`: imports parsed multimap data and returns `{ cfg = multimap, text = string, warnings = { ... }, messages = { ... } }`.
- `import_paths(opts)`: imports from `opts.ini` and optional `opts.cfg`, returning the same result shape as `import_maps`.

Multimaps are represented as `key -> array of strings` to preserve duplicate keys:

```lua
{
  encoding = { "win1252" },
  content = { "Morrowind.esm", "Tribunal.esm" },
}
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -W clippy::pedantic -D warnings
cargo test
cargo bench
```

The Criterion benchmark measures a large synthetic parse/import/serialize round trip. It does not include plugin header IO from `--game-files`. Use `cargo bench --no-run` to verify the benchmark builds without running measurements.

## License

`rome-ini` is licensed under GPL-3.0-or-later.
