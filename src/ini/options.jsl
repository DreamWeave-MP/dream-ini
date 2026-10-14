--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Option tables and multimaps, checked the way the native binding checks them, with its messages:
-- an unknown key names the known ones, a wrong type names the field and what it got.
local types = require './types'
type MultiMap = types.MultiMap

--- What an option holds: a string, a boolean, or an array of strings.
export type Kind = 'string' | 'boolean' | 'paths'
export type Spec = { [string]: Kind }

local function unknown(context: string, given: { [string]: unknown }, spec: Spec)
  local unknownKeys: { string } = {}

  for key in given do
    if not spec[key] then table.insert(unknownKeys, `'{key}'`) end
  end

  if #unknownKeys == 0 then return end

  local known: { string } = {}

  for key in spec do
    table.insert(known, key)
  end

  table.sort(unknownKeys)
  table.sort(known)
  error(
    `{context}: unknown option{if #unknownKeys == 1 then '' else 's'} {table.concat(
      unknownKeys,
      ', '
    )}; known options are {table.concat(known, ', ')}`,
    0
  )
end

--- `given` as a table of the options `spec` names, each of its kind; `required` must be there.
--- `nil` values are absent. Values are checked before unknown keys are reported, as the native
--- reader does.
local function read(
  context: string,
  given: unknown,
  spec: Spec,
  required: { string }?
): { [string]: unknown }
  if given == nil and not required then return {} end

  if type(given) ~= 'table' then
    error(`{context}: options must be a table, got {typeof(given)}`, 0)
  end

  local options = given :: { [unknown]: unknown }
  local result: { [string]: unknown } = {}

  for key, value in options do
    if type(key) ~= 'string' then
      error(`{context}: option keys must be strings, got {typeof(key)}`, 0)
    end

    result[key] = value
  end

  for _, key in required or {} do
    if result[key] == nil then error(`{context}: missing required option '{key}'`, 0) end
  end

  for key, kind in spec do
    local value = result[key]
    if value == nil then continue end

    if kind == 'paths' then
      if type(value) ~= 'table' then
        error(`{context}.{key}: expected an array of strings, got {typeof(value)}`, 0)
      end

      local paths = value :: { unknown }

      for index = 1, #paths do
        if type(paths[index]) ~= 'string' then
          error(`{context}.{key}[{index}]: expected a string, got {typeof(paths[index])}`, 0)
        end
      end
    elseif type(value) ~= kind then
      error(`{context}.{key}: expected {kind}, got {typeof(value)}`, 0)
    end
  end

  unknown(context, result, spec)
  return result
end

--- `value` checked to be a multimap: string keys, each an array of strings.
local function multimap(context: string, value: unknown): MultiMap
  if type(value) ~= 'table' then
    error(`{context}: expected a multimap table, got {typeof(value)}`, 0)
  end

  for key, values in value :: { [unknown]: unknown } do
    if type(key) ~= 'string' then
      error(`{context}: multimap keys must be strings, got {typeof(key)}`, 0)
    end

    if type(values) ~= 'table' then
      error(`{context}: expected array of strings for key '{key}', got {typeof(values)}`, 0)
    end

    local entries = values :: { unknown }

    for index = 1, #entries do
      if type(entries[index]) ~= 'string' then
        error(
          `{context}: expected a string at key '{key}' index {index}, got {typeof(entries[index])}`,
          0
        )
      end
    end
  end

  return value :: MultiMap
end

return table.freeze { read = read, multimap = multimap }
