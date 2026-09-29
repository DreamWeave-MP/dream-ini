+++
title = "Start here"
description = "Download dream-ini, import your Morrowind.ini into openmw.cfg, and check the result before OpenMW reads it."
weight = 10

[extra]
kind = "tutorial"
+++

You need a Morrowind install with its `Morrowind.ini`, and OpenMW. If OpenMW has run before, you
also have an `openmw.cfg` for dream-ini to update; if not, dream-ini can write a new one.

## Download it

Take the archive for your system from the [project page](@/home/index.md) and unzip it
anywhere. It holds one program, `dream-ini`. There is nothing to install.

- **macOS**: the builds are not notarized. If macOS will not open it, run
  `xattr -d com.apple.quarantine dream-ini` in its folder, or allow it under System Settings,
  Privacy & Security.
- **Linux and macOS**: if the shell says permission denied, `chmod +x dream-ini`.
- **Handhelds**: see [Handhelds](@/docs/handhelds.md) for the PortMaster build and the muOS app.

## Find your two files

| File | Where it usually is |
|---|---|
| `Morrowind.ini` | The Morrowind install folder, next to `Morrowind.exe` and `Data Files` |
| `openmw.cfg` on Linux | `~/.config/openmw/openmw.cfg` |
| `openmw.cfg` on Windows | `Documents\My Games\OpenMW\openmw.cfg` |
| `openmw.cfg` on macOS | `~/Library/Preferences/openmw/openmw.cfg` |

Copy your `openmw.cfg` somewhere safe first. dream-ini replaces a file in one step, so a crash
never leaves half of one, but an import is meant to change it: with content files on, it replaces
your load order with the one in `Morrowind.ini`.

## Import in the GUI

Run `dream-ini` with no arguments; on Windows, double-click it.

1. Under **Source**, choose your `Morrowind.ini`, and your `openmw.cfg` as **Existing**.
2. Under **Import options**, tick **Import content files / load order** if you want the INI's
   load order. **Import archives** is already on.
3. Leave **Output** on **Preview only** and press **Import / Preview**.
4. Read the result tabs. **Warnings** lists anything skipped; **Events** lists every archive and
   content file found, and where; **Generated cfg** is exactly what would be written.
5. When the preview is right, choose **Update existing openmw.cfg** and import again.

[The GUI](@/docs/gui.md) covers every field.

## Or on the command line

Without `--output` or `--in-place`, dream-ini writes the cfg to standard output and changes
nothing, which makes it a preview:

```sh
dream-ini --ini "Morrowind/Morrowind.ini" --cfg ~/.config/openmw/openmw.cfg --game-files > preview.cfg
```

Messages and warnings go to standard error, so `preview.cfg` holds only the cfg. When it is
right, write it back:

```sh
dream-ini --ini "Morrowind/Morrowind.ini" --cfg ~/.config/openmw/openmw.cfg --game-files --in-place
```

Leave out `--game-files` to keep your cfg's own load order and import only the settings and the
archives.

## What you get

An import of a Morrowind install with both expansions and one plugin, into a cfg the OpenMW
launcher wrote, adds this after the cfg's own lines:

```ini
encoding=win1252
no-sound=0
fallback=Weather_Sunrise_Time,6
fallback=Weather_Sunset_Time,18
data=/games/Morrowind/Data Files
content=Morrowind.esm
content=Tribunal.esm
content=Bloodmoon.esm
content=Better Clothes.esp
fallback-archive=Morrowind.bsa
fallback-archive=Tribunal.bsa
fallback-archive=Bloodmoon.bsa
```

A real `Morrowind.ini` gives several hundred `fallback=` lines: weather, water, moons, the
level-up texts and the rest. [What an import does](@/docs/importing.md) explains each line.

## If it stops

- **`ini file does not exist`**: the path after `--ini` is wrong. The exit code is 253.
- **`fallback archives not found: Tribunal.bsa`**: an archive the INI lists is not in any folder
  dream-ini searched, and the message lists those folders. Pass the folder with `--data`, or fix
  the INI. Nothing was written.
- **`GameFile entry not found`**: a warning, not an error. The load order stops at that file, as
  it does in Morrowind.exe; the files before it are imported.
