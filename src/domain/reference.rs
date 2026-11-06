use super::{Lang, NodeId, RefRole};
use lasso::Spur;
use serde::{Deserialize, Serialize};

/// Unresolved reference extracted from AST syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub from_symbol: NodeId,
    pub callee_name: Spur,
    pub qualifier: Option<Spur>,
    pub receiver_type: Option<Spur>,
    pub file_id: u32,
    pub line: u32,
    pub name_line: u32,
    pub start_byte: u32,
    pub lang: Lang,
    pub role: RefRole,
    pub arg_count: u16,
    pub arg_count_known: bool,
    pub callable_field: bool,
    /// The call is written inside a macro argument list, where the name cannot be confirmed as a call.
    pub macro_arg: bool,
    /// The reference is a member access (`x.f()`), so its target is a property, not a bare name.
    pub member_call: bool,
    /// The name is reached through a path (`Type::f`), so a method's receiver is written as its first argument.
    pub path_call: bool,
    /// The file whose imports name `receiver_type`, when that file is not this reference's own (a field's declared
    /// type, written in the file that declares the field).
    pub receiver_file: Option<u32>,
}

impl Reference {
    pub fn new(
        from_symbol: NodeId,
        callee_name: Spur,
        file_id: u32,
        line: u32,
        lang: Lang,
        role: RefRole,
    ) -> Self {
        Self {
            from_symbol,
            callee_name,
            qualifier: None,
            receiver_type: None,
            file_id,
            line,
            name_line: line,
            start_byte: 0,
            lang,
            role,
            arg_count: 0,
            arg_count_known: false,
            callable_field: false,
            macro_arg: false,
            member_call: false,
            path_call: false,
            receiver_file: None,
        }
    }

    #[must_use]
    pub fn with_qualifier(mut self, qualifier: Spur) -> Self {
        self.qualifier = Some(qualifier);
        self
    }
}
