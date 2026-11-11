/// Next.js app-router files the framework loads by name: every exported function in them is called by
/// the framework (`default` pages and layouts, `GET`/`OPTIONS` route handlers, `generateMetadata`).
const APP_FILES: &[&str] = &[
    "page",
    "layout",
    "loading",
    "error",
    "global-error",
    "not-found",
    "template",
    "default",
    "route",
    "opengraph-image",
    "twitter-image",
    "icon",
    "apple-icon",
    "sitemap",
    "robots",
    "manifest",
];

/// Files Next.js loads by name from the project or `src` root.
const ROOT_FILES: &[&str] = &["middleware", "instrumentation"];

pub(in crate::languages) fn is_next_convention_file(rel_path: &str) -> bool {
    let mut segments: Vec<&str> = rel_path.split('/').collect();
    let Some(file) = segments.pop() else {
        return false;
    };
    let Some((stem, ext)) = file.rsplit_once('.') else {
        return false;
    };
    if !matches!(ext, "js" | "jsx" | "ts" | "tsx" | "mjs") {
        return false;
    }
    (APP_FILES.contains(&stem) && segments.contains(&"app"))
        || (ROOT_FILES.contains(&stem) && matches!(segments.last(), None | Some(&"src")))
}
