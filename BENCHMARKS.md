# Benchmarks

`bench/bench.jsl` times the package through its exports: INI parses, cfg parses and
serialization, cfg documents and their edits, the option and multimap checks, and trimming. Each
workload is warmed up, sized to about a tenth of a second a batch, and reported as the median of
seven batches with a full collection before each. Every result is compared with what the
generator of its input says it must be, outside the time.

```sh
jess run --package jpk/dream-ini jpk/dream-ini/bench/bench.jsl [filter]
```

It refuses a build without the verified toolchain.

## l3i 2.0: before and after

2026-10-12, Intel Core i7-10870H @ 2.20GHz, a verified release build of Jess on l3i 2.0.1 (Luau
0.742). Base is `e91f00a`, the package as it was with this bench and its pinning tests added;
head is `c00be7f`. Same binary for both, the two run back to back for six rounds inside one hold
of the machine's bench lock, the order swapped every round, load average 1.2 to 1.9 from other
work, each run as

```sh
systemd-run --user --scope -q -p MemoryMax=3G -p MemorySwapMax=0 \
  target/l3i2/jess run --package <the base's export, or jpk/dream-ini> jpk/dream-ini/bench/bench.jsl
```

Each cell is the median of the six runs' medians. A difference under about 5 percent is noise
on this machine, and the rows whose code did not change show how much. The order matters: with
the base always run first, five rounds read those same rows 4 to 7 percent slower for the head.

| Workload | Base | Head | Base / head |
| --- | ---: | ---: | ---: |
| parseIni: 573 one-key sections (24196 bytes, 1146 lines) | 297.7 µs | 293.3 µs | no change |
| parseIni: dream-ini's large INI (127373 bytes, 6787 lines) | 1429.9 µs | 1395.5 µs | no change |
| parseIni: the shape of Morrowind.ini, CRLF (25347 bytes, 763 lines) | 289.4 µs | 284.0 µs | no change |
| parseCfg: dream-ini's large cfg (514 lines) | 183.0 µs | 145.8 µs | 1.26x |
| parseCfg: a hand-written cfg, CRLF (832 lines) | 323.4 µs | 232.2 µs | 1.39x |
| serializeCfg: dream-ini's large cfg (514 lines) | 36.6 µs | 28.0 µs | 1.30x |
| options.multimap: the large INI's entries (4225 values) | 23.6 µs | 23.1 µs | no change |
| keys: the large INI's entries (648 keys) | 97.8 µs | 93.9 µs | no change |
| options.read: an import's options | 0.9 µs | 0.9 µs | no change |
| document.parse: a commented cfg (576 lines) | 319.9 µs | 221.1 µs | 1.45x |
| document.parse: a hand-written cfg, CRLF (832 lines) | 495.0 µs | 344.8 µs | 1.44x |
| document.render: a commented cfg (512 settings) | 80.7 µs | 43.8 µs | 1.84x |
| document.replaceList: 256 data lines of 512 settings | 34.4 µs | 25.8 µs | 1.33x |
| trim: 1024 values, nothing to trim | 49.9 µs | 48.6 µs | no change |
| trim: 1024 values, spaces and tabs | 149.5 µs | 147.2 µs | no change |
| trim: 1024 values, Unicode spaces | 557.9 µs | 515.2 µs | 1.08x |

What moved is the cfg side, and none of it moved because of syntax. `parseCfg` tested every line
with a setting for a comment through `string.match`; reading the bytes before the `#` took 17 to
18 percent off both rows. It and `document.parse` cut a substring for each key and value and
then trimmed it into a second string; `span` in `src/ini/trim.jsl` finds the trimmed range in
the text, so each becomes a string once, which is the rest of `parseCfg`'s hand-written row
(the large cfg has nothing to trim, and that step left it where it was). `document.parse` also
made a string of every line before deciding what it was, and joined and cleared an empty comment
list for every setting; it walks the text by position now. `render` formatted a string per
setting with interpolation and now hands `table.concat` the pieces in a table sized for them;
`serializeCfg` sizes its table too, from a `sum[...]` over its keys; `replaceList` tests each
entry where it walks them instead of calling a closure per entry. It stays a walk, not a
comprehension: the entries are a caller's, and a comprehension counts to a length that a gap in
them cuts short. Timed alone in rotating order over six rounds, the walk and the comprehension
read 25.3 and 23.9 µs, 6 percent apart and at the edge of the noise. The Unicode walk in `trim`
no longer encodes each leading space to measure it, which pays for the call it gained when the
walk moved into a function `span` shares.

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
