+++
title = "Embedding Luau"
description = "Give scripts @dream/ini through l3i: the feature, the toolchain, composing the extension, the dreamIni global, and type definitions."
weight = 55

[extra]
kind = "guide"
+++

With the `luau` feature, dream-ini is an [l3i](https://github.com/DreamWeave-MP/l3i) extension: it
describes the `@dream/ini` module, and a Rust host that runs Luau through l3i composes it into its
runtime. Scripts then import with the same importer as the command line. The crate never creates a
VM and never installs a global; both are the host's decisions.

## Dependencies and toolchain

```toml
[dependencies]
dream-ini = { version = "0.4", default-features = false, features = ["luau"] }
l3i = "0.1"
```

`default-features = false` leaves out the GUI. l3i builds Luau itself, and only with clang, lld
and cross-language thin LTO: its build script refuses any other configuration and names the
missing piece. Cargo does not pass a dependency's configuration on, so the host copies the policy
into its own `.cargo/config.toml`, as dream-ini does:

```toml
[env]
CXX = "clang++"

[target.x86_64-unknown-linux-gnu]
rustflags = ["-Clinker-plugin-lto", "-Clinker=clang", "-Clink-arg=-fuse-ld=lld"]
```

clang and rustc must use the same LLVM major version. The
[l3i toolchain notes](https://github.com/DreamWeave-MP/l3i/blob/main/TOOLCHAIN.md) have the lines
for macOS and Windows, and the measurements behind the rule.

## Composing the extension

`IniExtension` goes into the host's `RuntimePlan`, next to whatever else the host provides. Every
runtime made from the plan can `require("@dream/ini")`:

```rust
use dream_ini::luau::IniExtension;
use l3i::Runtime;
use l3i::extension::RuntimePlan;

fn main() -> l3i::Result<()> {
    let plan = RuntimePlan::builder().extension(IniExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;

    runtime.exec(r#"
        local ini = require("@dream/ini")
        local cfg = ini.parseCfg("content=Morrowind.esm\n")
        assert(cfg.content[1] == "Morrowind.esm")
    "#)
}
```

## The dreamIni global

Scripts written for 0.3 and earlier used a `dreamIni` global. A host that still wants one exposes
the module as a compatibility global through its policy; the global and `require` then return the
same table:

```rust
use dream_ini::luau::{IniExtension, MODULE};
use l3i::Runtime;
use l3i::extension::{RuntimePlan, RuntimePolicy};

fn main() -> l3i::Result<()> {
    let policy = RuntimePolicy::new().compat_global(MODULE, "dreamIni");
    let plan = RuntimePlan::builder().policy(policy).extension(IniExtension).finalize()?;
    let runtime = Runtime::from_plan(&plan)?;

    runtime.exec(r#"assert(dreamIni == require("@dream/ini"))"#)
}
```

`MODULE` is `"@dream/ini"`.

## Types for editors and checks

Every function carries a Luau signature. `plan.type_definitions()` returns the `.d.luau` text for
everything in the plan, `@dream/ini` included, ready to save for an editor's language server.
With l3i's `analysis` feature, `plan.check_definitions()` type-checks those definitions; dream-ini's
tests run it, and type-check a strict script against the module. The
[module page](@/docs/luau/module.md#type-definitions) shows what it generates.

## What scripts get

- **Tables in, tables out.** No userdata: a cfg or a parsed INI is a plain table of string arrays,
  which scripts can read, change and pass back.
- **Strict options.** A misspelled or `snake_case` option is an error that lists the right names.
- **The disk, read-only.** `importPaths` reads the INI, the cfg and the folders it searches.
  Nothing in the module writes a file; the host decides what happens to the result.
- **UTF-8 paths.** Paths in and out are strings; paths that are not valid UTF-8 are outside what
  the module supports.

The [Luau API](@/docs/luau/module.md) lists every function.

## From 0.3

`lua::create_module` and `lua::register` are gone, with `mlua`. Compose `IniExtension` as above.
The functions, their names and their results are the same, with three changes:

- Multimap values must be strings; `mlua` turned numbers into strings.
- Errors name the function and list the known options:
  `ini.importMaps: unknown option 'user_data'; known options are ...`. Import errors read
  `dream.ini: <message>`.
- `parseIni` also takes a `buffer`.
