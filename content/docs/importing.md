+++
title = "What an import does"
description = "Every Morrowind.ini key dream-ini reads and the openmw.cfg line it becomes, where it finds content and archives, the load order it writes, and what it replaces."
weight = 20

[extra]
kind = "reference"
+++

An import reads `Morrowind.ini`, finds the files it names, and changes a handful of keys in the
cfg. The command line, the GUI, the Rust crate and the Luau module all run this same importer.

## What it reads

| From `Morrowind.ini` | Becomes | When |
|---|---|---|
| 570 known settings, from `[Weather]` to `[Moons]` | `fallback=Section_Key,value` | Always |
| `[Fonts]` `Font 0`, `Font 1`, `Font 2` | `fallback=Fonts_Font_0,value` | With `--fonts` |
| `[General]` `Disable Audio` | `no-sound=value` | Always, when the INI has it |
| `[Archives]` `Archive 0`, `Archive 1`, ... | `fallback-archive=`, after `Morrowind.bsa` | Unless `--no-archives` |
| `[Game Files]` `GameFile0`, `GameFile1`, ... | `content=`, in load order | With `--game-files` |
| | `encoding=` | Always |

Every other line in the INI is ignored.

## Fallback settings

OpenMW reads the engine values Morrowind.exe took from its INI as `fallback=` lines. dream-ini
imports the 573 keys it knows, the same list as OpenMW's importer, and nothing else. The key
becomes `Section_Key`, with spaces and the colon turned into underscores, and the value is copied
as it is:

```ini
; Morrowind.ini
[Weather Clear]
Cloud Texture=Tx_Sky_Clear.dds
```

```ini
# openmw.cfg
fallback=Weather_Clear_Cloud_Texture,Tx_Sky_Clear.dds
```

| Sections | Keys |
|---|---:|
| `Weather`, and `Weather Clear`, `Cloudy`, `Foggy`, `Overcast`, `Rain`, `Thunderstorm`, `Ashstorm`, `Blight`, `Snow`, `Blizzard` | 342 |
| `FontColor` | 45 |
| `Question 1` to `Question 10`, the character generation questions | 50 |
| `Water` and `PixelWater` | 35 |
| `Moons` | 23 |
| `Level Up` | 20 |
| `Blood` | 19 |
| `LightAttenuation` | 11 |
| `Map` | 10 |
| `Inventory` | 9 |
| `Movies` | 5 |
| `Fonts`, only with `--fonts` | 3 |
| `General`, `Werewolf FOV` | 1 |

A key the INI repeats gives one `fallback=` line per value, in order. Fallback keys must be spelled
as Morrowind writes them, in the same case. `known_fallback_keys()` in the
[Rust API](@/docs/api/parsing.md#known-fallback-keys) returns the whole list.

The three `[Fonts]` keys name Morrowind's bitmap fonts. OpenMW uses its own fonts unless the cfg
names others, so they are left out unless you ask for them.

## How the INI is read

- The whole file is decoded with the [encoding](#encoding) first.
- Lines end at a line feed and lose one trailing carriage return, so Windows and Unix line endings
  both work.
- `[Name]` starts a section. A line starting with `[` and no `]` after a name is skipped with a
  `malformedIniLine` warning.
- `;` starts a comment anywhere on a line.
- `Key=value`: everything before the first `=` is the key, everything after it the value, spaces
  included. A key with no value is skipped with an `ignoredEmptyValue` warning. A line with no
  `=` is ignored.

## Encoding

Morrowind stores text in a Windows code page, and so does OpenMW, which reads it from `encoding=`.
dream-ini uses one encoding for both jobs: it decodes the INI with it, and writes it to the cfg.

| Encoding | For |
|---|---|
| `win1250` | Central and Eastern European: Polish, Czech, Hungarian |
| `win1251` | Cyrillic: Russian |
| `win1252` | Western European: English, French, German, Spanish. The default |

The encoding is the one you pass (`--encoding`, or **Encoding** in the GUI), else the `encoding=`
already in the cfg, else `win1252`. `windows-1251` is accepted for `win1251`, and so on, in any
case.

## Where it looks for files

Archives and content files are searched for, in order, in:

1. The folder passed with `--data`.
2. Each `data=` folder in the cfg, in the cfg's order. A relative one is relative to the cfg's own
   folder.
3. `Data Files` in the folder that holds `Morrowind.ini`.

The first folder that has the file wins. A folder that does not exist is skipped, but still named
in the error if nothing is found. Names are compared by the file system, so on Linux,
`Tribunal.esm` and `tribunal.esm` are different files.

`data-local`, `resources` and `user-data` are never searched, even when you set them. They are
settings OpenMW reads when it runs, not places Morrowind's files live.

### The data= lines it adds

When a file is found in a folder the cfg does not list as `data=` yet, that folder is added: the
`--data` folder as you typed it, `Data Files` as its full path. A folder the cfg already lists,
in any spelling that leads to the same place, is not added twice.

New `data=` lines go after the ones already in the cfg. OpenMW gives later data folders priority,
so if your cfg lists mod folders and gets a new `Data Files` line, move that line above them.

## Archives

`Morrowind.bsa` comes first, always, because Morrowind.exe loads it whether the INI lists it or
not. Then `Archive 0`, `Archive 1` and on, in number order, up to the first missing number: after
`Archive 0` and `Archive 2`, `Archive 2` is not read. Entries that do not end in `.bsa` are
skipped.

Every archive must be found. If one is not, the import fails, names the archives and the folders
searched, and writes nothing. `--no-archives` leaves `fallback-archive=` alone.

## Content files and load order

With `--game-files`, the `.esm` and `.esp` files in `GameFile0`, `GameFile1` and on become the
cfg's `content=` lines. The rules are Morrowind.exe's:

- The list ends at the first missing number, as with archives. `GameFile` keys match in any case,
  and `GameFile07` is not `GameFile7`.
- A file that is not found stops the list, with a `missingGameFile` warning. The files before it
  are imported; the ones after it are not. This is not an error.
- Entries that do not end in `.esm` or `.esp` are skipped. An entry that is a path rather than a
  file name fails the import.

The INI's numbering does not set the load order. Morrowind.exe loads master files first, then
plugins, each oldest first by modification time, and dream-ini writes them in that order. Two
files with the same time load in reverse alphabetical order, whatever their case. Plugin headers
are not read.

## What it replaces and what it keeps

| Key | After an import |
|---|---|
| `content=` | With `--game-files`, all replaced by the INI's load order |
| `fallback-archive=` | Unless `--no-archives`, all replaced by `Morrowind.bsa` and the INI's archives |
| `fallback=` | All replaced by the INI's, when it has any |
| `no-sound=` | Replaced, when the INI has `Disable Audio` |
| `encoding=` | Always set, to the encoding above |
| `data=` | Added to, never removed |
| `data-local=`, `resources=`, `user-data=` | Replaced, when you pass them |
| Everything else | Kept as it was |

{% callout(kind="warning", title="The load order is replaced, not merged") %}
`--game-files` replaces every `content=` line in the cfg you give it, including OpenMW-only content
such as `.omwscripts` and `.omwaddon` files listed there. The same goes for `fallback=` lines that
did not come from an INI. Import into your cfg first and add those afterwards, or leave out
`--game-files` and keep your load order.
{% end %}

How the cfg is written, and what happens to its comments, is in
[Writing the cfg](@/docs/writing.md).
