# Benchmarks

`bench/bench.luau` times the package through its exports: INI parses, cfg parses and
serialization, cfg documents and their edits, the option and multimap checks, and trimming. Each
workload is warmed up, sized to about a tenth of a second a batch, and reported as the median of
seven batches with a full collection before each. Every result is compared with what the
generator of its input says it must be, outside the time.

```sh
jess run --package jpk/dream-ini jpk/dream-ini/bench/bench.luau [filter]
```

It refuses a build without the verified toolchain.

## l3i 2.0: before and after

2026-10-12, Intel Core i7-10870H @ 2.20GHz, a verified release build of Jess on l3i 2.0.1 (Luau
0.742). Base is `e91f00a`, the package as it was with this bench and its pinning tests added;
head is `50c69f0`. Same binary for both, the two alternated back to back for five rounds inside
one hold of the machine's bench lock, load average 1.5 to 2.0 from other work, each run as

```sh
systemd-run --user --scope -q -p MemoryMax=3G -p MemorySwapMax=0 \
  target/l3i2/jess run --package <the base's export, or jpk/dream-ini> jpk/dream-ini/bench/bench.luau
```

Each cell is the median of the five runs' medians. A difference under about 5 percent is noise
on this machine, and the rows whose code did not change show how much.

| Workload | Base | Head | Base / head |
| --- | ---: | ---: | ---: |
| parseIni: 573 one-key sections (24196 bytes, 1146 lines) | 293.3 µs | 304.9 µs | no change |
| parseIni: dream-ini's large INI (127373 bytes, 6787 lines) | 1409.7 µs | 1448.3 µs | no change |
| parseIni: the shape of Morrowind.ini, CRLF (25347 bytes, 763 lines) | 283.6 µs | 281.1 µs | no change |
| parseCfg: dream-ini's large cfg (514 lines) | 181.8 µs | 148.2 µs | 1.23x |
| parseCfg: a hand-written cfg, CRLF (832 lines) | 321.0 µs | 235.2 µs | 1.37x |
| serializeCfg: dream-ini's large cfg (514 lines) | 36.3 µs | 28.6 µs | 1.27x |
| options.multimap: the large INI's entries (4225 values) | 23.4 µs | 23.1 µs | no change |
| keys: the large INI's entries (648 keys) | 96.1 µs | 94.1 µs | no change |
| options.read: an import's options | 0.9 µs | 0.9 µs | no change |
| document.parse: a commented cfg (576 lines) | 307.5 µs | 218.6 µs | 1.41x |
| document.parse: a hand-written cfg, CRLF (832 lines) | 500.9 µs | 342.0 µs | 1.46x |
| document.render: a commented cfg (512 settings) | 80.9 µs | 45.2 µs | 1.79x |
| document.replaceList: 256 data lines of 512 settings | 34.2 µs | 24.6 µs | 1.39x |
| trim: 1024 values, nothing to trim | 49.9 µs | 49.8 µs | no change |
| trim: 1024 values, spaces and tabs | 151.5 µs | 155.3 µs | no change |
| trim: 1024 values, Unicode spaces | 560.8 µs | 536.2 µs | 1.05x, at the edge of the noise |

What moved is the cfg side, and none of it moved because of syntax. `parseCfg` tested every line
with a setting for a comment through `string.match`; reading the bytes before the `#` took 17 to
18 percent off both rows. It and `document.parse` cut a substring for each key and value and
then trimmed it into a second string; `span` in `src/ini/trim.luau` finds the trimmed range in
the text, so each becomes a string once, which is the rest of `parseCfg`'s hand-written row
(the large cfg has nothing to trim, and that step left it where it was). `document.parse` also
made a string of every line before deciding what it was, and joined and cleared an empty comment
list for every setting; it walks the text by position now. `render` formatted a string per
setting with interpolation and now hands `table.concat` the pieces in a table sized for them;
`serializeCfg` sizes its table too, from a `sum[...]` over its keys; `replaceList` filters with
a comprehension where it called a closure per entry. The Unicode walk in `trim` no longer
encodes each leading space to measure it, which pays for the call it gained when the walk moved
into a function `span` shares.

`parseIni` is where it was, and the measurements say why it stays there. Of the large INI's
1.4 ms, timed in a scratch copy of the parser with parts of it taken out, about 570 µs is the
walk (some 13,600 plain finds, 32 ns each where the newline finds were timed alone, and two
`string.byte` checks a line), about 700 µs is making the strings it returns and the two it needs
on the way to them (a section, a key's name), and the rest is the tables. Four ways round that
were measured and left out, none faster than what is there:

- **A line index from the data plane.** One `data.compare(bytes, 'u8', 0, length, 'eq', 10)`
  over the 127 KB costs 254 µs, and 354 µs with `indices()`, against 214 µs for the 6,787
  finds it would replace: `compare` reads about 2 ns a byte where `memchr` skips a whole line
  per call. `data.partition` took 527 µs.
- **Keys assembled in a buffer**, the section's prefix written before each name so that one
  `buffer.readstring` makes `Section:Name` without a string for the name: 1549 µs against 1473
  µs for the same scratch parser without it. The two buffer calls cost more than the string
  they save.
- **The section kept as `Section:`**, so a key is a two-part concatenation: 0.88x, 0.92x and
  0.95x on the three INI rows. A section with one key pays for a string it didn't need.
- **No `\r` check on a text that holds no `\r`**, found with one `string.find` first: within
  the noise on the two LF rows, 0.93x on the CRLF one.

The primitive `site/content/docs/scripting/packages.md` asks for is still the one that would
move this: a native call that returns every line's spans, or every position of a byte, at
`memchr`'s speed. l3i 2.0's data plane is not that for bytes of text.
