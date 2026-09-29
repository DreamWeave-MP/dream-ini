+++
title = "Command line"
description = "Every dream-ini option and command, the rules between them, and the exit codes."
weight = 50

[extra]
kind = "reference"
+++

```text
dream-ini --ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]
dream-ini --generate-completion <SHELL>
dream-ini --generate-manpage
dream-ini install-launcher
```

With no arguments at all, the desktop and PortMaster builds open the [GUI](@/docs/gui.md) instead.
Paths are always given with options: `dream-ini Morrowind.ini openmw.cfg` is an error, not an
import.

## Files

| Option | Meaning |
|---|---|
| `-i`, `--ini <FILE>` | The `Morrowind.ini` to import. Required for an import. |
| `-c`, `--cfg <FILE>` | The `openmw.cfg` to start from. Read first; written only with `--in-place`. Without it, the import starts from an empty cfg. |
| `-o`, `--output <FILE>` | Write the result to `FILE`. |
| `-w`, `--in-place` | Write the result back to the `--cfg` file. Needs `--cfg`; not with `--output`. |

With neither `--output` nor `--in-place`, the result goes to standard output and nothing is
written. [Writing the cfg](@/docs/writing.md) covers what each mode keeps of the cfg.

## What to import

| Option | Meaning |
|---|---|
| `-g`, `--game-files` | Import the `.esm` and `.esp` files in `[Game Files]` as the load order. Off by default. |
| `-f`, `--fonts` | Import the three `[Fonts]` settings, Morrowind's bitmap fonts. Off by default. |
| `-n`, `--no-archives` | Do not import `[Archives]`; leave `fallback-archive=` as it is. |
| `-e`, `--encoding <ENCODING>` | `win1250`, `win1251` or `win1252`. Decodes the INI and is written as `encoding=`. Default: the cfg's, else `win1252`. |
| `-d`, `--data <DIR>` | A folder to search for content and archives before any other. One folder. |

[What an import does](@/docs/importing.md) has what each key becomes and the search order.

## Settings to set

Each replaces the setting in the output cfg and is written as given. None of them is searched for
files.

| Option | Sets |
|---|---|
| `-l`, `--data-local <DIR>` | `data-local=`, OpenMW's highest-priority data folder |
| `-r`, `--resources <DIR>` | `resources=`, the engine's own resources. Point it only at OpenMW's |
| `-u`, `--user-data <DIR>` | `user-data=`, where OpenMW keeps saves, screenshots and the navmesh cache |

## Output and help

| Option | Meaning |
|---|---|
| `-v`, `--verbose` | Also report each content file found, with its modification time, and each archive. |
| `-C`, `--generate-completion <SHELL>` | Print a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish`. Takes no other option. |
| `-M`, `--generate-manpage` | Print the manual page, in roff. Takes no other option. |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print `dream-ini` and the version. |

```sh
dream-ini -C bash > ~/.local/share/bash-completion/completions/dream-ini
dream-ini -M > ~/.local/share/man/man1/dream-ini.1
```

## install-launcher

```sh
dream-ini install-launcher
```

Adds Dream INI to your application menu, for the current user, pointing at the `dream-ini` you ran.
Move the program afterwards and run it again.

| System | Writes |
|---|---|
| Linux | `applications/io.github.DreamWeave-MP.dream-ini.desktop` and `icons/hicolor/512x512/apps/io.github.DreamWeave-MP.dream-ini.png`, under `$XDG_DATA_HOME` when it is an absolute path, else `~/.local/share` |
| Windows | `Dream INI.lnk` in `%APPDATA%\Microsoft\Windows\Start Menu\Programs`, and its icon in `%APPDATA%\Dream INI\logo.ico` |
| macOS, Android | Nothing: an error says the platform is not supported |

It prints the two paths it wrote.

## Messages

In a preview, standard output is the cfg and nothing else. Progress messages go to standard
error, as `load ini file: ...` and one line per event; warnings go to standard error as
`Warning: ...`. When writing a file, progress messages go to standard output instead, ending with
`write to: ...`. Errors are `ERROR: ...` on standard error.

| Message | Meaning |
|---|---|
| `adding data directory used to resolve content files: DIR` | `DIR` gets a `data=` line |
| `adding data directory used to resolve fallback archives: DIR` | The same, for an archive |
| `content file: PATH timestamp = (SECONDS)` | With `--verbose`: a content file and its modification time |
| `archive: PATH` | With `--verbose`: an archive |
| `Warning: GameFile entry not found: FILE. Later GameFile entries were not imported.` | The load order stops at `FILE` |
| `Warning: ignored empty value for key 'KEY'.` | An INI line with nothing after `=` |
| `Warning: ini file wrongly formatted (LINE). Line ignored.` | A section line with no `]` |

## Exit codes

| Code | When |
|---|---|
| `0` | Success, warnings or not |
| `1` | The import failed: an archive not found, an unsupported encoding, a file that cannot be read or written. Nothing was written |
| `2` | The command line is wrong: an unknown option, a missing value, `--in-place` without `--cfg`, `--output` with `--in-place` |
| `253` | The `--ini` file does not exist. OpenMW's importer returns the same |

## Examples

```sh
# Preview the settings and archives an INI would add to a new cfg.
dream-ini --ini Morrowind.ini

# Import everything, load order included, into your cfg.
dream-ini --ini Morrowind.ini --cfg openmw.cfg --game-files --in-place

# Write a separate cfg, searching another Data Files first.
dream-ini --ini Morrowind.ini --game-files --data "/games/Morrowind/Data Files" --output imported.cfg

# A Russian install.
dream-ini --ini Morrowind.ini --cfg openmw.cfg --encoding win1251 --in-place

# Point OpenMW at a portable layout.
dream-ini --ini Morrowind.ini --cfg openmw.cfg -l local-data -u user-data --in-place
```
