use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

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
