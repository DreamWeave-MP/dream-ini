+++
title = "Parsing"
description = "MultiMap, the Morrowind.ini and openmw.cfg parsers, serialize_cfg, and the list of fallback keys an import knows."
weight = 20

[extra]
kind = "api"
+++

## MultiMap

{{ api_signature(value="type MultiMap = BTreeMap<String, Vec<String>>") }}

A cfg or a parsed INI: each key and its values, in order. Keys that repeat in a cfg, such as
`data`, `content` and `fallback`, keep every value. An INI's keys are `Section:Key`, such as
`General:Disable Audio`.

## Morrowind.ini

{{ api_signature(value="fn parse_ini_bytes_with_warnings(bytes: &[u8], encoding: TextEncoding) -> ParsedIni") }}

Decodes the bytes of a `Morrowind.ini` with `encoding` and parses them. A UTF-8 or UTF-16 byte
order mark at the start overrides `encoding`. This is what an import runs on the file it reads.

{{ api_signature(value="fn parse_ini_str_with_warnings(text: &str) -> ParsedIni") }}

Parses text that is already decoded.

{{ api_signature(value="fn parse_ini_bytes(bytes: &[u8], encoding: TextEncoding) -> MultiMap") }}

{{ api_signature(value="fn parse_ini_str(text: &str) -> MultiMap") }}

The same, without the warnings.

The rules, in [How the INI is read](@/docs/importing.md#how-the-ini-is-read): lines end at `\n`
and lose one `\r`; `[Section]` needs a name and a `]`; `;` starts a comment anywhere; the key is
everything before the first `=`, untrimmed, and the value everything after it. Empty values and
section lines with no `]` are skipped with a warning.

```rust
use dream_ini::{ImportWarning, parse_ini_str_with_warnings};

fn main() {
    let parsed = parse_ini_str_with_warnings("[Game Files]\r\nGameFile0=Morrowind.esm ; the base game\r\nGameFile1=\r\n");
    assert_eq!(parsed.entries["Game Files:GameFile0"], ["Morrowind.esm "]);
    assert_eq!(
        parsed.warnings,
        [ImportWarning::IgnoredEmptyValue { key: "Game Files:GameFile1".to_owned() }]
    );
}
```

The importer trims `GameFile` and `Archive` values itself, so the space before the comment does
not matter to an import.

## ParsedIni

{{ api_signature(value="struct ParsedIni") }}

| Field | Is |
|---|---|
| `entries: MultiMap` | Every `Section:Key` and its values |
| `warnings: Vec<ImportWarning>` | `IgnoredEmptyValue` and `MalformedIniLine`, in file order |

`Debug`, `Clone`, `PartialEq`, `Eq`.

## openmw.cfg

{{ api_signature(value="fn parse_cfg_str(text: &str) -> MultiMap") }}

Parses cfg text as it is. A line whose first character that is not a space is `#` is a comment;
key and value are trimmed; a line with no `=`, or nothing before it, is skipped. `config=` lines
are not followed, and tokens and relative paths are not resolved: for that, the importer reads a
cfg through [openmw-config](https://crates.io/crates/openmw-config).

{{ api_signature(value="fn serialize_cfg(cfg: &MultiMap) -> String") }}

`key=value` lines, one per value, keys in byte order and values in their order, each ending in
`\n`. The text contains nothing that was not in the map.

```rust
use dream_ini::{parse_cfg_str, serialize_cfg};

fn main() {
    let cfg = parse_cfg_str("# Written by hand\ncontent = Morrowind.esm\ndata=\"/games/Morrowind/Data Files\"\n");
    assert_eq!(serialize_cfg(&cfg), "content=Morrowind.esm\ndata=\"/games/Morrowind/Data Files\"\n");
}
```

## known_fallback_keys

{{ api_signature(value="fn known_fallback_keys() -> &'static [&'static str]") }}

The 573 `Section:Key` names an import turns into `fallback=` lines, the same list as OpenMW's
importer. The three `Fonts:Font N` keys are among them; the importer leaves them out
unless `import_fonts` is set.
