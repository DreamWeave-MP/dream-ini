--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0

local types, options = require './types', require './options'
type MultiMap = types.MultiMap
type Warning = types.Warning
local bytes = require '@dream/bytes'
local strings = require './trim'

local Labels: { [string]: string } = {
  win1250 = 'win1250',
  ['windows-1250'] = 'win1250',
  win1251 = 'win1251',
  ['windows-1251'] = 'win1251',
  win1252 = 'win1252',
  ['windows-1252'] = 'win1252',
}
table.freeze(Labels)

--- The canonical label (`win1252`) of an OpenMW encoding label, ASCII case-insensitively, or nil
--- for one dream-ini doesn't support.
local function encodingLabel(label: string): string? return Labels[string.lower(label)] end

local Codepages: { [string]: string } =
  { win1250 = 'windows-1250', win1251 = 'windows-1251', win1252 = 'windows-1252' }
table.freeze(Codepages)

--- The INI's bytes as text: a byte order mark picks UTF-8 or UTF-16, as encoding_rs's `decode`
--- does whatever codepage was asked for; otherwise the codepage.
local function decode(source: string | buffer, label: string?): string
  local canonical = if label then encodingLabel(label) else 'win1252'
  assert(canonical, `dream.ini: unsupported encoding: {label}`)
  local text = if type(source) == 'buffer' then buffer.tostring(source) else source
  local first, second, third = string.byte(text, 1, 3)

  if first == 239 and second == 187 and third == 191 then
    return bytes.decode(string.sub(text, 4), 'utf-8')
  end

  if first == 255 and second == 254 then return bytes.decode(string.sub(text, 3), 'utf-16le') end
  if first == 254 and second == 255 then return bytes.decode(string.sub(text, 3), 'utf-16be') end

  return bytes.decode(text, Codepages[canonical])
end

local function push(map: MultiMap, key: string, value: string)
  local values = map[key]

  if values then
    table.insert(values, value)
  else
    map[key] = { value }
  end
end

local Return, Open = 13, 91

--- The INI's `Section:Key` entries and warnings, walked by position: every byte is visited once,
--- through plain (memchr) finds whose results are kept until the line passes them, and only the
--- keys, values, and section names become strings.
local function parseText(text: string): { entries: MultiMap, warnings: { Warning } }
  local entries: MultiMap = {}
  local warnings: { Warning } = {}
  local section, start, length = '', 1, #text
  local nextEquals, nextSemicolon = 0, 0

  while start <= length do
    local newline = string.find(text, '\n', start, true) or length + 1
    local finish = newline - 1

    if finish >= start and string.byte(text, finish) == Return then
      finish -= 1
    end

    if finish < start then
      start = newline + 1
      continue
    end

    if string.byte(text, start) == Open then
      local closing = string.find(text, ']', start + 1, true)

      if closing and closing <= finish and closing - start >= 2 then
        section = string.sub(text, start + 1, closing - 1)
      else
        local line = string.sub(text, start, finish)
        table.insert(warnings, {
          kind = 'malformedIniLine',
          line = line,
          message = `ini file wrongly formatted ({line}). Line ignored.`,
        })
      end

      start = newline + 1
      continue
    end

    if nextSemicolon < start then
      nextSemicolon = string.find(text, ';', start, true) or length + 1
    end

    if nextSemicolon <= finish then finish = nextSemicolon - 1 end

    if nextEquals < start then nextEquals = string.find(text, '=', start, true) or length + 1 end

    local equals = nextEquals

    if equals <= finish and equals > start then
      local key = section .. ':' .. string.sub(text, start, equals - 1)

      if equals == finish then
        table.insert(warnings, {
          kind = 'ignoredEmptyValue',
          key = key,
          message = `ignored empty value for key '{key}'.`,
        })
      else
        push(entries, key, string.sub(text, equals + 1, finish))
      end
    end

    start = newline + 1
  end

  return { entries = entries, warnings = warnings }
end

local ParseSpec: options.Spec = { encoding = 'string' }
table.freeze(ParseSpec)

local function parseIni(source: string | buffer, given: { encoding: string? }?)
  local read = options.read('ini.parseIni', given, ParseSpec)
  local label = read.encoding :: string?

  if label and not encodingLabel(label) then
    error(`ini.parseIni.encoding: unsupported encoding: {label}`, 0)
  end

  return parseText(decode(source, label))
end

local Space, Tab, Hash = 32, 9, 35

--- Whether the cfg line at `start` is a comment: a `#` after spaces, tabs, or returns, as
--- dream-ini's `first_non_ws` looks for it; other whitespace before a `#` makes it part of the
--- key.
local function commented(text: string, start: number): boolean
  local code = string.byte(text, start)

  while code == Space or code == Tab or code == Return do
    start += 1
    code = string.byte(text, start)
  end

  return code == Hash
end

--- openmw.cfg text as a multimap: `key = value` lines, trimmed, `#` comments skipped. A key and
--- a value are trimmed where they lie in the text, so each becomes a string once.
local function parseCfg(text: string): MultiMap
  local map: MultiMap = {}
  local start, length = 1, #text
  local nextEquals = 0

  while start <= length do
    local newline = string.find(text, '\n', start, true) or length + 1
    local finish = newline - 1

    if finish >= start and string.byte(text, finish) == Return then
      finish -= 1
    end

    if nextEquals < start then nextEquals = string.find(text, '=', start, true) or length + 1 end

    local equals = nextEquals

    if equals <= finish and equals > start and not commented(text, start) then
      local keyStart, keyEnd = strings.span(text, start, equals - 1)
      local valueStart, valueEnd = strings.span(text, equals + 1, finish)
      push(map, string.sub(text, keyStart, keyEnd), string.sub(text, valueStart, valueEnd))
    end

    start = newline + 1
  end

  return map
end

local function keys(map: MultiMap): { string }
  local result: { string } = {}

  for key in map do
    table.insert(result, key)
  end

  table.sort(result)
  return result
end

local function serializeCfg(map: MultiMap): string
  map = options.multimap('ini.serializeCfg', map)
  local sorted = keys(map)
  local output: { string } = table.create(4 * sum[for key in sorted => #map[key]])
  local count = 0

  for _, key in sorted do
    for _, value in map[key] do
      output[count + 1], output[count + 2], output[count + 3], output[count + 4] =
        key, '=', value, '\n'
      count += 4
    end
  end

  return table.concat(output)
end

return table.freeze {
  parseIni = parseIni,
  parseText = parseText,
  parseCfg = parseCfg,
  serializeCfg = serializeCfg,
  decode = decode,
  encodingLabel = encodingLabel,
  trim = strings.trim,
  keys = keys,
}
