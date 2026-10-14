--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- An openmw.cfg kept as its lines, edited and written the way dream-ini's
-- `serialize_preserved_cfg_document` and `save_preserved_cfg_document_to_path` write one:
-- comments, blank lines, and the order of untouched lines survive; a replaced list gives up its
-- old lines' places and goes at the end.
local storage, strings = require './storage', require './trim'

--- One `key=value` line with the comment and blank lines above it, or, without a key, the lines
--- that ended the file with no setting after them.
export type Entry = { comment: string, key: string?, value: string? }

export type Document = {
  --- The file the lines came from, or would have come from.
  path: string,
  entries: { Entry },
}

local Stamp = '# OpenMW-Config Serializer Version:'

local Return, Hash = 13, 35

--- `text` split into entries the way openmw-config's loader attaches comments to settings: a
--- second `encoding=` takes the first's place, `config=` lines move behind the settings, and the
--- comments that ended the file follow them, minus a serializer stamp.
---
--- The lines are the loader's (`str::lines`): split at `\n`, with a `\r` before the `\n` dropped
--- and a final line without a `\n` kept whole. Each is trimmed where it lies in the text, and so
--- are its key and its value, so only a comment, a key and a value become strings.
local function parse(text: string): { Entry }
  local entries: { Entry }, subConfigs: { Entry } = {}, {}
  local comment: { string } = {}
  local encodingAt: number?
  local start, length = 1, #text
  local nextEquals = 0

  while start <= length do
    local newline = string.find(text, '\n', start, true)
    local finish = if newline then newline - 1 else length

    if newline and finish >= start and string.byte(text, finish) == Return then
      finish -= 1
    end

    local first, last = strings.span(text, start, finish)
    local lineStart = start
    start = if newline then newline + 1 else length + 1

    if first > last then
      table.insert(comment, '\n')
      continue
    end

    if string.byte(text, first) == Hash then
      -- A line that lost no `\r` is followed by its `\n` in the text.
      table.insert(
        comment,
        if finish + 1 == newline
          then string.sub(text, lineStart, newline)
          else string.sub(text, lineStart, finish) .. '\n'
      )
      continue
    end

    if nextEquals < first then nextEquals = string.find(text, '=', first, true) or length + 1 end

    local equals = nextEquals

    if equals > last then
      error(`dream.ini: OpenMW config error: invalid line: {string.sub(text, first, last)}`, 0)
    end

    local keyStart, keyEnd = strings.span(text, first, equals - 1)
    local valueStart, valueEnd = strings.span(text, equals + 1, last)
    local above = ''

    if #comment > 0 then
      above = table.concat(comment)
      table.clear(comment)
    end

    local entry: Entry = {
      comment = above,
      key = string.sub(text, keyStart, keyEnd),
      value = string.sub(text, valueStart, valueEnd),
    }

    if entry.key == 'config' then
      table.insert(subConfigs, entry)
    elseif entry.key == 'encoding' and encodingAt then
      entries[encodingAt] = entry
    else
      table.insert(entries, entry)
      if entry.key == 'encoding' then encodingAt = #entries end
    end
  end

  table.move(subConfigs, 1, #subConfigs, #entries + 1, entries)
  local trailing = [for line in comment if string.sub(line, 1, #Stamp) ~= Stamp => line]

  if #trailing > 0 then table.insert(entries, { comment = table.concat(trailing) }) end

  return entries
end

local function append(document: Document, key: string, values: { string }?)
  for _, value in values or {} do
    table.insert(document.entries, { comment = '', key = key, value = value })
  end
end

--- Every line of `key` goes, and `values` follow the last line (`replace_list`).
local function replaceList(document: Document, key: string, values: { string }?)
  local kept: { Entry } = {}

  -- The entries are a caller's, who may have set one to nil: the walk passes over a gap, where
  -- counting to the length would stop at one or index it.
  for _, entry in document.entries do
    if not (entry.key and entry.key == key) then table.insert(kept, entry) end
  end

  document.entries = kept
  append(document, key, values)
end

--- The document's text: each line with its comment, as the file would read.
local function render(document: Document): string
  local entries = document.entries
  local output = table.create(5 * #entries) :: { string }

  for _, entry in entries do
    local { comment, key, value } = entry
    table.insert(output, comment)
    if not key then continue end

    -- A key and a value that are strings join the output as they are. Anything else a caller
    -- left there, a missing value included, is spelled the way interpolation spells it.
    if type(key) == 'string' and type(value) == 'string' then
      table.insert(output, key)
      table.insert(output, '=')
      table.insert(output, value)
      table.insert(output, '\n')
    else
      table.insert(output, `{key}={value}\n`)
    end
  end

  return table.concat(output)
end

--- `text` written over `path` through a temporary file beside it and a rename, so a failure
--- leaves the old file whole, with dream-ini's error wording.
local function replaceFile(path: string, text: string)
  -- `replace` throws when the folder can't take a temporary file or the rename fails.
  local ok, problem = pcall(jess.fs.replace, path, text)
  if ok then return end

  local message = tostring(problem)
  error(
    `dream.ini: {path}: {string.match(message, '^failed to write \'.-\': (.*)$') or message}`,
    0
  )
end

--- The document written to `path`, resolved through symbolic links first so a linked openmw.cfg's
--- target is what's replaced (`save_preserved_cfg_document_to_path`).
local function save(document: Document, path: string)
  replaceFile(storage.canonical(path), render(document))
end

return table.freeze {
  render = render,
  save = save,
  replaceFile = replaceFile,
  parse = parse,
  append = append,
  replaceList = replaceList,
}
