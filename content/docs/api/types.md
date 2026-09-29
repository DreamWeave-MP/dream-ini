+++
title = "Other types"
description = "TextEncoding and its parser, Game, and reading a TES3 plugin's name and masters with read_plugin_header."
weight = 50

[extra]
kind = "api"
+++

## TextEncoding

{{ api_signature(value="enum TextEncoding { Win1250, Win1251, Win1252 }") }}

The Windows code pages Morrowind's text uses, and OpenMW's `encoding=` values. `Debug`, `Clone`,
`Copy`, `PartialEq`, `Eq`. [Encoding](@/docs/importing.md#encoding) says which is for which
language.

{{ api_signature(value="fn TextEncoding::parse(value: &str) -> Result<TextEncoding, ImportError>") }}

Parses an OpenMW encoding name: `win1250`, `win1251` or `win1252`, or `windows-1250` and so on, in
any case. Anything else is `ImportError::UnsupportedEncoding`.

```rust
use dream_ini::TextEncoding;

fn main() {
    assert_eq!(TextEncoding::parse("Windows-1251").unwrap(), TextEncoding::Win1251);
    assert!(TextEncoding::parse("utf-8").is_err());
}
```

## Game

{{ api_signature(value="enum Game { Morrowind }") }}

The game an INI is from, in `ImportOptions::game`. There is one. `Debug`, `Clone`, `Copy`,
`PartialEq`, `Eq`.

## read_plugin_header

{{ api_signature(value="fn read_plugin_header(path: &Path, format: PluginFormat, encoding: TextEncoding) -> Result<PluginHeader, ImportError>") }}

Reads the header record of a plugin and returns its masters, the files it needs loaded first.
Only the header is read, however large the file. Master names are decoded with `encoding`.

An import does not call it: since 0.2.0 the load order is Morrowind.exe's, by file type and time,
not by masters. It is here for tools that want to check a load order themselves.

Fails with `ImportError::Io` when the file cannot be read, and `ImportError::InvalidPluginHeader`
when it does not start with a `TES3` record or the record runs past its own end.

```rust
use std::path::Path;

use dream_ini::{PluginFormat, TextEncoding, read_plugin_header};

fn main() -> Result<(), dream_ini::ImportError> {
    let header = read_plugin_header(
        Path::new("Data Files/Bloodmoon.esm"),
        PluginFormat::Tes3,
        TextEncoding::Win1252,
    )?;
    println!("{} needs {:?}", header.name, header.masters);
    Ok(())
}
```

{{ api_signature(value="enum PluginFormat { Tes3 }") }}

The plugin format: Morrowind's TES3. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

{{ api_signature(value="struct PluginHeader") }}

| Field | Is |
|---|---|
| `name: String` | The plugin's file name |
| `masters: Vec<String>` | Its masters, in the order the header lists them |

`Debug`, `Clone`, `PartialEq`, `Eq`.
