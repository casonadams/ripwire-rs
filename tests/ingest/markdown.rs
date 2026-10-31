use ripwire_rs::languages::markdown::extract::extract_markdown;
use ripwire_rs::{Lang, LanguageRegistry};

#[test]
fn test_markdown_setext_atx_and_links() {
    let reg = LanguageRegistry::new().unwrap();
    let (language, _) = reg.get(Lang::Markdown).unwrap();
    let md = b"Main Title\n==========\n\n## Hello World ##\r\n\nSub Section\n-----------\n\nHere is a [link](#sub-section).\n";
    let res = extract_markdown(md, "docs/guide.md", language);
    assert!(res.defs.iter().any(|d| d.name == "Main Title"));
    assert!(res.defs.iter().any(|d| d.name == "Hello World"));
    assert!(res.defs.iter().any(|d| d.name == "Sub Section"));
    assert!(!res.refs.is_empty());
    // The anchor names the heading it slugs to (`Sub Section`), not the raw slug.
    assert_eq!(res.refs[0].callee_name, "Sub Section");
}

#[test]
fn test_markdown_links_name_only_repository_targets() {
    let reg = LanguageRegistry::new().unwrap();
    let (language, _) = reg.get(Lang::Markdown).unwrap();
    let md = b"# Guide\n\nSee [setup](#sub-section), [site](https://example.com/x), [code](#ignored) and `[span](#ignored)`.\n\n## Sub Section\n\nSee [other](other.md \"Other\") and [img](logo.png) and [missing](#nope).\n\n```md\n[fenced](#sub-section)\n```\n";
    let res = extract_markdown(md, "docs/guide.md", language);
    let callees: Vec<&str> = res.refs.iter().map(|r| r.callee_name.as_str()).collect();
    assert_eq!(callees, ["Sub Section", "other"]);
}
