+++
title = "The GUI"
description = "The Dream INI window: every field, the three output modes, the result tabs, the file picker, languages, and controller navigation."
weight = 40

[extra]
kind = "guide"
+++

Run `dream-ini` with no arguments and it opens Dream INI, the same import as the command line in
a window. It asks for the same things in the same order: a `Morrowind.ini`, optionally a cfg,
what to import, and where the result goes. Nothing is written until you import with an output
chosen.

The desktop builds for Windows, Linux and macOS open a normal window, for mouse, keyboard or a
controller. The [PortMaster build](@/docs/handhelds.md) draws the same window full screen, for a
controller. The Android build has no GUI.

## Source

- **Morrowind.ini**: the INI to import. Required.
- **Existing**: the `openmw.cfg` to start from. Optional; without it, the import starts from an
  empty cfg, as without `--cfg`.

Type a path, or use **Browse…**.

## Import options

- **Encoding**: **Auto** uses the cfg's `encoding=`, or `win1252` if it has none. Or choose
  `win1250`, `win1251` or `win1252`. See [Encoding](@/docs/importing.md#encoding).
- **Import bitmap fonts**: `--fonts`. Off.
- **Import archives**: the opposite of `--no-archives`. On.
- **Import content files / load order**: `--game-files`. Off.

## Overrides

- **Data Files directory**: `--data`, a folder searched for content and archives before any other.
- **data-local**, **resources**, **user-data**: `--data-local`, `--resources` and `--user-data`.
  Each replaces that setting in the result, and none is searched for files. Hover over each for
  what OpenMW uses it for.

## Output

| Choice | Writes | Command line |
|---|---|---|
| **Preview only** | Nothing | neither `--output` nor `--in-place` |
| **Save as** | The **Output path** | `--output` |
| **Update existing openmw.cfg** | The **Existing** cfg | `--in-place` |

Press **Import / Preview**. The button says why it cannot run yet: no `Morrowind.ini`, no output
path for **Save as**, or no existing cfg to update. [Writing the cfg](@/docs/writing.md) covers
what each choice keeps of the cfg you started from.

## Results

| Tab | Shows |
|---|---|
| **Errors** | Why the import failed. Nothing was written |
| **Warnings** | Lines skipped and a load order that stopped early |
| **Events** | Every content file and archive found, and every `data=` folder added |
| **Generated cfg** | The whole cfg, exactly as written or as it would be |

**Copy** puts the generated cfg on the clipboard, and **Clear** empties the results. After a write,
the window names the file it wrote.

## Picking files

**Browse…** opens Dream INI's own picker rather than the system's, so it works the same with a
controller and on a handheld. It starts where the field's path points. **Parent** goes up a
folder, **Refresh** rereads it, and **Show hidden directories** reveals folders such as
`~/.config/openmw`. For an output file, type its name under **File name**.

## Languages

**Language** switches the window between English, French, German, Russian, Spanish and Swedish.
The window starts in English.

## Controllers

On Linux, Dream INI reads controllers from `/dev/input` itself. If your session cannot read those
devices, the window still works, without the controller; adding yourself to the group that owns
them, usually `input`, fixes it. Windows and macOS read controllers through
[gilrs](https://crates.io/crates/gilrs).

On the form:

| Button | Does |
|---|---|
| D-pad or left stick | Move between fields |
| A | Toggle a checkbox, open a picker, or type into a path with the on-screen keyboard |
| X | Clear the selected path |
| Left and right | Change the selected option, such as the encoding or the language |
| Start | Import |
| Select, or B | Leave Dream INI |
| Right stick | Scroll the generated cfg |
| LB and RB | Page through the generated cfg |

In the picker:

| Button | Does |
|---|---|
| A | Open the selected folder, or choose the selected file |
| B or left | Go to the parent folder |
| Right | Open the selected folder, or choose the selected file |
| Start | Choose the current or expected path |
| LB | Show or hide hidden folders |
| Select | Cancel |

On the path keyboard, A presses a key, B shifts, Y types a space, X deletes, Start applies, and
Select cancels. The window shows the buttons for whatever is on screen.
