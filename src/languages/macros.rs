macro_rules! impl_grammar_method {
    (crate $lang_expr:expr, $scm:expr) => {
        fn grammar_and_query(
            &self,
        ) -> Option<(&'static tree_sitter::Language, &'static tree_sitter::Query)> {
            static ENTRY: std::sync::LazyLock<(tree_sitter::Language, tree_sitter::Query)> =
                std::sync::LazyLock::new(|| {
                    let ts: tree_sitter::Language = $lang_expr.into();
                    let q = tree_sitter::Query::new(&ts, include_str!($scm)).expect("valid query");
                    (ts, q)
                });
            Some((&ENTRY.0, &ENTRY.1))
        }
    };
    (ffi $ffi_fn:ident, $scm:expr) => {
        fn grammar_and_query(
            &self,
        ) -> Option<(&'static tree_sitter::Language, &'static tree_sitter::Query)> {
            unsafe extern "C" {
                fn $ffi_fn() -> *const tree_sitter::ffi::TSLanguage;
            }
            static ENTRY: std::sync::LazyLock<(tree_sitter::Language, tree_sitter::Query)> =
                std::sync::LazyLock::new(|| {
                    let ts = unsafe { tree_sitter::Language::from_raw($ffi_fn()) };
                    let q = tree_sitter::Query::new(&ts, $scm).expect("valid query");
                    (ts, q)
                });
            Some((&ENTRY.0, &ENTRY.1))
        }
    };
}
pub(crate) use impl_grammar_method;

macro_rules! impl_class_dispatch_flags {
    () => {
        fn indexes_implementations(&self) -> bool {
            true
        }
        fn typed_receiver_miss_uses_names(&self) -> bool {
            true
        }
        fn has_class_inheritance(&self) -> bool {
            true
        }
        fn interface_calls_keep_declaration(&self) -> bool {
            true
        }
    };
}
pub(crate) use impl_class_dispatch_flags;

macro_rules! impl_typed_class_hooks {
    ($typed:expr) => {
        fn read_bindings<'a>(
            &self,
            root: tree_sitter::Node<'_>,
            source: &'a [u8],
        ) -> super::reads::ReadBindings<'a> {
            super::typed::typed_read_bindings(root, source, &$typed)
        }
        fn implementations(
            &self,
            root: tree_sitter::Node<'_>,
            source: &[u8],
        ) -> Vec<super::Implementation> {
            super::typed::class_implementations(root, source, &$typed)
        }
        super::impl_class_dispatch_flags!();
        fn call_receiver_name<'a>(
            &self,
            receiver: tree_sitter::Node<'_>,
            source: &'a [u8],
        ) -> Option<&'a str> {
            super::typed::call_receiver_name(receiver, source, &$typed)
        }
    };
}
pub(crate) use impl_typed_class_hooks;
