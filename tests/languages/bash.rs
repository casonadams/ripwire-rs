use ripwire_rs::Lang;

#[test]
fn test_bash_graph_protocol() {
    assert!(!Lang::Bash.semantics().supports_slice());
}

#[test]
fn test_bash_quoted_heredoc_terminator() {
    let cases: [(&str, Option<&str>); 7] = [
        ("python3 - \"$X\" <<'PY' | pbcopy", Some("PY")),
        ("cat <<-\"EOF\"", Some("EOF")),
        ("cat << 'END_SQL'", Some("END_SQL")),
        ("cat <<EOF", None),
        ("python3 -c '", None),
        ("echo $((1 << 2))", None),
        ("cat <<''", None),
    ];
    for (line, want) in cases {
        assert_eq!(
            Lang::Bash.semantics().quoted_heredoc_terminator(line),
            want,
            "{line}"
        );
    }
    assert_eq!(
        Lang::Python
            .semantics()
            .quoted_heredoc_terminator("x <<'PY'"),
        None
    );
}
