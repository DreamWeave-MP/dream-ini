--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Rust's Unicode whitespace trimming, with an ASCII fast path. openmw-config trims with the same
-- function; this is a copy of it, because this package depends on nothing.

local function whitespace(code: number): boolean
  return code == 32 or (code >= 9 and code <= 13) or code == 133 or code == 160 or code == 5760
    or (code >= 8192 and code <= 8202) or code == 8232 or code == 8233 or code == 8239
    or code == 8287 or code == 12288
end

local function asciiSpace(code: number): boolean return code == 32 or (code >= 9 and code <= 13) end

--- Where `value`, whose ASCII whitespace is already off, begins and ends without its Unicode
--- whitespace: the first past the last when nothing is left. It is walked as UTF-8, which raises
--- on a first or last character that isn't.
local function unicodeSpan(value: string): (number, number)
  local first, last = 1, #value
  for index, code in utf8.codes(value) do
    if not whitespace(code) then
      first = index
      break
    end

    first = index + #utf8.char(code)
  end

  if first > last then return first, last end

  while last >= first do
    local index = utf8.offset(value, -1, last + 1) :: number
    if not whitespace(utf8.codepoint(value, index)) then break end

    last = index - 1
  end

  return first, last
end

local function trim(value: string): string
  local first, last = 1, #value
  while first <= last and asciiSpace(string.byte(value, first)) do first += 1 end

  while last >= first and asciiSpace(string.byte(value, last)) do last -= 1 end

  if first > last then return '' end

  local lead, tail = string.byte(value, first), string.byte(value, last)
  if lead ~= 194 and lead ~= 225 and lead ~= 226 and lead ~= 227 and tail < 128 then
    return if first == 1 and last == #value then value else string.sub(value, first, last)
  end

  value = string.sub(value, first, last)
  first, last = unicodeSpan(value)
  return string.sub(value, first, last)
end

--- Where the bytes of `value` from `first` to `last` begin and end once trimmed, the first past
--- the last when nothing is left: `string.sub(value, span(value, first, last))` is
--- `trim(string.sub(value, first, last))` without the string in between. Only a range whose ends
--- could be Unicode whitespace is cut out and walked, as `trim` walks it.
local function span(value: string, first: number, last: number): (number, number)
  while first <= last and asciiSpace(string.byte(value, first)) do first += 1 end

  while last >= first and asciiSpace(string.byte(value, last)) do last -= 1 end

  if first > last then return first, last end

  local lead, tail = string.byte(value, first), string.byte(value, last)
  if lead ~= 194 and lead ~= 225 and lead ~= 226 and lead ~= 227 and tail < 128 then
    return first, last
  end

  local inner, outer = unicodeSpan(string.sub(value, first, last))
  return first + inner - 1, first + outer - 1
end

return table.freeze { trim = trim, span = span }
