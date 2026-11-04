use super::escape::escape_xml_write;

const BUILTIN_LAYERS: &[(&str, &str)] = &[
    ("game", "game"),
    ("gameplay", "game"),
    ("infra", "infra"),
    ("infrastructure", "infra"),
    ("infrastucture", "infra"),
    ("metal", "render"),
    ("render", "render"),
    ("renderer", "render"),
    ("math", "math"),
    ("numerics", "math"),
    ("sound", "audio"),
    ("audio", "audio"),
    ("steer", "ai"),
    ("ai", "ai"),
    ("behavior", "ai"),
    ("test", "test"),
    ("tests", "test"),
    ("bench", "test"),
];

/// Extension trait for built-in architectural layer inference and `<f p=... layer=...>` tag emission.
pub trait PathLayerExt {
    fn builtin_layer(&self) -> Option<&'static str>;
    fn is_inventory_code_file(&self, lang: crate::domain::Lang) -> bool;
    fn write_layer_attr<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()>;
    fn write_file_open_tag<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()>;
}

impl PathLayerExt for str {
    fn builtin_layer(&self) -> Option<&'static str> {
        let (dirs, _) = self.rsplit_once('/')?;
        for comp in dirs.split('/') {
            for &(dir_name, layer) in BUILTIN_LAYERS {
                if comp.eq_ignore_ascii_case(dir_name) {
                    return Some(layer);
                }
            }
        }
        crate::domain::test_path::is_test_path(self).then_some("test")
    }

    fn is_inventory_code_file(&self, lang: crate::domain::Lang) -> bool {
        !lang.semantics().is_prose_document()
            && !lang.semantics().is_data_format()
            && !crate::domain::test_path::is_test_or_bench_path(self)
    }

    fn write_layer_attr<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        if let Some(layer) = self.builtin_layer() {
            write!(out, " layer=\"{layer}\"")
        } else {
            Ok(())
        }
    }

    fn write_file_open_tag<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        out.write_all(b"<f p=\"")?;
        escape_xml_write(out, self)?;
        out.write_all(b"\"")?;
        self.write_layer_attr(out)?;
        out.write_all(b">")
    }
}
