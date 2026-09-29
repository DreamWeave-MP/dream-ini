+++
title = "Rust API"
description = "Every public type and function in the dream-ini library: the importer, the INI and cfg parsers, the cfg writers, and the reports."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"
weight = 90

[extra]
kind = "api"
hide_child_cards = true
+++

The crate `dream_ini` is the importer the command line and the GUI run, as a library. Everything
is exported from the crate root.

```toml
[dependencies]
dream-ini = { version = "0.4", default-features = false }
```

`default-features = false` leaves out the GUI, which the library does not use.

```rust
use std::path::Path;

use dream_ini::{ImportOptions, IniImporter};

fn main() -> Result<(), dream_ini::ImportError> {
    let options = ImportOptions {
        import_game_files: true,
        ..ImportOptions::default()
    };
    let result = IniImporter::new(options)
        .import_optional_cfg_path(Path::new("Morrowind/Morrowind.ini"), Some(Path::new("openmw/openmw.cfg")))?;

    for file in &result.cfg["content"] {
        println!("{file}");
    }
    for warning in &result.warnings {
        eprintln!("Warning: {warning}");
    }
    Ok(())
}
```

| Page | Covers |
|---|---|
| [Importing](@/docs/api/importer.md) | `IniImporter`, `ImportOptions`, `ImportResult` and `ImportReport` |
| [Parsing](@/docs/api/parsing.md) | `MultiMap`, the INI and cfg parsers, `serialize_cfg` and `known_fallback_keys` |
| [Writing cfg files](@/docs/api/writing.md) | Updating a cfg with its comments kept, and writing new or flattened ones |
| [Reports and errors](@/docs/api/reports.md) | `ImportWarning`, `ImportEvent` and `ImportError` |
| [Other types](@/docs/api/types.md) | `TextEncoding`, `Game`, and reading plugin headers |
| [Luau extension](@/docs/luau/extension.md) | `luau::IniExtension` and its constants, behind the `luau` feature |

## Features

| Feature | Default | Adds to the library |
|---|---|---|
| `luau` | No | The `luau` module: [`IniExtension`](@/docs/luau/extension.md), an l3i extension providing `@dream/ini`. `lua` is the old name |
| `gui`, `controller-support`, `portmaster-gui` | `gui` | Nothing: they build the program's GUI |

## Paths

Paths are `Path` and `PathBuf` going in. Coming out, in cfg values and in the Luau module, they are
UTF-8 strings; a path that is not valid UTF-8 is outside what the crate supports.
