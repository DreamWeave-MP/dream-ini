+++
title = "Differences from OpenMW's importer"
description = "Where dream-ini and OpenMW's mwiniimporter part ways, and why: load order, missing files, output, and what each keeps of your cfg."
weight = 60

[extra]
kind = "reference"
+++

dream-ini reimplements `mwiniimporter`, the C++ importer that ships with OpenMW and that the OpenMW
launcher runs on first start. It imports the same keys, with the same list of fallback settings,
and returns the same exit code for a missing INI. Everything below differs on purpose. The C++
side was checked against OpenMW's source in September 2026.

## The load order

| | `mwiniimporter` | dream-ini |
|---|---|---|
| Order | Sorted by modification time, to the second; then each plugin's header is read and the list sorted so masters come before what needs them; then Tribunal is put before Bloodmoon | Morrowind.exe's order: master files, then plugins, each by modification time to the nanosecond. Headers are not read |
| A GameFile that is not found | `Warning: ... not found, ignoring`, and the list goes on | A warning, and the list stops there, as in Morrowind.exe |
| `GameFile` key case | Exact | Any |
| Which names count | Any name ending in the letters `esm` or `esp` | Names ending in `.esm` or `.esp`. A path instead of a file name fails the import |

The rules Morrowind.exe follows for its `GameFile` list are G7's research. Following them means an
install imports into the load order it already had in Morrowind, whatever its plugin headers say.

## Finding files

| | `mwiniimporter` | dream-ini |
|---|---|---|
| Searched for content | `data=`, then `data-local=`, then `Data Files` beside the INI | `--data`, then `data=`, then `Data Files` beside the INI. `data-local=` is a setting for OpenMW, not a place Morrowind's files live |
| Archives | Not looked for; any name is written | Each must be found, or the import fails. Only `.bsa` names |
| `data=` lines | Never added | The folders content and archives were found in are added |

## Your cfg

| | `mwiniimporter` | dream-ini |
|---|---|---|
| Files needed | `--ini` and `--cfg`, or it prints its help and exits with 0 | `--ini`. Without `--cfg`, it starts from an empty cfg |
| Paths as arguments | `mwiniimporter Morrowind.ini openmw.cfg` works | Refused: always `--ini` and `--cfg` |
| Where the result goes | Written over `--cfg`, unless `--output` | Standard output, unless `--output` or `--in-place` |
| What is written | Every key, sorted, one per line. Comments, `config=` chains and the file's order are lost | The cfg's own file with only the imported keys changed. See [Writing the cfg](@/docs/writing.md) |
| How | The file is opened and written | A new file, renamed over the old one when complete |
| An INI or cfg that cannot be read | Imported as empty | An error |

## Messages

| | `mwiniimporter` | dream-ini |
|---|---|---|
| Warnings | Standard output | Standard error |
| Content files and their times | Always printed during `--game-files` | Only with `--verbose` |

## What dream-ini adds

- `--data`, a folder to search first.
- `--data-local`, `--resources` and `--user-data`, to set those settings in the same run.
- `--in-place`, shell completions, a manual page and `install-launcher`.
- A [GUI](@/docs/gui.md) for desktops, controllers and [handhelds](@/docs/handhelds.md).
- The importer as a [Rust library](@/docs/api/_index.md) and a [Luau module](@/docs/luau/_index.md).
