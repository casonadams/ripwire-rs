use ripwire_rs::serialize::{MinifyingWriter, XmlWriter, serialize_legend_dict_xml};
use std::io::Write;

#[test]
fn test_minifying_writer_strips_inter_tag_whitespace() {
    let input = "<!-- comment -->\n<root a=\"1\">\n  <child b=\"2\"/>\n  <child c=\"3\">hello</child>\n</root>\n";
    let mut out = Vec::new();
    {
        let mut writer = MinifyingWriter::new(&mut out);
        writer.write_all(input.as_bytes()).unwrap();
    }
    let res = String::from_utf8(out).unwrap();
    assert_eq!(
        res,
        "<!-- comment --><root a=\"1\"><child b=\"2\"/><child c=\"3\">hello</child></root>\n"
    );
}

#[test]
fn test_xml_writer_json_conversion() {
    let input = "<!-- comment -->\n<r schema=\"ripwire-rs.map/v1\" files=\"2\"><f p=\"src/lib.rs\"><s t=\"fn\" n=\"run\"/></f></r>\n";
    let mut out = Vec::new();
    {
        let mut writer = XmlWriter::with_format(&mut out, false, true);
        writer.write_all(input.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
    let val: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(val["schema"], "ripwire-rs.map/v1");
    assert_eq!(val["files"], 2);
    assert!(!val["items"].as_array().unwrap().is_empty());
}

#[test]
fn test_xml_writer_strip_comments_and_legend_dict() {
    let input = "<!-- ripwire-rs map schema=ripwire-rs.map/v1 -->\n<r schema=\"ripwire-rs.map/v1\"><f p=\"src/lib.rs\"/></r>\n";
    let mut out = Vec::new();
    {
        let mut writer = XmlWriter::with_options(&mut out, false, false, true);
        writer.write_all(input.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
    let res = String::from_utf8(out).unwrap();
    assert!(!res.contains("<!--"));
    assert!(res.starts_with("<r schema=\"ripwire-rs.map/v1\">"));

    let mut dict_buf = Vec::new();
    serialize_legend_dict_xml(&mut dict_buf).unwrap();
    let dict_str = String::from_utf8(dict_buf).unwrap();
    assert!(dict_str.contains("schema=\"ripwire-rs.legend-dict/v1\""));
    assert!(dict_str.contains("k=\"ripwire-rs.map/v1\""));
    assert!(dict_str.contains("k=\"ccx\""));
    assert!(dict_str.contains("k=\"prov\""));
}

fn json_report(input: &[u8]) -> serde_json::Value {
    let mut output = Vec::new();
    let mut writer = XmlWriter::with_format(&mut output, false, true);
    writer.write_all(input).unwrap();
    writer.finish().unwrap();
    drop(writer);
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn navigation_definitions_keep_their_own_nested_rows() {
    let value = json_report(
        br#"<r><definition name="run" path="a.rs"><call name="first" line="12"/><call name="second" line="14"/></definition><definition name="run" path="b.rs"><call name="other" line="27"/></definition></r>"#,
    );
    assert_eq!(value["items"][0]["path"], "a.rs");
    assert_eq!(value["items"][0]["items"][0]["name"], "first");
    assert_eq!(value["items"][0]["items"][1]["line"], 14);
    assert_eq!(value["items"][1]["path"], "b.rs");
    assert_eq!(value["items"][1]["items"][0]["name"], "other");
    assert_eq!(value["items"][1]["items"].as_array().unwrap().len(), 1);
}

#[test]
fn cdata_source_survives_json_comment_stripping_and_chunked_minification() {
    let source = "fn body() {\n  let s = \"<!-- keep --> <tag> &amp;\";\n  a > b;\n  <tag>\n}\n";
    let input = format!("<!-- remove -->\n<r><body><![CDATA[{source}]]></body></r>");
    let value = json_report(input.as_bytes());
    assert_eq!(value["items"][0]["text"], source);
    for strip_comments in [false, true] {
        let mut output = Vec::new();
        {
            let mut writer = XmlWriter::with_options(&mut output, false, false, strip_comments);
            for byte in input.as_bytes() {
                writer.write_all(&[*byte]).unwrap();
            }
            writer.finish().unwrap();
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains(&format!("<![CDATA[{source}]]>")));
        assert_eq!(output.contains("<!-- remove -->"), !strip_comments);
    }
}

#[test]
fn escaped_attributes_and_mixed_text_keep_decoded_values_and_order() {
    let value = json_report(
        br#"<r path='a&gt;b&amp;c&quot;&apos;&#x3c;' code="001" tag="original">before &lt; &amp; &#65;<child/> after &gt;</r>"#,
    );
    assert_eq!(value["path"], "a>b&c\"'<");
    assert_eq!(value["code"], "001");
    assert_eq!(value["attributes"]["tag"], "original");
    assert_eq!(value["items"][0], "before < & A");
    assert_eq!(value["items"][1]["tag"], "child");
    assert_eq!(value["items"][2], " after >");
}

#[test]
fn root_only_documents_and_text_are_preserved() {
    let empty = json_report(br#"<report status="ok"/>"#);
    assert_eq!(empty["tag"], "report");
    assert_eq!(empty["status"], "ok");
    assert_eq!(
        json_report(b"<report>answer &amp; detail</report>")["text"],
        "answer & detail"
    );
}

#[test]
fn malformed_reports_return_invalid_data_without_emitting_json() {
    for input in [
        b"<r><child></r>".as_slice(),
        b"<r/><another/>",
        b"<r a='one' a='two'/>",
        b"<r>&unknown;</r>",
        b"<r><![CDATA[unfinished</r>",
        b"<r>\xff</r>",
    ] {
        let mut output = Vec::new();
        let mut writer = XmlWriter::with_format(&mut output, false, true);
        writer.write_all(input).unwrap();
        writer.flush().unwrap();
        assert_eq!(
            writer.finish().unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        drop(writer);
        assert!(output.is_empty());
    }
}

#[test]
fn valid_json_values_pass_through_without_xml_interpretation() {
    for input in [
        r#"{"body":"<!-- keep --> <tag>","rows":[{"line":42}]}"#,
        r#"[{"name":"first"},{"name":"second"}]"#,
        r#""<tag>""#,
        "true",
    ] {
        let expected: serde_json::Value = serde_json::from_str(input).unwrap();
        assert_eq!(json_report(input.as_bytes()), expected);
    }
}

#[test]
fn minification_keeps_quoted_delimiters_and_entity_text() {
    let input = b"<r value='>  <tag>'>\n <body>&lt;tag&gt; &amp; data</body>\n</r>";
    let mut output = Vec::new();
    {
        let mut writer = MinifyingWriter::new(&mut output);
        for chunk in input.chunks(2) {
            writer.write_all(chunk).unwrap();
        }
        writer.flush().unwrap();
    }
    assert_eq!(
        output,
        b"<r value='>  <tag>'><body>&lt;tag&gt; &amp; data</body></r>"
    );
}

#[test]
fn buffered_wrappers_finish_only_after_the_closing_tag() {
    for json in [false, true] {
        let mut output = Vec::new();
        {
            let mut writer = XmlWriter::with_options(&mut output, false, json, true);
            writer.write_all(b"<ctx><!-- remove --><r><body><![CDATA[<!-- literal --> & <example>]]></body></r>").unwrap();
            writer.flush().unwrap();
            writer.flush().unwrap();
            writer.write_all(b"</ctx>").unwrap();
            writer.finish().unwrap();
            writer.finish().unwrap();
        }
        if json {
            let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(value["tag"], "ctx");
            assert_eq!(value["items"][0]["tag"], "r");
            assert_eq!(
                value["items"][0]["items"][0]["text"],
                "<!-- literal --> & <example>"
            );
        } else {
            assert_eq!(
                output,
                b"<ctx><r><body><![CDATA[<!-- literal --> & <example>]]></body></r></ctx>"
            );
        }
    }
}

#[test]
fn legend_none_leaves_plain_recall_and_json_bytes_unchanged() {
    for pretty in [false, true] {
        for input in [
            "  Recall: A & B\nExamples: <r><child/></r>, <!-- literal -->\n",
            "{ \"body\": \"<!-- keep --> <tag> &\", \"rows\": [] }\n",
        ] {
            let mut output = Vec::new();
            {
                let mut writer = XmlWriter::with_options(&mut output, pretty, false, true);
                writer.write_all(input.as_bytes()).unwrap();
                writer.flush().unwrap();
                writer.finish().unwrap();
            }
            assert_eq!(output, input.as_bytes());
        }
    }
}

#[test]
fn malformed_xml_in_legend_none_errors_only_at_finish() {
    let mut output = Vec::new();
    {
        let mut writer = XmlWriter::with_options(&mut output, false, false, true);
        writer.write_all(b"<ctx><r></ctx>").unwrap();
        writer.flush().unwrap();
        assert_eq!(
            writer.finish().unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }
    assert!(output.is_empty());
}

#[test]
fn buffered_finish_drains_each_report_once() {
    for json in [false, true] {
        let mut output = Vec::new();
        {
            let mut writer = XmlWriter::with_options(&mut output, false, json, true);
            writer.write_all(b"<first/>").unwrap();
            writer.finish().unwrap();
            writer.flush().unwrap();
            writer.finish().unwrap();
            writer.write_all(b"<second/>").unwrap();
            writer.flush().unwrap();
            writer.finish().unwrap();
        }
        if json {
            let reports: Vec<serde_json::Value> = serde_json::Deserializer::from_slice(&output)
                .into_iter()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(reports.len(), 2);
            assert_eq!(reports[0]["tag"], "first");
            assert_eq!(reports[1]["tag"], "second");
        } else {
            assert_eq!(output, b"<first/><second/>");
        }
    }
}

#[test]
fn streaming_xml_is_visible_before_finish_and_can_continue_after_flush() {
    for pretty in [false, true] {
        let mut output = Vec::new();
        {
            let mut writer = XmlWriter::new(&mut output, pretty);
            writer.write_all(b"<ctx>").unwrap();
            writer.flush().unwrap();
        }
        assert_eq!(output, b"<ctx>");
        {
            let mut writer = XmlWriter::new(&mut output, pretty);
            writer.write_all(b"<r/>").unwrap();
            writer.flush().unwrap();
            writer.write_all(b"</ctx>").unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(output, b"<ctx><r/></ctx>");
    }
}

#[test]
fn plain_text_json_conversion_preserves_xml_examples_and_ampersands() {
    for input in [
        "  Recall: parser & lexer\n<r>\n  <child/>\n</r>\n<!-- literal -->\n",
        "not an XML report",
    ] {
        assert_eq!(
            json_report(input.as_bytes()),
            serde_json::json!({ "text": input.trim() })
        );
    }
}

#[test]
fn recall_renderer_finishes_as_a_text_json_envelope() {
    let value = serde_json::json!({
        "relevant": 1,
        "documents": 1,
        "docs": [{
            "path": "README.md",
            "score": 0.5,
            "content": "Parser & lexer\n<r>\n  <child/>\n</r>\n<!-- keep -->\n"
        }]
    });
    let mut plain = Vec::new();
    ripwire_rs::serialize::serialize_recall_text("parser", &value, &mut plain).unwrap();
    let mut output = Vec::new();
    {
        let mut writer = XmlWriter::with_format(&mut output, false, true);
        ripwire_rs::serialize::serialize_recall_text("parser", &value, &mut writer).unwrap();
        writer.flush().unwrap();
        writer.finish().unwrap();
    }
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["text"], std::str::from_utf8(&plain).unwrap().trim());
    assert!(
        report["text"]
            .as_str()
            .unwrap()
            .contains("<r>\n  <child/>\n</r>\n<!-- keep -->")
    );
}

#[test]
fn test_row_bytes_measure_rows_as_minified_output_emits_them() {
    let row = |w: &mut dyn std::io::Write| {
        writeln!(w, "  <file p=\"src/a.rs\">")?;
        writeln!(w, "    <f p=\"src/b.rs\" deps=\"2\"/>")?;
        writeln!(w, "  </file>")
    };
    let minified = "<file p=\"src/a.rs\"><f p=\"src/b.rs\" deps=\"2\"/></file>";
    assert_eq!(ripwire_rs::serialize::row_bytes(row), minified.len());
}
