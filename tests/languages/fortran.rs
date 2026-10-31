use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_fortran_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "fortran",
        files: &[(
            "src/math.f90",
            Lang::Fortran,
            b"module math_ops\ncontains\n  subroutine compute(x)\n    integer :: x\n    call helper(x)\n  end subroutine compute\n  subroutine helper(y)\n    integer :: y\n  end subroutine helper\nend module math_ops\n",
        )],
        expected_syms: &[
            sym("math_ops", SymKind::Section, None, false),
            sym("compute", SymKind::Function, None, false),
            sym("helper", SymKind::Function, None, false),
        ],
        expected_edges: &[("compute", "helper")],
        ..Default::default()
    });
    assert!(Lang::Fortran.semantics().is_std_or_builtin_member("matmul"));
    assert!(
        Lang::Fortran
            .semantics()
            .is_std_or_builtin_qualifier("iso_c_binding")
    );
    assert_eq!(Lang::from_extension("f90"), Lang::Fortran);
    assert_eq!(Lang::from_extension("f"), Lang::Fortran);
}

/// A named generic interface is a procedure family: a call to `scale(..)` names the generic.
#[test]
fn fortran_named_generic_interface_is_a_callable_definition() {
    let source = b"module shapes\n  implicit none\n  interface scale\n    module procedure scale_real\n  end interface scale\ncontains\n  real function scale_real(x, f)\n    real, intent(in) :: x, f\n    scale_real = x * f\n  end function scale_real\nend module shapes\n\nprogram demo\n  use shapes\n  real :: a\n  a = scale(1.0, 2.0)\nend program demo\n";
    let files: &[(&str, Lang, &[u8])] = &[("src/shapes.f90", Lang::Fortran, source)];
    assert_eq!(
        owner_qualified_callees(files, "demo"),
        vec![".scale".to_string()]
    );
}
