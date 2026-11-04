use ripwire_rs::serialize::XmlEscapeExt;
use ripwire_rs::{escape_xml_str, escape_xml_write};
use std::borrow::Cow;

#[test]
fn test_escape_borrows_clean_text_and_owns_escaped_text() {
    let cases: [(&str, &str, bool); 2] = [
        ("clean_identifier_123", "clean_identifier_123", true),
        (
            "<tag name=\"foo\" key='bar' flag=\"a & b\">",
            "&lt;tag name=&quot;foo&quot; key=&apos;bar&apos; flag=&quot;a &amp; b&quot;&gt;",
            false,
        ),
    ];
    for (text, expected, borrowed) in cases {
        let escaped = escape_xml_str(text);
        assert_eq!(matches!(escaped, Cow::Borrowed(_)), borrowed, "{text}");
        assert_eq!(escaped, expected);

        let mut buf = Vec::new();
        escape_xml_write(&mut buf, text).unwrap();
        assert_eq!(std::str::from_utf8(&buf).unwrap(), expected);
    }
}

#[test]
fn test_escape_controls_and_c0() {
    let text = "line1\tline2\nline3\rline4\x01bell\x1f";
    let escaped = escape_xml_str(text);
    assert_eq!(escaped, "line1&#9;line2&#10;line3&#13;line4 bell ");
    assert_eq!(
        text.xml_escaped().to_string(),
        "line1&#9;line2&#10;line3&#13;line4 bell "
    );
}
