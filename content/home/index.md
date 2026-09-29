+++
title = "dream-ini"
description = "Import Morrowind.ini into openmw.cfg: a command line, a desktop and handheld GUI, and a Rust and Luau library."

[taxonomies]
tags = ["OpenMW", "Morrowind", "Rust", "Luau", "PortMaster"]
+++

`Morrowind.ini` is where a Morrowind install keeps what the engine reads at startup: the load
order, the archives, and hundreds of values for weather, water, moons, fonts and the level-up
screen. OpenMW reads none of it. It reads `openmw.cfg`.

dream-ini moves one into the other. Point it at your `Morrowind.ini`, and at your `openmw.cfg` if
you have one, and it writes the cfg OpenMW needs: every archive and content file found in your
`Data Files`, the content in the order Morrowind.exe would load it, and the INI's settings as
`fallback=` lines. Everything in your cfg that it does not import stays exactly as it was,
comments included.

{{ schematic(data_path="data/schematics/import.json") }}

It is a reimplementation of OpenMW's `mwiniimporter`, in Rust, with a GUI that works with a mouse,
a keyboard or a controller, on the desktop and on handhelds. Where it behaves differently from the
C++ tool, it is on purpose, and [written down](@/docs/differences.md).

```sh
dream-ini --ini "Morrowind/Morrowind.ini" --cfg ~/.config/openmw/openmw.cfg --game-files --in-place
```

Run it with no arguments and you get the same import as a window:

- **Desktop**: Windows, Linux and macOS, driven by mouse, keyboard or a controller.
- **Handhelds**: a PortMaster build that draws straight to the framebuffer, and a muOS app.
- **Android**: the command line.
- **Library**: the importer as a Rust crate, and as the Luau module `@dream/ini`.

## Documentation

- **[Start here](@/docs/start-here.md)**: your first import, in the GUI or on the command line.
- **[What an import does](@/docs/importing.md)**: every key it reads, where it looks for files, and
  the load order it writes.
- **[Command line](@/docs/cli.md)**: every option, the output modes and exit codes.
- **[Rust API](@/docs/api/_index.md)** and **[Luau API](@/docs/luau/_index.md)**: the importer as
  a library.
