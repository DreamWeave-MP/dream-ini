+++
title = "@dream/ini"
description = "version, parseIni, parseCfg, serializeCfg, importMaps and importPaths: their arguments, options, results and errors, with the Luau type definitions."
weight = 10

[extra]
kind = "api"
+++

```lua
local ini = require("@dream/ini")
```

A frozen table: `version` and five functions over plain tables. Names are camelCase throughout,
and so are option fields, result fields and `kind` values.

```lua
local result = ini.importPaths({
    ini = "Morrowind/Morrowind.ini",
    cfg = "openmw/openmw.cfg",
    gameFiles = true,
})

for _, file in result.cfg.content do
    print(file)
end
for _, warning in result.warnings do
    print(warning.message)
end
```

## Multimaps

A cfg, or a parsed INI, is a table from each key to an array of its values, in order. Keys that
repeat, such as `content` or `data`, keep every value:

```lua
local cfg = {
    encoding = { "win1252" },
    content = { "Morrowind.esm", "Tribunal.esm" },
}
```

An INI's keys are `Section:Key`, such as `General:Disable Audio`. Every value must be a string:
a number, even one that looks like a setting, is an error naming the key.

## version

{{ api_signature(value="version: string") }}

The dream-ini version, such as `"0.4.0"`.

## parseIni

{{ api_signature(value="parseIni(text: string | buffer, options: { encoding: string? }?) -> { entries: Multimap, warnings: { Warning } }") }}

Parses a `Morrowind.ini` from its raw bytes, as read from disk. `encoding` decodes them:
`"win1250"`, `"win1251"` or `"win1252"`, the default. The rules are the importer's own,
in [How the INI is read](@/docs/importing.md#how-the-ini-is-read).

```lua
local parsed = ini.parseIni("[General]\r\nDisable Audio=1\r\nWerewolf FOV=\r\n")
assert(parsed.entries["General:Disable Audio"][1] == "1")
assert(parsed.warnings[1].kind == "ignoredEmptyValue")
assert(parsed.warnings[1].key == "General:Werewolf FOV")
```

## parseCfg

{{ api_signature(value="parseCfg(text: string) -> Multimap") }}

Parses `openmw.cfg` text. Lines whose first character that is not a space is `#` are comments.
Key and value are trimmed of spaces; a line with no `=`, or nothing before it, is skipped. Nothing
is resolved: `config=` lines, tokens and relative paths stay as written.

## serializeCfg

{{ api_signature(value="serializeCfg(cfg: Multimap) -> string") }}

`key=value` lines, one per value, keys in byte order and values in their order, each ending in a
newline. No comments, and no formatting of the cfg it came from.

```lua
assert(ini.serializeCfg({ content = { "A.esm", "B.esp" }, encoding = { "win1252" } })
    == "content=A.esm\ncontent=B.esp\nencoding=win1252\n")
```

## importMaps

{{ api_signature(value="importMaps(cfg: Multimap, ini: Multimap, options: ImportOptions?) -> ImportResult") }}

Imports a parsed INI into a copy of `cfg`; the table passed in is not changed. It still searches
the disk for content and archives. Two options of its own:

| Option | Default | Meaning |
|---|---|---|
| `iniPath` | `"Morrowind.ini"` | Where the INI would be. Its folder's `Data Files` is searched last |
| `cfgDir` | none | The folder the cfg would be in. Relative `data=` values and `dataDirs` are relative to it; without it, to the current folder |

So `importMaps` on `parseCfg(cfgText)` and `parseIni(iniBytes).entries` imports what `importPaths`
would from the same files, given the same encoding and a `cfgDir` naming the cfg's folder, except
that `cfgText` is taken as written: its `config=` lines are not followed, and its paths keep their
spelling in the result.

## importPaths

{{ api_signature(value="importPaths(options: { ini: string, cfg: string?, ...ImportOptions }) -> ImportResult") }}

Reads `ini`, and `cfg` if given, and imports. A `cfg` that does not exist starts empty. The cfg
is read as OpenMW would: its `config=` chain followed and its paths resolved. Relative `dataDirs`
are relative to the cfg's folder, or to `cfgDir` without a cfg, or to the current folder. It
writes nothing; what to do with the result is the script's business.

## ImportOptions

Both import functions take these, all optional:

| Option | Default | Command line |
|---|---|---|
| `gameFiles` | `false` | `--game-files` |
| `archives` | `true` | the opposite of `--no-archives` |
| `fonts` | `false` | `--fonts` |
| `encoding` | the cfg's, else `"win1252"` | `--encoding` |
| `dataDirs` | `{}` | `--data`, but any number of folders, searched in order |
| `dataLocal`, `resources`, `userData` | none | `--data-local`, `--resources`, `--user-data` |
| `verbose` | `false` | `--verbose`: add `contentFileResolved` and `archiveResolved` events |
| `game` | `"morrowind"` | The only game there is |

[What an import does](@/docs/importing.md) explains each.

## ImportResult

| Field | Is |
|---|---|
| `cfg` | The imported cfg, a multimap |
| `text` | `serializeCfg(cfg)`: every key, sorted. Not the comment-preserving update the command line writes |
| `warnings` | Array of warnings: what was skipped |
| `events` | Array of events: what was found and added |

Warnings, each with a `message` in the command line's words:

| `kind` | Field | When |
|---|---|---|
| `ignoredEmptyValue` | `key` | An INI line with nothing after `=` |
| `malformedIniLine` | `line` | A section line with no `]` |
| `missingGameFile` | `file` | A GameFile not found; the load order stops there |

Events:

| `kind` | Fields | When |
|---|---|---|
| `dataDirAddedForContent` | `path` | A folder got a `data=` line for a content file found in it |
| `dataDirAddedForArchive` | `path` | The same, for an archive |
| `contentFileResolved` | `path`, `modified` | With `verbose`: a content file, and its modification time in Unix seconds |
| `archiveResolved` | `path` | With `verbose`: an archive |

## Errors

A failed call raises an error; nothing is returned.

| Error | Example |
|---|---|
| An unknown option | `ini.importMaps: unknown option 'user_data'; known options are archives, cfgDir, ...` |
| A wrong option | `ini.importPaths.encoding: unsupported encoding: utf-8` |
| A bad multimap | `ini.serializeCfg: expected a string at key 'no-sound' index 1, got number` |
| A failed import | `dream.ini: fallback archives not found: Tribunal.bsa; searched: ...` |

A missing INI in `importPaths` is a failed import, in the system's words:
`dream.ini: Morrowind.ini: No such file or directory (os error 2)`.

## Type definitions

What `plan.type_definitions()` generates for the module, for an editor's language server:

```lua
-- module @dream/ini (provided by dream.ini)
-- Morrowind.ini import into OpenMW cfg data, on multimap tables.
export type Module__dream_ini = {
    -- The dream-ini crate version.
    version: string,
    -- Parses a Morrowind.ini's raw bytes, decoded with `encoding` (default win1252).
    parseIni: (text: string | buffer, options: { encoding: string? }?) -> { entries: { [string]: { string } }, warnings: { { kind: string, message: string, key: string?, line: string?, file: string? } } },
    -- Parses openmw.cfg text into a multimap.
    parseCfg: (text: string) -> { [string]: { string } },
    -- Serializes a multimap as normalized cfg text.
    serializeCfg: (cfg: { [string]: { string } }) -> string,
    -- Imports a parsed INI into a copy of `cfg`; `iniPath` (default "Morrowind.ini") locates the fallback `Data Files` folder.
    importMaps: (cfg: { [string]: { string } }, ini: { [string]: { string } }, options: { game: string?, gameFiles: boolean?, fonts: boolean?, archives: boolean?, verbose: boolean?, encoding: string?, dataDirs: { string }?, dataLocal: string?, resources: string?, userData: string?, cfgDir: string?, iniPath: string? }?) -> { cfg: { [string]: { string } }, text: string, warnings: { { kind: string, message: string, key: string?, line: string?, file: string? } }, events: { { kind: string, path: string, modified: number? } } },
    -- Reads `ini` and the optional `cfg` from disk and imports; it writes nothing.
    importPaths: (options: { ini: string, cfg: string?, game: string?, gameFiles: boolean?, fonts: boolean?, archives: boolean?, verbose: boolean?, encoding: string?, dataDirs: { string }?, dataLocal: string?, resources: string?, userData: string?, cfgDir: string? }) -> { cfg: { [string]: { string } }, text: string, warnings: { { kind: string, message: string, key: string?, line: string?, file: string? } }, events: { { kind: string, path: string, modified: number? } } },
}
declare dreamIni: Module__dream_ini
```

The last line appears when the host exposes the
[`dreamIni` global](@/docs/luau-hosts.md#the-dreamini-global).
