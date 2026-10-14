--!strict
--!native
-- The package's own cases: the edge semantics dream-ini pins, checked on the port alone.
local ini = require '@dream-ini'
local test = require '@jess/test'
local expect = test.expect
local suite = test.new()

type ParserCase = { name: string, text: string, entries: { [string]: { string } } }

local ParserCases: { ParserCase } = {
  {
    name = 'duplicates and equals signs in values',
    text = '[General]\nName=a=b\nName=c\n',
    entries = { ['General:Name'] = { 'a=b', 'c' } },
  },

  {
    name = 'a key before any section',
    text = 'Loose=value\n',
    entries = { [':Loose'] = { 'value' } },
  },
  { name = 'CRLF', text = '[General]\r\nName=a\r\n', entries = { ['General:Name'] = { 'a' } } },
  {
    name = 'exactly one trailing CR comes off',
    text = '[General]\r\nName=a\r\r\nLast=b\r',
    entries = { ['General:Name'] = { 'a\r' }, ['General:Last'] = { 'b' } },
  },

  {
    name = 'CR alone is not a line end',
    text = '[General]\rName=a\r[Movies]\rNew Game=intro.bik\r',
    entries = {},
  },

  {
    name = 'a UTF-8 byte order mark',
    text = '\239\187\191[General]\nName=a\n',
    entries = { ['General:Name'] = { 'a' } },
  },

  {
    name = 'repeated sections append',
    text = '[Movies]\nNew Game=a\n[Movies]\nNew Game=b\n',
    entries = { ['Movies:New Game'] = { 'a', 'b' } },
  },

  {
    name = 'whitespace is significant',
    text = ' [General]\nName=a\n[General]\n Name=b\nName =c\n',
    entries = { [':Name'] = { 'a' }, ['General: Name'] = { 'b' }, ['General:Name '] = { 'c' } },
  },

  {
    name = 'a semicolon starts a comment',
    text = '[Movies]\nNew Game=intro;rest\n',
    entries = { ['Movies:New Game'] = { 'intro' } },
  },

  {
    name = 'control bytes survive',
    text = '[Movies]\nNew Game=intro\0\31.bik\n',
    entries = { ['Movies:New Game'] = { 'intro\0\31.bik' } },
  },

  {
    name = 'a malformed section line is skipped',
    text = '[bad\n[General]\nName=a\n',
    entries = { ['General:Name'] = { 'a' } },
  },

  {
    name = 'windows-1252 by default',
    text = '[Movies]\nNew Game=caf\233.bik\n',
    entries = { ['Movies:New Game'] = { 'café.bik' } },
  },

  {
    name = 'windows-1251 on request',
    text = '[Movies]\nNew Game=\192.bik\n',
    entries = { ['Movies:New Game'] = { 'А.bik' } },
  },
}

suite.describe('parseIni', function()
  for _, case in ParserCases do
    suite.it(case.name, function()
      local encoding = if string.find(case.name, '1251') then 'win1251' else nil
      expect(ini.parseIni(case.text, { encoding = encoding }).entries).toEqual(case.entries)
    end)
  end

  suite.it('reports warnings in input order', function()
    local warnings = ini.parseIni('[General]\nEmpty=\n[bad\n[]=ignored\n').warnings
    expect(warnings).toEqual {
      {
        kind = 'ignoredEmptyValue',
        key = 'General:Empty',
        message = 'ignored empty value for key \'General:Empty\'.',
      },

      {
        kind = 'malformedIniLine',
        line = '[bad',
        message = 'ini file wrongly formatted ([bad). Line ignored.',
      },

      {
        kind = 'malformedIniLine',
        line = '[]=ignored',
        message = 'ini file wrongly formatted ([]=ignored). Line ignored.',
      },
    }
  end)

  suite.it('rejects other encodings and unknown options', function()
    expect(function() ini.parseIni('', { encoding = 'utf-8' }) end).toThrow 'unsupported encoding'
    expect(function() ini.parseIni('', { encodng = 'win1252' } :: any) end).toThrow 'unknown option'
  end)
end)

suite.describe('parseCfg and serializeCfg', function()
  suite.it(
    'trims and keeps an inline hash',
    function()
      expect(ini.parseCfg ' # comment\nkey = value # not comment\nkey= second\ninvalid\n').toEqual {
        key = { 'value # not comment', 'second' },
      }
    end
  )

  suite.it(
    'serializes keys sorted and values in order',
    function()
      expect(ini.serializeCfg { data = { 'b', 'a' }, content = { 'x' } }).toBe 'content=x\ndata=b\ndata=a\n'
    end
  )
end)

return suite
