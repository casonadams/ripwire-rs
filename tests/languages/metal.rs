use super::harness::{GraphProtocolCase, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_metal_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "metal",
        files: &[(
            "src/shader.metal",
            Lang::Metal,
            b"#include \"shader.h\"\nvoid compute_pass() {\n    metal::sin(0.0);\n    shade_vertex();\n}\nvoid shade_vertex() {\n    return;\n}\n",
        )],
        expected_syms: &[
            sym("compute_pass", SymKind::Function, None, true),
            sym("shade_vertex", SymKind::Function, None, true),
        ],
        expected_edges: &[("compute_pass", "shade_vertex")],
        expect_external: true,
        expected_import: Some("shader.h"),
        ..Default::default()
    });
}

#[test]
fn metal_function_qualifiers_and_address_spaces_parse_into_definitions() {
    verify_protocol_case(&GraphProtocolCase {
        label: "metal-qualifiers",
        files: &[(
            "src/pass.metal",
            Lang::Metal,
            b"#include \"device.h\"\nstruct Uniforms { float scale; };\nfloat scale_uv(float2 uv, constant Uniforms& u) {\n    return uv.x * u.scale;\n}\nkernel void add_one(device float* out [[buffer(0)]],\n                    uint gid [[thread_position_in_grid]]) {\n    out[gid] = scale_uv(float2(0.0), out[gid]);\n}\n",
        )],
        expected_syms: &[
            sym("Uniforms", SymKind::Class, None, false),
            sym("scale_uv", SymKind::Function, None, true),
            sym("add_one", SymKind::Function, None, true),
        ],
        expected_edges: &[("add_one", "scale_uv")],
        expected_import: Some("device.h"),
        ..Default::default()
    });
}
