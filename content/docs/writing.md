+++
title = "Writing the cfg"
description = "Preview, a separate file, or your cfg updated in place: what each output mode writes, what it keeps of the cfg you started from, and how relative paths are handled."
weight = 30

[extra]
kind = "guide"
+++

An import always produces a whole cfg. Where it goes is up to you, and nothing is written until
you say where.

| Command line | GUI | Writes |
|---|---|---|
| neither option | **Preview only** | Nothing. The cfg goes to standard output, or the **Generated cfg** tab |
| `--output FILE` | **Save as** | `FILE` |
| `--in-place` | **Update existing openmw.cfg** | The `--cfg` file |

`--in-place` needs `--cfg`, and cannot be combined with `--output`.

## Updating a cfg keeps it as you wrote it

With `--cfg`, and the output in the same folder as that cfg (a preview, `--in-place`, or
`--output` beside it), dream-ini rewrites the file from its own lines. The keys the import
[changes](@/docs/importing.md#what-it-replaces-and-what-it-keeps) get their new values. Everything
else stays as you wrote it: comments, `config=` and `replace=` lines, settings dream-ini knows
nothing about, and paths spelled relative or with OpenMW's tokens, such as `?userdata?`.

Only that one file is written. When it includes others with `config=`, their `data=` folders are
searched for content and archives, but their settings are not copied into it.

## Saving to another folder writes full paths

A relative path in a cfg is relative to the folder the cfg is in. Save the result of `--cfg` into
a different folder and those paths would point somewhere else, so dream-ini writes a flattened cfg
instead: every setting from the whole `config=` chain, with every path in full, and no `config=`
or `replace=` lines, since the chain is already applied. Comments are not kept.

```sh
dream-ini --ini Morrowind.ini --cfg ~/.config/openmw/openmw.cfg --output backup/openmw.cfg
```

## Without --cfg, a new cfg

With no `--cfg`, the import starts from an empty cfg and writes only what it imported. On standard
output, it ends with a comment naming the serializer's version:

```ini
content=Morrowind.esm
data=/games/Morrowind/Data Files
encoding=win1252
fallback=Weather_Sunrise_Time,6
fallback-archive=Morrowind.bsa
no-sound=0
# OpenMW-Config Serializer Version: 2.0.0
```

A `--cfg` that does not exist is the same: dream-ini prints `cfg file does not exist` and starts
empty. The file is created only if it is also where the result goes.

## Relative --data folders

A relative `--data` is relative to the folder the cfg lives in, so the `data=` line means the same
folder to OpenMW:

| Output | `--data` is relative to | The `data=` line is written |
|---|---|---|
| `--output` or `--in-place` | The output cfg's folder | As you typed it |
| Preview with `--cfg` | The `--cfg` file's folder | As you typed it |
| `--output` in another folder than `--cfg` | The output cfg's folder | As a full path |
| Preview without `--cfg` | The current folder | As a full path |

The GUI's **Data Files directory** follows the same rules.

## Messages stay out of the cfg

In a preview, standard output holds the cfg and nothing else; everything else goes to standard
error, so redirecting the output to a file is safe. When writing a file, the progress messages
(`load ini file`, the found files, `write to`) go to standard output, and warnings still go to
standard error.

## Writes are all or nothing

The new cfg is written to a temporary file beside the target, named
`.openmw.cfg.dream-ini-<process>-<n>.tmp`, flushed to disk, and renamed over the old one. A crash
leaves either the old cfg or the new one, never half of either. The file's permission bits are
kept; its owner, ACLs and timestamps are not, because the result is a new file. If the cfg is a
symbolic link, the file it points to is replaced and the link stays.

An import that fails, a missing archive for instance, writes nothing at all.
