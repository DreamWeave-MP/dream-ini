+++
title = "Writing cfg files"
description = "Update a user's openmw.cfg with its comments kept, write a new cfg, or write a flattened one with full paths: the functions the command line uses, and in which order."
weight = 30

[extra]
kind = "api"
+++

An import returns a [`MultiMap`](@/docs/api/parsing.md#multimap). These functions turn it into a
file, in the three ways [Writing the cfg](@/docs/writing.md) describes. The ones that keep a cfg
as its author wrote it work on an `OpenMWConfiguration` from
[openmw-config](https://docs.rs/openmw-config/2/openmw_config/struct.OpenMWConfiguration.html) 2,
which dream-ini depends on; name the type through the same major version.

Every `save_*` function writes atomically: a temporary file in the same folder, flushed, then
renamed over the target, keeping the old file's permission bits.

## Updating a cfg, comments kept

This is what `--in-place` does:

```rust
use std::path::Path;

use dream_ini::{
    ImportOptions, IniImporter, PreservedCfgUpdate, apply_preserved_cfg_update, load_cfg_document,
    save_preserved_cfg_document_to_path,
};

fn main() -> Result<(), dream_ini::ImportError> {
    let ini = Path::new("Morrowind/Morrowind.ini");
    let cfg = Path::new("openmw/openmw.cfg");
    let options = ImportOptions {
        import_game_files: true,
        ..ImportOptions::default()
    };
    let update = PreservedCfgUpdate {
        import_game_files: options.import_game_files,
        import_archives: options.import_archives,
        data_local: None,
        resources: None,
        user_data: None,
    };

    let result = IniImporter::new(options).import_optional_cfg_path(ini, Some(cfg))?;
    let mut document = load_cfg_document(cfg)?;
    apply_preserved_cfg_update(&mut document, &result.cfg, &update, &result.changed_keys)?;
    save_preserved_cfg_document_to_path(&document, cfg, cfg, &update, &result.changed_keys)
}
```

{{ api_signature(value="struct PreservedCfgUpdate") }}

What the import was asked to change, beside `changed_keys`. `Debug`, `Clone`.

| Field | Is |
|---|---|
| `import_game_files: bool` | Rewrite `content=`. The same as `ImportOptions::import_game_files` |
| `import_archives: bool` | Rewrite `fallback-archive=`. The same as `ImportOptions::import_archives` |
| `data_local`, `resources`, `user_data: Option<PathBuf>` | Replace those settings |

{{ api_signature(value="fn load_cfg_document(path: &Path) -> Result<OpenMWConfiguration, ImportError>") }}

Loads a cfg and its `config=` chain as openmw-config models it, every line kept. A file that does
not exist loads as an empty cfg.

{{ api_signature(value="fn apply_preserved_cfg_update(config: &mut OpenMWConfiguration, imported_cfg: &MultiMap, update: &PreservedCfgUpdate, changed_keys: &BTreeSet<String>) -> Result<(), ImportError>") }}

Sets the keys in `changed_keys` from `imported_cfg`: `encoding`, `no-sound`, `fallback`,
`content` and `fallback-archive` replaced, `data` added where not already present, and the
`update`'s settings replaced. Nothing else in `config` changes.

{{ api_signature(value="fn serialize_preserved_cfg_document(config: &OpenMWConfiguration, source_path: &Path, update: &PreservedCfgUpdate, changed_keys: &BTreeSet<String>) -> String") }}

The text of the file at `source_path` after the update: its own lines, comments included, with the
changed keys. Settings from other files in the chain are not copied in, except the changed keys
openmw-config attributes to the user's cfg. What a preview with `--cfg` prints.

{{ api_signature(value="fn save_preserved_cfg_document_to_path(config: &OpenMWConfiguration, source_path: &Path, output_path: &Path, update: &PreservedCfgUpdate, changed_keys: &BTreeSet<String>) -> Result<(), ImportError>") }}

Writes that text to `output_path`. If `output_path` is a symbolic link, the file it points to is
replaced. Relative paths in the text mean what they meant only if `output_path` is in the same
folder as `source_path`; otherwise, write a flattened cfg.

## A flattened cfg, full paths

{{ api_signature(value="fn save_resolved_configuration_to_path(config: &OpenMWConfiguration, output_path: &Path) -> Result<(), ImportError>") }}

{{ api_signature(value="fn serialize_resolved_configuration(config: &OpenMWConfiguration) -> String") }}

The whole chain applied into one cfg: every setting, every path in full, no `config=` or
`replace=` lines, keys sorted. What `--output` into another folder writes, after
`apply_preserved_cfg_update`.

{{ api_signature(value="fn save_resolved_cfg_to_path(cfg: &MultiMap, output_path: &Path) -> Result<(), ImportError>") }}

{{ api_signature(value="fn serialize_resolved_cfg(cfg: &MultiMap, user_config_dir: &Path) -> Result<String, ImportError>") }}

The same from a map. Relative paths in `cfg` are resolved against `user_config_dir`, or the output
file's folder when saving.

## A new cfg

{{ api_signature(value="fn save_cfg_output_to_path(cfg: &MultiMap, output_path: &Path) -> Result<(), ImportError>") }}

{{ api_signature(value="fn serialize_cfg_output(cfg: &MultiMap, user_config_dir: &Path) -> Result<String, ImportError>") }}

A cfg holding just `cfg`, paths spelled as they are, through openmw-config's writer, which ends it
with a comment naming its version. What an import without `--cfg` writes. `user_config_dir` is
the folder the cfg is for; an empty path means the current folder.

## Errors

The functions returning `Result` fail with `ImportError::Io` when a file cannot be read or
written, and `ImportError::OpenMwConfig` when openmw-config refuses a setting, such as an encoding
or a fallback it cannot represent.
