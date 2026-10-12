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

--- What stands before a comment's `#`: mostly what a cfg takes for the start of a comment.
local Leads = { ' ', ' ', '\t', '\t', '\r', '\r', '\v', '\u{A0}' }

--- What a line is made of: text, the bytes a parser gives meaning to, whitespace, letters outside
--- ASCII, and the keys a document treats specially.
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
}

--- Bytes that are not UTF-8: a lead byte alone, a character cut short, a byte no character has.
local Broken = { '\194', '\226\128', '\255' }

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

--- Up to `most` words, one after another; one in `rarity` is not UTF-8, and none when it is 0.
local function words(most: number, rarity: number): string
  local parts: { string } = {}

  for _ = 1, nextRandom() % (most + 1) do
    local list = if rarity > 0 and nextRandom() % rarity == 0 then Broken else Words
    table.insert(parts, list[nextRandom() % #list + 1])
  end

  return table.concat(parts)
end

--- One line: blank, a comment, a setting, and, unless the text is `cfgShaped`, a section or
--- whatever comes.
local function line(cfgShaped: boolean, rarity: number): string
  local kind = nextRandom() % (if cfgShaped then 17 else 20)
  if kind < 2 then return run(Spaces, 3) end
  if kind == 2 then return Stamp end
  if kind < 6 then return `{run(Leads, 3)}#{words(6, rarity)}` end

  if kind < 17 then
    return `{run(Spaces, 1)}{words(4, rarity)}={words(6, rarity)}{run(Spaces, 1)}`
  end

  if kind < 19 then return `{run(Spaces, 1)}[{words(4, rarity)}]{words(1, rarity)}` end

  return words(8, rarity)
end

local Endings = { '\n', '\n', '\n', '\r\n', '\r\n', '\r', '\r\r\n' }

--- `count` texts of up to 24 lines. One in three holds only what a cfg holds, all of it UTF-8;
--- one in three the same with bytes that are not; one in three whatever an INI might.
local function texts(count: number): { string }
  local result: { string } = {}

  for index = 1, count do
    local parts: { string } = {}

    for _ = 1, nextRandom() % 25 do
      table.insert(parts, line(index % 3 ~= 2, if index % 3 == 0 then 0 else 40))
      table.insert(parts, Endings[nextRandom() % #Endings + 1])
    end

    if index % 4 == 0 then table.remove(parts) end

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

local function raised(answer: string): boolean return string.sub(answer, 1, 1) == '!' end

local function digest(answers: { string }): string return bytes.sha256(table.concat(answers, '\n')) end

local soup = texts(900)
local padded = [for _ in range(1, 4000) => `{run(Spaces, 3)}{words(3, 12)}{run(Spaces, 3)}`]

suite.describe('generated text', function()
  suite.it('parses as INI text', function()
    local answers = [for text in soup => canonical(parser.parseText(text))]
    expect(digest(answers)).toBe '8207584ba5140d11a83165b523411bd985b722f35323fd94f14d287795a871c7'
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

    expect(digest(answers)).toBe '2f4d76e0a44e4c60aed0c31482f8ef15cfcbcfffe1992b6484711929c4da4ba9'
  end)

  suite.it('parses as a cfg, and serializes back', function()
    local answers: { string } = {}
    local parsed = 0

    for _, text in soup do
      local answer = outcome(parser.parseCfg, text)
      table.insert(answers, answer)
      if raised(answer) then continue end

      parsed += 1
      local map = parser.parseCfg(text)
      table.insert(answers, canonical(parser.keys(map)))
      table.insert(answers, ini.serializeCfg(map))
    end

    expect(parsed).toBe(693)
    expect(digest(answers)).toBe '46963cb2f310b82a8192ca78066ef2884e46784b6239d70ca77ddbb431b835a0'
  end)

  suite.it('parses as a document, renders, and takes edits', function()
    local answers: { string } = {}
    local parsed, invalid = 0, 0

    for index, text in soup do
      local answer = outcome(document.parse, text)
      table.insert(answers, answer)
      if string.find(answer, 'invalid line', 1, true) then invalid += 1 end
      if raised(answer) then continue end

      parsed += 1
      local edited = { path = '', entries = document.parse(text) }
      table.insert(answers, document.render(edited))
      document.replaceList(edited, 'data', if index % 4 == 0 then nil else { 'first', 'second' })
      document.append(edited, 'content', { `Plugin {index}.esp` })
      table.insert(answers, canonical(edited.entries))
      table.insert(answers, ini.renderDocument(edited))
    end

    expect({ parsed, invalid }).toEqual { 529, 183 }
    expect(digest(answers)).toBe 'da19b3840d5c979516906703aeae987dbdf3915cd3a7d4bf0882ff8dc5fe5838'
  end)

  suite.it('trims', function()
    local answers = [for value in padded => outcome(parser.trim, value)]
    local refused = #[for answer in answers if raised(answer) => answer]
    expect(refused).toBe(411)
    expect(digest(answers)).toBe '89d255714122d19ebe88e347aa8e9ee2ee07ab02a72f3141e7903e442d045348'
  end)
end)

return suite
