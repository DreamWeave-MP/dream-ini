// SPDX-License-Identifier: GPL-3.0-only

use crate::test_support::values;
use crate::{
    ImportWarning, TextEncoding, parse_cfg_str, parse_ini_bytes, parse_ini_str,
    parse_ini_str_with_warnings,
};

#[test]
fn parses_ini_sections_comments_duplicates_and_equals() {
    let parsed = parse_ini_str(
        "[General]\nDisable Audio=1 ; comment\nName=a=b\nName=c\n=ignored\nEmpty=\n[bad\nignored\n",
    );

    assert_eq!(values(&parsed, "General:Disable Audio"), &["1 ".to_owned()]);
    assert_eq!(
        values(&parsed, "General:Name"),
        &["a=b".to_owned(), "c".to_owned()]
    );
    assert!(!parsed.contains_key("General:Empty"));
}

#[test]
fn surfaces_ini_parse_warnings() {
    let parsed = parse_ini_str_with_warnings("[General]\nEmpty=\n[bad\n[]=ignored\n");

    assert_eq!(
        parsed.warnings,
        vec![
            ImportWarning::IgnoredEmptyValue {
                key: "General:Empty".to_owned()
            },
            ImportWarning::MalformedIniLine {
                line: "[bad".to_owned()
            },
            ImportWarning::MalformedIniLine {
                line: "[]=ignored".to_owned()
            },
        ]
    );
}

#[test]
fn parses_ini_keys_before_section_like_cpp_importer() {
    let parsed = parse_ini_str("Loose=value\n");
    assert_eq!(values(&parsed, ":Loose"), &["value".to_owned()]);
}

#[test]
fn parses_ini_crlf_lines() {
    let parsed =
        parse_ini_str("[General]\r\nDisable Audio=1\r\n[Movies]\r\nNew Game=intro.bik\r\n");

    assert_eq!(values(&parsed, "General:Disable Audio"), &["1".to_owned()]);
    assert_eq!(
        values(&parsed, "Movies:New Game"),
        &["intro.bik".to_owned()]
    );
}

#[test]
fn old_mac_carriage_returns_are_not_ini_line_breaks() {
    let parsed = parse_ini_str("[General]\rDisable Audio=1\r[Movies]\rNew Game=intro.bik\r");

    assert_eq!(values(&parsed, "General:Disable Audio"), &[] as &[String]);
    assert_eq!(values(&parsed, "Movies:New Game"), &[] as &[String]);
}

#[test]
fn utf8_bom_is_stripped_from_win1252_ini_input() {
    let parsed = parse_ini_bytes(
        b"\xef\xbb\xbf[General]\nDisable Audio=1\n",
        TextEncoding::Win1252,
    );

    assert_eq!(values(&parsed, "General:Disable Audio"), &["1".to_owned()]);
    assert_eq!(values(&parsed, ":Disable Audio"), &[] as &[String]);
}

#[test]
fn repeated_ini_sections_accumulate_values_in_file_order() {
    let parsed = parse_ini_str(
        "[Movies]\nNew Game=intro.bik\n[Weather]\nSunrise Time=6\n[Movies]\nNew Game=outro.bik\n",
    );

    assert_eq!(
        values(&parsed, "Movies:New Game"),
        &["intro.bik".to_owned(), "outro.bik".to_owned()]
    );
    assert_eq!(values(&parsed, "Weather:Sunrise Time"), &["6".to_owned()]);
}

#[test]
fn ini_whitespace_around_sections_and_keys_is_significant() {
    let parsed = parse_ini_str(
        " [General]\nDisable Audio=1\n[General]\n Disable Audio=2\nDisable Audio =3\n",
    );

    assert_eq!(values(&parsed, ":Disable Audio"), &["1".to_owned()]);
    assert_eq!(values(&parsed, "General:Disable Audio"), &[] as &[String]);
    assert_eq!(values(&parsed, "General: Disable Audio"), &["2".to_owned()]);
    assert_eq!(values(&parsed, "General:Disable Audio "), &["3".to_owned()]);
}

#[test]
fn inline_semicolon_truncates_ini_values() {
    let parsed = parse_ini_str("[Movies]\nNew Game=intro;not-part-of-filename.bik\n");

    assert_eq!(values(&parsed, "Movies:New Game"), &["intro".to_owned()]);
}

#[test]
fn ini_values_preserve_nul_and_control_bytes() {
    let parsed = parse_ini_str("[Movies]\nNew Game=intro\0\u{1f}.bik\n");

    assert_eq!(
        values(&parsed, "Movies:New Game"),
        &["intro\0\u{1f}.bik".to_owned()]
    );
}

#[test]
fn malformed_ini_section_does_not_poison_following_valid_section() {
    let parsed = parse_ini_str_with_warnings("[bad\nignored\n[General]\nDisable Audio=1\n");

    assert_eq!(
        parsed.warnings,
        vec![ImportWarning::MalformedIniLine {
            line: "[bad".to_owned()
        }]
    );
    assert_eq!(
        values(&parsed.entries, "General:Disable Audio"),
        &["1".to_owned()]
    );
}

#[test]
fn parses_cfg_trims_and_preserves_inline_hash() {
    let parsed = parse_cfg_str(" # comment\nkey = value # not comment\nkey= second\ninvalid\n");
    assert_eq!(
        values(&parsed, "key"),
        &["value # not comment".to_owned(), "second".to_owned()]
    );
}

#[test]
fn decodes_ini_with_selected_codepage() {
    let parsed = parse_ini_bytes(b"[Movies]\nNew Game=caf\xe9.bik\n", TextEncoding::Win1252);
    assert_eq!(
        values(&parsed, "Movies:New Game"),
        &["caf\u{e9}.bik".to_owned()]
    );
}
