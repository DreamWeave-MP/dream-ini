--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- An openmw.cfg kept as its lines, edited and written the way dream-ini's
-- `serialize_preserved_cfg_document` and `save_preserved_cfg_document_to_path` write one:
-- comments, blank lines, and the order of untouched lines survive; a replaced list gives up its
-- old lines' places and goes at the end.
local parser, storage = require './parser', require './storage'

--- One `key=value` line with the comment and blank lines above it, or, without a key, the lines
--- that ended the file with no setting after them.
export type Entry = { comment: string, key: string?, value: string? }

export type Document = {
  --- The file the lines came from, or would have come from.
  path: string,
  entries: { Entry },
}

local Stamp = '# OpenMW-Config Serializer Version:'

--- The lines of `text` as openmw-config's loader reads them (`str::lines`): split at `\n`, with a
--- `\r` before the `\n` dropped and a final line without a `\n` kept whole.
local function lines(text: string): { string }
  local result: { string } = {}
  local start, length = 1, #text

  while start <= length do
    local newline = string.find(text, '\n', start, true)

    if not newline then
      table.insert(result, string.sub(text, start))
      break
    end

    local finish = newline - 1
    if finish >= start and string.byte(text, finish) == 13 then
      finish -= 1
    end

    table.insert(result, string.sub(text, start, finish))
    start = newline + 1
  end

  return result
end

--- `text` split into entries the way openmw-config's loader attaches comments to settings: a
--- second `encoding=` takes the first's place, `config=` lines move behind the settings, and the
--- comments that ended the file follow them, minus a serializer stamp.
local function parse(text: string): { Entry }
  local entries: { Entry }, subConfigs: { Entry } = {}, {}
  local comment: { string } = {}
  local encodingAt: number?

  for _, line in lines(text) do
    local trimmed = parser.trim(line)

    if trimmed == '' then
      table.insert(comment, '\n')
      continue
    end

    if string.sub(trimmed, 1, 1) == '#' then
      table.insert(comment, line .. '\n')
      continue
    end

    local equals = string.find(trimmed, '=', 1, true)
      or error(`dream.ini: OpenMW config error: invalid line: {trimmed}`, 0)
    local entry: Entry = {
      comment = table.concat(comment),
      key = parser.trim(string.sub(trimmed, 1, equals - 1)),
      value = parser.trim(string.sub(trimmed, equals + 1)),
    }
    table.clear(comment)

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
  local trailing: { string } = {}

  for _, line in comment do
    if string.sub(line, 1, #Stamp) ~= Stamp then table.insert(trailing, line) end
  end

  if #trailing > 0 then table.insert(entries, { comment = table.concat(trailing) }) end

  return entries
end

local function remove(document: Document, matches: (Entry) -> boolean)
  local kept: { Entry } = {}

  for _, entry in document.entries do
    if not (entry.key and matches(entry)) then table.insert(kept, entry) end
  end

  document.entries = kept
end

local function append(document: Document, key: string, values: { string }?)
  for _, value in values or {} do
    table.insert(document.entries, { comment = '', key = key, value = value })
  end
end

--- Every line of `key` goes, and `values` follow the last line (`replace_list`).
local function replaceList(document: Document, key: string, values: { string }?)
  remove(document, function(entry) return entry.key == key end)
  append(document, key, values)
end

--- The document's text: each line with its comment, as the file would read.
local function render(document: Document): string
  local output: { string } = {}

  for _, entry in document.entries do
    table.insert(output, entry.comment)

    if entry.key then table.insert(output, `{entry.key}={entry.value}\n`) end
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
