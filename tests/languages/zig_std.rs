use super::zig::callee_locations;
use ripwire_rs::Lang;

const STD_CHAIN: &[(&str, Lang, &[u8])] = &[
    ("src/check.zig", Lang::Zig, b"const std = @import(\"std\");\npub fn check() !void {\n    try std.testing.expectEqual(@as(u32, 1), 1);\n}\n"),
    ("src/helpers.zig", Lang::Zig, b"pub fn expectEqual(a: anytype, b: anytype) !void {\n    _ = a;\n    _ = b;\n}\n"),
];

const STD_MEMBER: &[(&str, Lang, &[u8])] = &[
    ("src/emit.zig", Lang::Zig, b"pub fn emit(w: anytype) !void {\n    try w.writeAll(\"x\");\n}\n"),
    ("src/sink.zig", Lang::Zig, b"pub const Sink = struct {\n    pub fn writeAll(self: *Sink, bytes: []const u8) !void {\n        _ = self;\n        _ = bytes;\n    }\n};\n"),
];

#[test]
fn zig_std_chain_binds_to_std_not_a_same_named_in_tree_function() {
    assert_eq!(callee_locations(STD_CHAIN, "check"), Vec::<String>::new());
}

#[test]
fn zig_std_member_name_does_not_bind_to_an_unrelated_in_tree_method() {
    assert_eq!(callee_locations(STD_MEMBER, "emit"), Vec::<String>::new());
}
