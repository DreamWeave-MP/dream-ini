--!strict
--!native
-- SPDX-License-Identifier: MIT OR Apache-2.0
local parser, document = require '@self/ini/parser', require '@self/ini/document'
local types = require '@self/ini/types'

export type MultiMap = types.MultiMap
export type Warning = types.Warning

--- The dream-ini release this port follows, to the byte.
local Version = '1.0.1'

return table.freeze {
  version = Version,
  parseIni = parser.parseIni,
  parseCfg = parser.parseCfg,
  serializeCfg = parser.serializeCfg,
  renderDocument = document.render,
  saveDocument = document.save,
}
