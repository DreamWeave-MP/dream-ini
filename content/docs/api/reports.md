+++
title = "Reports and errors"
description = "ImportWarning, ImportEvent and ImportError: every variant, when it happens, and the text it displays."
weight = 40

[extra]
kind = "api"
+++

An import that finishes returns warnings and events; one that fails returns an `ImportError` and
changes nothing. Each type's `Display` is the text the command line prints, so a host can show
them as they are, or match on the variants and word them itself, as the GUI does in six languages.

## ImportWarning

{{ api_signature(value="enum ImportWarning") }}

Something skipped. The import went on. `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`.

| Variant | When | Displays |
|---|---|---|
| `IgnoredEmptyValue { key: String }` | An INI line with nothing after `=`. `key` is `Section:Key` | `ignored empty value for key 'KEY'.` |
| `MalformedIniLine { line: String }` | A line starting with `[` that has no `]`, or nothing between the brackets | `ini file wrongly formatted (LINE). Line ignored.` |
| `MissingGameFile { file: String }` | A GameFile not found in any searched folder. The load order stops before it | `GameFile entry not found: FILE. Later GameFile entries were not imported.` |

## ImportEvent

{{ api_signature(value="enum ImportEvent") }}

Something found or added. `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`.

| Variant | When | Displays |
|---|---|---|
| `DataDirAddedForContent { path: PathBuf }` | `path` gets a `data=` line, for a content file found there | `adding data directory used to resolve content files: PATH` |
| `DataDirAddedForArchive { path: PathBuf }` | The same, for an archive | `adding data directory used to resolve fallback archives: PATH` |
| `ContentFileResolved { path: PathBuf, modified: SystemTime }` | With `verbose`: a content file, where it was found, and its modification time | `content file: PATH timestamp = (UNIX SECONDS)` |
| `ArchiveResolved { path: PathBuf }` | With `verbose`: an archive, where it was found | `archive: PATH` |

Content and archive paths are canonical: full, with symbolic links followed.

## ImportError

{{ api_signature(value="enum ImportError") }}

Why an import failed. `Debug`, `Display`, `Error`. Marked `#[non_exhaustive]`: match it with a
wildcard arm.

| Variant | When | Displays |
|---|---|---|
| `Io { path: PathBuf, source: io::Error }` | A file could not be read or written | `PATH: the system's message` |
| `UnsupportedEncoding(String)` | An encoding that is not `win1250`, `win1251` or `win1252` | `unsupported encoding: VALUE` |
| `MissingArchives { files: Vec<String>, searched_paths: Vec<PathBuf> }` | One or more archives were not found | `fallback archives not found: FILES; searched: FOLDERS; pass --data or add data=... to the cfg` |
| `InvalidContentFileName(String)` | A GameFile that is a path, not a file name | `invalid content file name: VALUE; content entries must be plugin filenames, not paths` |
| `InvalidArchiveName(String)` | An Archive entry that is a path | `invalid fallback archive name: VALUE; archive entries must be BSA filenames, not paths` |
| `OpenMwConfig(String)` | openmw-config could not load the cfg, or represent a setting | `OpenMW config error: MESSAGE` |
| `InvalidPluginHeader { path: PathBuf, message: String }` | From [`read_plugin_header`](@/docs/api/types.md#read-plugin-header) only | `invalid plugin header in PATH: MESSAGE` |
| `MissingContentFiles { files: Vec<String>, searched_paths: Vec<PathBuf> }` | Never, since 0.2.0: a missing GameFile is the `MissingGameFile` warning | `content files not found: FILES; ...` |

`searched_paths` lists every folder that was to be searched, including ones that do not exist.
