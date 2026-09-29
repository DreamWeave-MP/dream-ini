+++
title = "Importing"
description = "IniImporter and its three import functions, every ImportOptions field, and what ImportResult and ImportReport return."
weight = 10

[extra]
kind = "api"
+++

An `IniImporter` holds the options; each call imports once and keeps nothing.
[What an import does](@/docs/importing.md) describes the import itself.

## IniImporter

{{ api_signature(value="struct IniImporter") }}

`Debug`, `Clone`.

{{ api_signature(value="fn new(options: ImportOptions) -> IniImporter") }}

{{ api_signature(value="fn import_optional_cfg_path(&self, ini_path: &Path, cfg_path: Option<&Path>) -> Result<ImportResult, ImportError>") }}

What the command line and the GUI call. Reads the INI at `ini_path`, decodes it with the
[encoding](@/docs/importing.md#encoding), and imports it into the cfg at `cfg_path`, or into an
empty cfg. The cfg is read as OpenMW would: its `config=` chain followed and its paths resolved in
full. A cfg that does not exist is empty; an INI that does not exist is `ImportError::Io`. Relative
`data=` values are relative to the cfg's folder. Nothing is written.

The result's `cfg` is that resolved cfg with the import applied. To write the user's cfg back with
its comments, pass the result to the [cfg writers](@/docs/api/writing.md).

{{ api_signature(value="fn import_paths(&self, ini_path: &Path, cfg_path: &Path) -> Result<ImportResult, ImportError>") }}

`import_optional_cfg_path` with a cfg.

{{ api_signature(value="fn import_maps(&self, cfg: &mut MultiMap, ini: &MultiMap, ini_path: &Path) -> Result<ImportReport, ImportError>") }}

Imports an INI that is already parsed into `cfg`, in place. `ini_path` is where the INI would be:
its folder's `Data Files` is searched last, and the file itself is not read. Relative paths in
`cfg` are relative to `options.cfg_dir`, or to the current folder. `cfg` gets the same `encoding`
as a path import. On error, `cfg` is left as it was.

```rust
use std::path::Path;

use dream_ini::{ImportOptions, IniImporter, parse_cfg_str, parse_ini_str};

fn main() -> Result<(), dream_ini::ImportError> {
    let mut cfg = parse_cfg_str("no-sound=0\n");
    let ini = parse_ini_str("[General]\nDisable Audio=1\n[Weather]\nSunrise Time=6\n");

    let options = ImportOptions {
        import_archives: false,
        ..ImportOptions::default()
    };
    let report = IniImporter::new(options).import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))?;

    assert_eq!(cfg["no-sound"], ["1"]);
    assert_eq!(cfg["fallback"], ["Weather_Sunrise_Time,6"]);
    assert_eq!(cfg["encoding"], ["win1252"]);
    assert!(report.changed_keys.contains("fallback"));
    Ok(())
}
```

## ImportOptions

{{ api_signature(value="struct ImportOptions") }}

`Debug`, `Clone`, `Default`. Every field is public; set the ones you need and take the rest from
`ImportOptions::default()`.

| Field | Default | Meaning |
|---|---|---|
| `game: Game` | `Game::Morrowind` | The only game there is |
| `import_game_files: bool` | `false` | Import `[Game Files]` as `content=`. `--game-files` |
| `import_fonts: bool` | `false` | Import the three `[Fonts]` fallback settings. `--fonts` |
| `import_archives: bool` | `true` | Import `[Archives]` as `fallback-archive=`. Off is `--no-archives` |
| `data_dirs: Vec<PathBuf>` | empty | Folders searched first, in order. `--data` |
| `data_dir_base: Option<PathBuf>` | `None` | What relative `data_dirs` are relative to. With `None`, the current folder, and they are written as full paths |
| `write_resolved_data_dirs: bool` | `false` | Write `data=` lines for `data_dirs` as full paths, even with a base |
| `data_local: Option<PathBuf>` | `None` | Set `data-local=`. `--data-local` |
| `resources: Option<PathBuf>` | `None` | Set `resources=`. `--resources` |
| `user_data: Option<PathBuf>` | `None` | Set `user-data=`. `--user-data` |
| `cfg_dir: Option<PathBuf>` | `None` | For `import_maps`: the folder the cfg would be in. The path imports use the cfg's own folder |
| `encoding: Option<TextEncoding>` | `None` | The encoding. With `None`, the cfg's, else `win1252` |
| `verbose: bool` | `false` | Also report `ContentFileResolved` and `ArchiveResolved` events. `--verbose` |

The command line sets `data_dir_base` to the output cfg's folder, or the input cfg's for a preview,
and `write_resolved_data_dirs` when the output is in another folder than the input cfg. See
[Relative --data folders](@/docs/writing.md#relative-data-folders).

## ImportResult

{{ api_signature(value="struct ImportResult") }}

What the path imports return. `Debug`, `Clone`, `PartialEq`, `Eq`.

| Field | Is |
|---|---|
| `cfg: MultiMap` | The cfg after the import |
| `warnings: Vec<ImportWarning>` | What was skipped, in order: the INI's own warnings first |
| `events: Vec<ImportEvent>` | What was found and added |
| `changed_keys: BTreeSet<String>` | The cfg keys the import set, such as `content` or `data` |

`changed_keys` is what the [preserving writer](@/docs/api/writing.md) uses to decide which keys to
rewrite. `encoding` is in it when you chose an encoding or the cfg had none.

## ImportReport

{{ api_signature(value="struct ImportReport") }}

What `import_maps` returns, which changes the cfg you pass instead of returning one: `warnings`,
`events` and `changed_keys`, as above. `Debug`, `Clone`, `PartialEq`, `Eq`.
