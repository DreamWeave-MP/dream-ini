// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{ImportWarning, MultiMap, TextEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedIni {
    pub entries: MultiMap,
    pub warnings: Vec<ImportWarning>,
}

#[must_use]
pub fn parse_ini_bytes(bytes: &[u8], encoding: TextEncoding) -> MultiMap {
    parse_ini_bytes_with_warnings(bytes, encoding).entries
}

#[must_use]
pub fn parse_ini_bytes_with_warnings(bytes: &[u8], encoding: TextEncoding) -> ParsedIni {
    let (decoded, _, _) = encoding.encoding_rs().decode(bytes);
    parse_ini_str_with_warnings(&decoded)
}

#[must_use]
pub fn parse_ini_str(text: &str) -> MultiMap {
    parse_ini_str_with_warnings(text).entries
}

#[must_use]
pub fn parse_ini_str_with_warnings(text: &str) -> ParsedIni {
    let mut section = String::new();
    let mut map = MultiMap::new();
    let mut warnings = Vec::new();
    // `Section:Key`, rebuilt in place per line; the map copies it only for a new key.
    let mut key = String::new();

    // Lines end at '\n' and lose exactly one trailing '\r', as the C++ importer's `getline`
    // loop does; `str::lines` would also strip a second one from `\r\r\n`.
    for raw_line in text.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);

        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            match line.find(']') {
                Some(end) if end >= 2 => line[1..end].clone_into(&mut section),
                _ => warnings.push(ImportWarning::MalformedIniLine {
                    line: line.to_owned(),
                }),
            }
            continue;
        }

        let line = line.find(';').map_or(line, |comment| &line[..comment]);

        let Some(equals) = line.find('=') else {
            continue;
        };
        if equals < 1 {
            continue;
        }

        key.clear();
        key.push_str(&section);
        key.push(':');
        key.push_str(&line[..equals]);
        let value = &line[equals + 1..];
        if value.is_empty() {
            warnings.push(ImportWarning::IgnoredEmptyValue { key: key.clone() });
            continue;
        }
        push_value(&mut map, &key, value);
    }

    ParsedIni {
        entries: map,
        warnings,
    }
}

#[must_use]
pub fn parse_cfg_str(text: &str) -> MultiMap {
    let mut map = MultiMap::new();

    for line in text.lines() {
        if line.find('#') == first_non_ws(line) {
            continue;
        }
        if line.is_empty() {
            continue;
        }

        let Some(equals) = line.find('=') else {
            continue;
        };
        if equals < 1 {
            continue;
        }

        push_value(&mut map, line[..equals].trim(), line[equals + 1..].trim());
    }

    map
}

#[must_use]
pub fn serialize_cfg(cfg: &MultiMap) -> String {
    let length = cfg
        .iter()
        .map(|(key, values)| {
            values
                .iter()
                .map(|value| key.len() + value.len() + 2)
                .sum::<usize>()
        })
        .sum();
    let mut output = String::with_capacity(length);
    for (key, values) in cfg {
        for value in values {
            output.push_str(key);
            output.push('=');
            output.push_str(value);
            output.push('\n');
        }
    }
    output
}

/// Appends `value` under `key`, copying the key only when it is new to the map.
fn push_value(map: &mut MultiMap, key: &str, value: &str) {
    if let Some(values) = map.get_mut(key) {
        values.push(value.to_owned());
    } else {
        map.insert(key.to_owned(), vec![value.to_owned()]);
    }
}

pub(crate) fn insert_multimap(map: &mut MultiMap, key: String, value: String) {
    map.entry(key).or_default().push(value);
}

pub(crate) fn set_single_value(map: &mut MultiMap, key: &str, value: String) {
    map.insert(key.to_owned(), vec![value]);
}

fn first_non_ws(value: &str) -> Option<usize> {
    value
        .char_indices()
        .find_map(|(index, ch)| (!matches!(ch, ' ' | '\t' | '\r' | '\n')).then_some(index))
}
