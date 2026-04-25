# rome-ini

`rome-ini` imports settings from `Morrowind.ini` into an `openmw.cfg`-style file. It is a standalone Rust implementation of the OpenMW `mwiniimporter` behavior with a reusable library core and a thin CLI.

## Build

```bash
cargo build --release
```

## Usage

```bash
rome-ini [OPTIONS] <inifile> <configfile>
```

The existing cfg file is read first, imported keys are replaced, unrelated settings are preserved, and the result is written back to the cfg path unless `--output` is supplied.

```bash
rome-ini Morrowind.ini openmw.cfg
rome-ini --output imported.cfg Morrowind.ini openmw.cfg
rome-ini --game-files Morrowind.ini openmw.cfg
rome-ini --game-files --verbose Morrowind.ini openmw.cfg
rome-ini --fonts --encoding win1252 Morrowind.ini openmw.cfg
rome-ini --no-archives Morrowind.ini openmw.cfg
```

## Options

- `-i, --ini <FILE>`: Morrowind.ini input path.
- `-c, --cfg <FILE>`: openmw.cfg input path.
- `-o, --output <FILE>`: output cfg path.
- `-g, --game-files`: import `.esm` and `.esp` content files.
- `-f, --fonts`: import bitmap font fallback settings.
- `-A, --no-archives`: disable BSA archive import.
- `-e, --encoding <ENCODING>`: `win1250`, `win1251`, or `win1252`.
- `-v, --verbose`: print content-file timestamp messages during `--game-files` import.
- `-h, --help`: print help.

## Behavior

- Output is normalized `key=value` data sorted by key. Comments and original formatting are not preserved.
- Missing cfg files are treated as empty configs and are not created unless they are also the output path.
- Missing INI files fail with shell exit code `253`, matching the C++ importer's `return -3` behavior.
- Existing cfg settings are preserved unless replaced by imported keys such as `encoding`, `no-sound`, `fallback`, `fallback-archive`, or `content`.
- `--game-files` searches existing `data` and `data-local` cfg paths, then `<Morrowind.ini parent>/Data Files`.

## Intentional Differences From OpenMW's C++ Importer

- Warnings are written to stderr instead of stdout.
- Game-file import requires filenames ending in `.esm` or `.esp`; the C++ importer accepts any suffix ending in `esm` or `esp`.
- Unreadable input files are reported as errors instead of silently importing from an empty stream.
- Game-file timestamp sorting uses Rust's full `SystemTime` precision instead of C++ `time_t` seconds.
- `--verbose` gates content-file timestamp messages. The C++ importer accepts `--verbose` but prints those messages unconditionally during game-file import.

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
