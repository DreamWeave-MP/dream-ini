+++
title = "Documentation"
description = "How to import Morrowind.ini into openmw.cfg with dream-ini, what an import changes, and the importer's Rust and Luau API."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"

[extra]
docs_root = true
docs_project_name = "dream-ini"
docs_short_title = "dream-ini docs"
docs_project_path = "@/home/index.md"
docs_repository_url = "https://github.com/DreamWeave-MP/dream-ini/tree/main/content/docs"
docs_sidebar_label = "Documentation"
hide_child_cards = true
kind = "guide"
+++

dream-ini reads a `Morrowind.ini` and writes what OpenMW needs from it into an `openmw.cfg`. The
command line, the GUI and the library run the same importer, so everything here about what an
import does applies to all three.

## Learn it

- **[Start here](@/docs/start-here.md)**: download it, import your `Morrowind.ini`, and check the
  result before OpenMW reads it.
- **[What an import does](@/docs/importing.md)**: every key it reads, where it finds your content
  and archives, the load order it writes, and what it leaves alone.

## Use it

- **[Writing the cfg](@/docs/writing.md)**: preview, a separate file, or your cfg updated in place,
  and what each keeps of the cfg you started from.
- **[The GUI](@/docs/gui.md)**: every field, the three output modes, languages and controllers.
- **[Handhelds](@/docs/handhelds.md)**: the PortMaster build and the muOS app.
- **[Embedding Luau](@/docs/luau-hosts.md)**: giving scripts `@dream/ini` through l3i.

## Look it up

- **[Command line](@/docs/cli.md)**: every option and command, and the exit codes.
- **[Differences from OpenMW's importer](@/docs/differences.md)**: where dream-ini and
  `mwiniimporter` part ways, on purpose.
- **[Platforms and performance](@/docs/compatibility.md)**: what each download can do, the
  license, what is tested, and what an import costs.
- **[Rust API](@/docs/api/_index.md)**: the importer, the parsers and the cfg writers.
- **[Luau API](@/docs/luau/_index.md)**: the `@dream/ini` module and the extension that provides
  it.
