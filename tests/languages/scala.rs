use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

#[test]
fn scala_new_expression_reaches_the_constructed_class() {
    let src: &[u8] = b"class Box(v: Int) {\n  def get(): Int = v\n}\nclass Pair[T](a: T, b: T)\nclass Empty\nobject Main {\n  def run(): Int = {\n    val b = new Box(2)\n    val p = new Pair[Int](1, 2)\n    val e = new Empty\n    b.get()\n  }\n}\n";
    let callees = owner_qualified_callees(&[("src/Main.scala", Lang::Scala, src)], "run");
    assert!(callees.contains(&".Box".to_string()), "{callees:?}");
    assert!(callees.contains(&".Pair".to_string()), "{callees:?}");
    assert!(callees.contains(&".Empty".to_string()), "{callees:?}");
}
