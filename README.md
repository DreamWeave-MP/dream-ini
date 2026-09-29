# dream-ini

Import `Morrowind.ini` into `openmw.cfg`.

`Morrowind.ini` holds what a Morrowind install reads at startup: the load order, the archives, and
hundreds of values for weather, water, moons, fonts and the level-up screen. OpenMW reads none of
it. dream-ini writes it into your `openmw.cfg`: every archive and content file found in your
`Data Files`, the content in the order Morrowind.exe would load it, and the INI's settings as
`fallback=` lines, with everything else in your cfg kept as you wrote it.

It reimplements OpenMW's `mwiniimporter`, with a GUI for desktops, controllers and handhelds, and
the importer as a Rust library and a Luau module.

**Documentation, downloads and the full Rust and Luau API reference:
<https://dreamweave-mp.github.io/dream-ini/>**

## Install

Download the build for your system from the
[releases](https://github.com/DreamWeave-MP/dream-ini/releases): Windows, macOS, Linux, Android,
PortMaster, and a muOS app. Or build it:

```sh
cargo install dream-ini
```

## Use

Run `dream-ini` with no arguments for the GUI. On the command line, a preview goes to standard
output and changes nothing:

```sh
dream-ini --ini Morrowind.ini --cfg openmw.cfg --game-files > preview.cfg
dream-ini --ini Morrowind.ini --cfg openmw.cfg --game-files --in-place
```

## As a library

```toml
[dependencies]
dream-ini = { version = "0.4", default-features = false }
```

```rust
use std::path::Path;

use dream_ini::{ImportOptions, IniImporter};

let result = IniImporter::new(ImportOptions::default())
    .import_optional_cfg_path(Path::new("Morrowind.ini"), Some(Path::new("openmw.cfg")))?;
```

With the `luau` feature, `dream_ini::luau::IniExtension` is an
[l3i](https://github.com/DreamWeave-MP/l3i) extension providing the module `@dream/ini`.

## Where to read next

- [Start here](https://dreamweave-mp.github.io/dream-ini/docs/start-here/): a first import
- [What an import does](https://dreamweave-mp.github.io/dream-ini/docs/importing/): every key, the
  search order and the load order
- [Command line](https://dreamweave-mp.github.io/dream-ini/docs/cli/) and
  [the GUI](https://dreamweave-mp.github.io/dream-ini/docs/gui/)
- [Differences from OpenMW's importer](https://dreamweave-mp.github.io/dream-ini/docs/differences/)
- [Rust API](https://dreamweave-mp.github.io/dream-ini/docs/api/) and
  [Luau API](https://dreamweave-mp.github.io/dream-ini/docs/luau/)

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -W clippy::pedantic -D warnings
cargo test --all-features
```

The `luau` feature needs l3i's toolchain, clang, lld and cross-language thin LTO, which
`.cargo/config.toml` sets. The site in `content/` is a
[DreamWeave Mod Template](https://github.com/DreamWeave-MP/DreamWeave-Mod-Template) site; preview it
with `zola serve`.

## License

`dream-ini` is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
