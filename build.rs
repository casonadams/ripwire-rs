use std::path::PathBuf;

/// Compile the vendored C grammar a plugin declares in `src/languages/<name>/grammar.txt`:
/// `files = parser.c scanner.c`, optional `src = <dir under vendor/grammars/<name>>` and `include = <dirs>`.
fn compile_grammar(name: &str, manifest: &str) {
    let field = |key: &str| -> Vec<&str> {
        manifest
            .lines()
            .find_map(|line| line.strip_prefix(key)?.trim_start().strip_prefix('='))
            .map(|values| values.split_whitespace().collect())
            .unwrap_or_default()
    };
    let dir = PathBuf::from("vendor/grammars").join(name);
    let src_dir = dir.join(field("src").first().copied().unwrap_or("src"));
    let mut build = cc::Build::new();
    build.include(&src_dir);
    for inc in field("include") {
        build.include(dir.join(inc));
    }
    for file in field("files") {
        let path = src_dir.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(path);
    }
    build.warnings(false);
    build.flag_if_supported("/std:c11");
    build.flag_if_supported("-std=c11");
    build.compile(&format!("tree_sitter_{name}"));
}

fn main() {
    assert!(
        PathBuf::from("vendor/grammars").exists(),
        "Missing 'vendor/grammars' directory. Run 'make update-grammars' or './scripts/update-grammars.sh' before building."
    );

    println!("cargo:rerun-if-changed=src/languages/mod.rs");
    let mut plugins: Vec<_> = std::fs::read_dir("src/languages")
        .expect("src/languages")
        .flatten()
        .filter_map(|entry| {
            let manifest_path = entry.path().join("grammar.txt");
            let manifest = std::fs::read_to_string(&manifest_path).ok()?;
            println!("cargo:rerun-if-changed={}", manifest_path.display());
            Some((entry.file_name().to_string_lossy().into_owned(), manifest))
        })
        .collect();
    plugins.sort();
    for (name, manifest) in &plugins {
        compile_grammar(name, manifest);
    }
    install_man_pages();
    embed_skills();
}

fn copy_man_to_dir(src: &std::path::Path, dir: PathBuf) {
    if dir.exists() || std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::copy(src.join("ripwire-rs.1"), dir.join("ripwire-rs.1"));
    }
}

fn install_man_pages() {
    println!("cargo:rerun-if-changed=man/man1/ripwire-rs.1");

    let src = PathBuf::from("man/man1");
    if !src.join("ripwire-rs.1").exists() {
        return;
    }

    if let Ok(home) = std::env::var("HOME") {
        copy_man_to_dir(&src, PathBuf::from(home).join(".local/share/man/man1"));
    }
    copy_man_to_dir(&src, PathBuf::from("/opt/homebrew/share/man/man1"));
}
fn is_skill_md(path: &std::path::Path) -> bool {
    let Some(ext) = path.extension() else {
        return false;
    };
    if ext != "md" {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name != "README.md"
}

fn add_skill_entry(
    base: &std::path::Path,
    path: &std::path::Path,
    entries: &mut Vec<(String, String)>,
) {
    if let Ok(stripped) = path.strip_prefix(base)
        && let Some(raw_src) = stripped.to_str()
    {
        let rel_src = raw_src.replace('\\', "/");
        entries.push((rel_src.clone(), rel_src));
    }
}

fn collect_skills(
    base: &std::path::Path,
    dir: &std::path::Path,
    entries: &mut Vec<(String, String)>,
) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skills(base, &path, entries);
        } else if is_skill_md(&path) {
            add_skill_entry(base, &path, entries);
        }
    }
}

fn embed_skills() {
    println!("cargo:rerun-if-changed=skills");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| "target".into()));
    let dest_path = out_dir.join("embedded_skills.rs");

    let skills_dir = PathBuf::from("skills");
    if !skills_dir.exists() {
        let _ = std::fs::write(
            &dest_path,
            "pub static EMBEDDED_SKILLS: &[(&str, &str)] = &[];\n",
        );
        return;
    }

    let mut entries = Vec::new();
    collect_skills(&skills_dir, &skills_dir, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut code = String::from("pub static EMBEDDED_SKILLS: &[(&str, &str)] = &[\n");
    for (rel_dest, rel_src) in entries {
        code.push_str(&format!(
            "    ({:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/skills/{}\"))),\n",
            rel_dest, rel_src
        ));
    }
    code.push_str("];\n");

    let _ = std::fs::write(&dest_path, code);
}
