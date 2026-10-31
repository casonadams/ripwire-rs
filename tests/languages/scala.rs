use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_scala_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "scala",
        files: &[(
            "src/Service.scala",
            Lang::Scala,
            b"trait BaseService {\n  def run(): Unit\n}\nobject MainService extends BaseService {\n  def run(): Unit = {\n    List.empty()\n    step()\n  }\n  def step(): Unit = {\n    ()\n  }\n}\n",
        )],
        expected_syms: &[
            sym("BaseService", SymKind::Interface, None, true),
            sym("MainService", SymKind::Class, None, true),
            sym("run", SymKind::Method, Some("MainService"), true),
            sym("step", SymKind::Method, Some("MainService"), true),
        ],
        expected_edges: &[("run", "step")],
        expect_external: true,
        expected_lego: Some(("BaseService", "MainService")),
        ..Default::default()
    });
}

const STORES: &str = "trait AccountStore {\n  def lookup(id: Int): Account\n}\n\
trait OrderStore {\n  def lookup(id: Int): Order\n}\n\
class Base {\n  def close(): Unit = ()\n}\n\
class Other {\n  def close(): Unit = ()\n}\n\
class Account\nclass Order\n\
class Desk(val orders: OrderStore) extends Base {\n  val accounts: AccountStore = null\n  def viaThisField(): Unit = this.accounts.lookup(1)\n  def viaField(): Unit = accounts.lookup(1)\n  def viaConstructorProperty(): Unit = orders.lookup(1)\n  def viaLocal(): Unit = {\n    val store: OrderStore = orders\n    store.lookup(2)\n  }\n  def viaSuper(): Unit = super.close()\n}\n";

#[test]
fn scala_typed_receivers_pick_the_declared_type_method() {
    let callees = |caller| {
        owner_qualified_callees(
            &[("src/Desk.scala", Lang::Scala, STORES.as_bytes())],
            caller,
        )
    };
    assert_eq!(callees("viaThisField"), ["AccountStore.lookup"]);
    assert_eq!(callees("viaField"), ["AccountStore.lookup"]);
    assert_eq!(callees("viaConstructorProperty"), ["OrderStore.lookup"]);
    assert_eq!(callees("viaLocal"), ["OrderStore.lookup"]);
    assert_eq!(callees("viaSuper"), ["Base.close"]);
}

#[test]
fn scala_enum_is_a_type_definition() {
    verify_protocol_case(&GraphProtocolCase {
        label: "scala-enum",
        files: &[(
            "src/Color.scala",
            Lang::Scala,
            b"enum Color {\n  case Red, Green\n}\nclass Shade {\n  def tint(): Int = 1\n}\n",
        )],
        expected_syms: &[
            sym("Color", SymKind::Var, None, false),
            sym("Shade", SymKind::Class, None, true),
        ],
        ..Default::default()
    });
}

#[test]
fn scala_new_expression_reaches_the_constructed_class() {
    let src: &[u8] = b"class Box(v: Int) {\n  def get(): Int = v\n}\nclass Pair[T](a: T, b: T)\nclass Empty\nobject Main {\n  def run(): Int = {\n    val b = new Box(2)\n    val p = new Pair[Int](1, 2)\n    val e = new Empty\n    b.get()\n  }\n}\n";
    let callees = owner_qualified_callees(&[("src/Main.scala", Lang::Scala, src)], "run");
    assert!(callees.contains(&".Box".to_string()), "{callees:?}");
    assert!(callees.contains(&".Pair".to_string()), "{callees:?}");
    assert!(callees.contains(&".Empty".to_string()), "{callees:?}");
}
