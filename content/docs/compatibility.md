+++
title = "Platforms and performance"
description = "What each download can do, how to build dream-ini yourself and with which features, how releases are signed, what is tested, and what an import costs."
weight = 70

[extra]
kind = "reference"
+++

## Downloads

| Download | For | Has |
|---|---|---|
| `dream-ini-Windows-X64.zip` | Windows, x86-64 | GUI and command line |
| `dream-ini-macOS-ARM64.zip` | macOS, Apple silicon | GUI and command line |
| `dream-ini-macOS-X64.zip` | macOS, Intel | GUI and command line |
| `dream-ini-Linux-X64.zip` | Linux, x86-64 | GUI and command line |
| `dream-ini-Portmaster-ARM64.zip` | ARM64 Linux handhelds, glibc 2.34 or newer | Framebuffer GUI and command line |
| `dream-ini-Portmaster-ARM64.muxapp` | muOS | The PortMaster build as a muOS app |
| `dream-ini-Android-ARM64.zip` | Android 6 (API 23) or newer, ARM64 | Command line. A program for a terminal such as Termux, not an app |

Controllers work in the GUI on Windows, macOS, Linux and PortMaster. `install-launcher` works on
Windows and Linux.

## Building it yourself

```sh
cargo install dream-ini
```

That builds the desktop program, GUI included. Other shapes are a matter of Cargo features:

| Feature | Default | Adds |
|---|---|---|
| `gui` | Yes | The desktop GUI, through eframe, with `controller-support` |
| `controller-support` | With `gui` | Controllers: `/dev/input` on Linux, gilrs on Windows and macOS |
| `portmaster-gui` | No | The framebuffer GUI, drawn by dream-soft-render. Use without `gui` |
| `luau` | No | The [`@dream/ini` Luau extension](@/docs/luau/_index.md). `lua` is the old name |

The release builds are exactly these:

| Build | Features |
|---|---|
| Windows, macOS, Linux | `gui` |
| PortMaster | `--no-default-features --features portmaster-gui` |
| Android | `--no-default-features` |

A program that only wants the importer depends on the library without the GUI:

```toml
[dependencies]
dream-ini = { version = "0.4", default-features = false }
```

## Signed releases

Each release archive holds, beside the program, a Sigstore bundle for it, such as
`dream-ini-Linux-X64.bundle`, made by StroggForge's release workflow. It proves that workflow built
the program for this repository:

```sh
cosign verify-blob dream-ini \
  --bundle dream-ini-Linux-X64.bundle \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/DreamWeave-MP/StroggForge/\.github/workflows/rustGlobalBuild\.yml@' \
  --certificate-github-workflow-repository DreamWeave-MP/dream-ini
```

Each GitHub release also links every archive's VirusTotal scan.

## License

MIT OR Apache-2.0, at your option, since 0.3.1. 0.1.0 and 0.2.0 were GPL-3.0-only.

## What is tested

Every push runs [StroggForge](https://github.com/DreamWeave-MP/StroggForge)'s release workflow:
the tests with every feature on Windows, Linux and macOS on both Apple silicon and Intel, Clippy at
the pedantic level with warnings as errors, `rustfmt`, and `cargo audit`, before anything is
built for release.

The tests import real files: INIs with each edge of the GameFile and Archive rules, cfgs with
comments, relative paths, symbolic links and `config=` chains, and every output mode through the
built program. The Luau tests type-check the extension's definitions with Luau's own checker, run
a strict script against them, and drive every function.

## What an import costs

`cargo bench --bench round_trip` imports a large synthetic install: 4226 INI entries in 1543
sections, a 514-entry cfg, 129 archives and 256 `data=` folders that no longer exist. Medians on
one quiet machine, release build:

| Step | 0.3.1 | 0.4.0 |
|---|---:|---:|
| The whole import: parse, find every file, write the cfg | 198.5 ms | 4.97 ms |
| Parse the INI | 1.465 ms | 1.029 ms |
| Parse the cfg | 68.3 µs | 58.1 µs |
| Serialize the cfg | 3.20 µs | 3.20 µs |

0.4.0 is faster because it no longer probes search paths that are not folders, and resolves the
cfg's `data=` folders once per import instead of once per file found.

From Luau, `cargo bench --features luau --bench luau_boundary` runs each function from a frozen
script on the same install. Medians, against the `mlua` binding it replaced in 0.4.0, both over
0.3.1's importer:

| Call | mlua, 0.3.1 | l3i |
|---|---:|---:|
| `importPaths` | 205.9 ms | 208.8 ms |
| `importMaps` | 2.277 ms | 2.126 ms |
| `parseIni` | 2.816 ms | 2.452 ms |
| `parseCfg` | 131.9 µs | 141.7 µs |
| `serializeCfg` | 54.4 µs | 55.0 µs |

The binding costs about what it did; the import is what got faster. With 0.4.0's importer,
`importPaths` on this install takes 4.87 ms.
