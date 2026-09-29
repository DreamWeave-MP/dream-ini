+++
title = "Handhelds"
description = "The PortMaster build, which draws Dream INI straight to the framebuffer, the muOS app, and the settings for troubleshooting either."
weight = 45

[extra]
kind = "guide"
+++

OpenMW runs on ARM handhelds, and those need an `openmw.cfg` as much as a desktop does. dream-ini
has two downloads for them, which hold the same program.

| Download | Is |
|---|---|
| `dream-ini-Portmaster-ARM64.zip` | The program, for 64-bit ARM Linux handhelds |
| `dream-ini-Portmaster-ARM64.muxapp` | The same program, packed as a muOS app |

The PortMaster build is built against glibc 2.34, like PortMaster's ports, so it runs on the
firmwares PortMaster supports. It has the command line and the [GUI](@/docs/gui.md), which it draws
straight to the framebuffer (`/dev/fb0`) rather than through a window system. There is no
clipboard, so **Copy** does nothing; everything else in the window works.

## muOS

1. Copy `dream-ini-Portmaster-ARM64.muxapp` into the `ARCHIVE` folder on your SD card.
2. On the handheld, install it with the Archive Manager.
3. Start **Dream INI** from Applications.

It installs to `/mnt/mmc/MUOS/application/Dream INI`. What the program prints goes to
`logs/dream-ini.log` there.

## Other firmwares

The zip holds only the `dream-ini` program. Copy it to the handheld and start it from a launcher
script of your own, or from a terminal, with no arguments for the GUI.

## Controls

The handheld GUI uses the controls in [The GUI](@/docs/gui.md#controllers), read from
`/dev/input` directly. Anbernic-style handhelds report Start, Select and the shoulder buttons with
their own codes, and the PortMaster build maps them to the same actions. The help line at the
bottom of the screen always shows the buttons for what is on it.

Paths are typed on the on-screen keyboard: select a path field and press A.

## When something is wrong

The GUI writes `dream-ini-portmaster.log` beside the program, or in the current folder if it
cannot write there. It records how the program was started, the framebuffer and refresh rate it
chose, and any fatal error or crash.

| Environment variable | Does |
|---|---|
| `DREAM_INI_PORTMASTER_REFRESH_HZ` | The frame rate to draw at, from 15 to 120. By default, the display's own rate, or 60 if it does not say |
| `DREAM_INI_CONTROLLER_LOG` | Any value but `0` prints every controller event to standard error, to find a button that does nothing. On muOS that is `logs/dream-ini.log` |

The `DREAM_INI_PM_*` and `DREAM_INI_FB_*` variables are for measuring the renderer; they are not
needed to use it.
