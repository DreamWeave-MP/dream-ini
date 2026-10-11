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
  first, last = 1, #value
  for index, code in utf8.codes(value) do
    if not whitespace(code) then
      first = index
      break
    end

    first = index + #utf8.char(code)
  end

  if first > last then return '' end

  while last >= first do
    local index = utf8.offset(value, -1, last + 1) :: number
    if not whitespace(utf8.codepoint(value, index)) then break end

    last = index - 1
  end

  return string.sub(value, first, last)
end

return table.freeze { trim = trim }
