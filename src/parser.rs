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

    for raw_line in text.lines() {
        let mut line = raw_line;
        if let Some(stripped) = line.strip_suffix('\r') {
            line = stripped;
        }

        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') {
            let Some(end) = line.find(']') else {
                warnings.push(ImportWarning::MalformedIniLine {
                    line: line.to_owned(),
                });
                continue;
            };
            if end < 2 {
                warnings.push(ImportWarning::MalformedIniLine {
                    line: line.to_owned(),
                });
                continue;
            }
            line[1..end].clone_into(&mut section);
            continue;
        }

        if let Some(comment) = line.find(';') {
            line = &line[..comment];
        }

        let Some(equals) = line.find('=') else {
            continue;
        };
        if equals < 1 {
            continue;
        }

        let key = format!("{}:{}", section, &line[..equals]);
        let value = &line[equals + 1..];
        if value.is_empty() {
            warnings.push(ImportWarning::IgnoredEmptyValue { key });
            continue;
        }
        insert_multimap(&mut map, key, value.to_owned());
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

        let key = line[..equals].trim().to_owned();
        let value = line[equals + 1..].trim().to_owned();
        insert_multimap(&mut map, key, value);
    }

    map
}

#[must_use]
pub fn serialize_cfg(cfg: &MultiMap) -> String {
    let mut output = String::new();
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
