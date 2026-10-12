--!strict
--!native
-- Generated text through every parser, the document's edits, and trimming, held to digests of
-- what the package answered when this file was written: its explicit cases say what the edges
-- mean, and these say that nothing else moved. A digest that changes is a change of behaviour on
-- some input here; `Seed` and the generators give the same inputs on every run.
local bytes = require '@dream/bytes'
local document = require '@dream-ini/document'
local ini = require '@dream-ini'
local parser = require '@dream-ini/parser'
local test = require '@jess/test'

local suite, expect = test.new(), test.expect

local Seed = 0x2545f491

local Stamp = '# OpenMW-Config Serializer Version: 1'

--- Whitespace as the parsers and `trim` tell it apart: ASCII, the Unicode spaces of each lead
--- byte the fast path looks for, and a space that only looks like one.
local Spaces = {
  ' ',
  ' ',
  ' ',
  '\t',
  '\t',
  '\r',
  '\v',
  '\f',
  '\u{85}',
  '\u{A0}',
  '\u{1680}',
  '\u{2003}',
  '\u{2028}',
  '\u{202F}',
  '\u{205F}',
  '\u{3000}',
  '\u{200B}',
}

--- What a line is made of: text, the bytes a parser gives meaning to, whitespace, letters outside
--- ASCII, bytes that are not UTF-8, and the keys a document treats specially.
local Words = {
  'a',
  'B',
  'key',
  'Some Value',
  '0.75',
  '12,34,56',
  'data',
  'content',
  'fallback',
  'encoding',
  'config',
  'win1251',
  ' ',
  ' ',
  ' ',
  '\t',
  '=',
  '=',
  ';',
  '#',
  '[',
  ']',
  ':',
  '\r',
  '\0',
  '\u{E9}',
  '\u{416}',
  '\u{A0}',
  '\u{2003}',
  '\u{3000}',
  '\u{FEFF}',
  '\194',
  '\226\128',
  '\255',
}

local random = Seed

local function nextRandom(): number
  random = bit32.bxor(random, bit32.lshift(random, 13))
  random = bit32.bxor(random, bit32.rshift(random, 17))
  random = bit32.bxor(random, bit32.lshift(random, 5))
  return random
end

--- Up to `most` of `list`'s strings, one after another.
local function run(list: { string }, most: number): string
  return table.concat([for _ in range(1, nextRandom() % (most + 1)) => list[nextRandom() % #list + 1]])
end

--- One line: blank, a comment, a setting, and, unless the text is `cfgShaped`, a section or
--- whatever comes.
local function line(cfgShaped: boolean): string
  local kind = nextRandom() % (if cfgShaped then 17 else 20)
  if kind < 2 then return run(Spaces, 3) end
  if kind == 2 then return Stamp end
  if kind < 5 then return `{run(Spaces, 2)}#{run(Words, 6)}` end
  if kind < 17 then return `{run(Spaces, 1)}{run(Words, 4)}={run(Words, 6)}{run(Spaces, 1)}` end
  if kind < 19 then return `{run(Spaces, 1)}[{run(Words, 4)}]{run(Words, 1)}` end

  return run(Words, 8)
end

local Endings = { '\n', '\n', '\n', '\r\n', '\r\n', '\r', '\r\r\n' }

--- `count` texts of up to 24 lines, every other one holding only what a cfg holds.
local function texts(count: number): { string }
  local result: { string } = {}

  for index = 1, count do
    local parts: { string } = {}

    for _ = 1, nextRandom() % 25 do
      table.insert(parts, line(index % 2 == 0))
      table.insert(parts, Endings[nextRandom() % #Endings + 1])
    end

    if index % 3 == 0 then table.remove(parts) end

    result[index] = table.concat(parts)
  end

  return result
end

--- A value as text that two different values never share: strings with their length, arrays in
--- order, other tables by their sorted keys.
local function canonical(value: unknown): string
  if type(value) == 'string' then return `{#value}:{value}` end
  if type(value) ~= 'table' then return tostring(value) end

  local map = value :: { [unknown]: unknown }

  if #map > 0 or next(map) == nil then
    return `[{table.concat([for item in map :: { unknown } => canonical(item)], ',')}]`
  end

  local keys: { string } = {}

  for key in map do
    table.insert(keys, key :: string)
  end

  table.sort(keys)
  return `<{table.concat([for key in keys => `{canonical(key)}={canonical(map[key])}`], ',')}>`
end

--- What `call` answered for `text`, or what it raised. An error Luau raises names the chunk and
--- line it came from, which a packed package and an edited file spell differently, so the one
--- such error these calls raise is kept by its words alone.
local function outcome(call: (text: string) -> unknown, text: string): string
  -- `trim` raises on a first or last character that isn't UTF-8, through `parseCfg` and
  -- `document.parse` too, and `document.parse` raises on a line that is neither a comment nor a
  -- setting. The soup holds both, and what is raised is part of what is held.
  local ok, result = pcall(call, text)
  if ok then return canonical(result) end

  local message = tostring(result)
  return if string.find(message, 'invalid UTF-8 code', 1, true) then '!invalid UTF-8 code' else `!{message}`
end

local function digest(answers: { string }): string return bytes.sha256(table.concat(answers, '\n')) end

local soup = texts(600)
local padded = [for _ in range(1, 4000) => `{run(Spaces, 3)}{run(Words, 3)}{run(Spaces, 3)}`]

suite.describe('generated text', function()
  suite.it('parses as INI text', function()
    local answers = [for text in soup => canonical(parser.parseText(text))]
    expect(digest(answers)).toBe 'd751f527c887dc41135e73e8f604a65148203e21669b421b21a11c46acad9660'
  end)

  suite.it('parses as an INI in each encoding, with and without a byte order mark', function()
    local answers: { string } = {}

    for index, text in soup do
      local marked = if index % 4 == 0
        then `\239\187\191{text}`
        elseif index % 4 == 1 then `\255\254{text}`
        elseif index % 4 == 2 then `\254\255{text}`
        else text

      for _, encoding in { 'win1250', 'windows-1251', 'WIN1252' } do
        table.insert(answers, canonical(ini.parseIni(marked, { encoding = encoding })))
        table.insert(answers, canonical(ini.parseIni(buffer.fromstring(text), { encoding = encoding })))
      end
    end

    expect(digest(answers)).toBe '2d365ea23406f27447e5bb19994a8231158e284ff3692f131a8ba2338b17c1b5'
  end)

  suite.it('parses as a cfg, and serializes back', function()
    local answers: { string } = {}

    for _, text in soup do
      local answer = outcome(parser.parseCfg, text)
      table.insert(answers, answer)
      if string.sub(answer, 1, 1) == '!' then continue end

      local map = parser.parseCfg(text)
      table.insert(answers, canonical(parser.keys(map)))
      table.insert(answers, ini.serializeCfg(map))
    end

    expect(digest(answers)).toBe '0ccfdca549a4cd8fee6732adf404c56ac384593ad8b9e8aac1b0dafd0a53f3dc'
  end)

  suite.it('parses as a document, renders, and takes edits', function()
    local answers: { string } = {}
    local parsed = 0

    for index, text in soup do
      local answer = outcome(document.parse, text)
      table.insert(answers, answer)
      if string.sub(answer, 1, 1) == '!' then continue end

      parsed += 1
      local edited = { path = '', entries = document.parse(text) }
      table.insert(answers, document.render(edited))
      document.replaceList(edited, 'data', if index % 4 == 0 then nil else { 'first', 'second' })
      document.append(edited, 'content', { `Plugin {index}.esp` })
      table.insert(answers, canonical(edited.entries))
      table.insert(answers, ini.renderDocument(edited))
    end

    expect(parsed > 100).toBe(true)
    expect(digest(answers)).toBe '458ed5a916f2c44236eea9437785d8cd0f7f519fb739bbf03aa603c56a672e26'
  end)

  suite.it('trims', function()
    local answers = [for value in padded => outcome(parser.trim, value)]
    expect(#[for answer in answers if string.sub(answer, 1, 1) == '!' => answer] > 10).toBe(true)
    expect(digest(answers)).toBe '5bda6993bb60a627fa40352b46acbf0f80f7d359a86704cb67fc82e94fdfca3d'
  end)
end)

return suite
