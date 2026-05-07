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
