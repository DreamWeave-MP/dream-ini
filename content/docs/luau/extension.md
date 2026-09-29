+++
title = "IniExtension"
description = "The l3i extension that provides @dream/ini, and the constants a host uses to expose it."
weight = 20

[extra]
kind = "api"
+++

Rust, in the `luau` module, behind the `luau` feature. [Embedding Luau](@/docs/luau-hosts.md)
shows it in a host.

## IniExtension

{{ api_signature(value="struct IniExtension") }}

The `dream.ini` extension: an `l3i::extension::Extension` that describes the `@dream/ini` module
and its type signatures. It holds no state; `Clone`, `Copy`, `Debug`, `Default`. Add it to a plan
with `RuntimePlan::builder().extension(IniExtension)`.

It never creates a runtime, declares no userdata, takes no tags, checks no capabilities, and
installs no global.

{{ api_signature(value="fn extension() -> IniExtension") }}

The same value, for hosts that collect extensions from functions.

## Constants

{{ api_signature(value='const EXTENSION_ID: &str = "dream.ini"') }}

The extension's id in the plan.

{{ api_signature(value='const MODULE: &str = "@dream/ini"') }}

The path scripts `require`. A host that wants the conventional global adds
`RuntimePolicy::new().compat_global(MODULE, "dreamIni")`.
