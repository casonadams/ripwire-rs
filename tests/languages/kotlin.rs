use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

#[test]
fn test_kotlin_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "kotlin",
        files: &[(
            "src/Handler.kt",
            Lang::Kotlin,
            b"import com.example.Util\ninterface Handler\nclass HttpHandler : Handler {\n    fun handle() {\n        Collections.emptyList()\n        dispatchReq()\n    }\n    fun dispatchReq() {\n        return\n    }\n}\n",
        )],
        expected_syms: &[
            sym("Handler", SymKind::Interface, None, false),
            sym("HttpHandler", SymKind::Class, None, true),
            sym("handle", SymKind::Method, Some("HttpHandler"), true),
            sym("dispatchReq", SymKind::Method, Some("HttpHandler"), true),
        ],
        expected_edges: &[("handle", "dispatchReq")],
        expect_external: true,
        expected_lego: Some(("Handler", "HttpHandler")),
        expected_import: Some("Util"),
        ..Default::default()
    });
}

const STORES: &str = "interface AccountStore {\n    fun lookup(id: Int): Account\n}\n\
interface OrderStore {\n    fun lookup(id: Int): Order\n}\n\
open class Base {\n    fun close() {}\n}\n\
class Other {\n    fun close() {}\n}\n\
class Account\nclass Order\n\
class Desk(private val orders: OrderStore) : Base() {\n    lateinit var accounts: AccountStore\n    fun viaThisField() { this.accounts.lookup(1) }\n    fun viaField() { accounts.lookup(1) }\n    fun viaConstructorProperty() { orders.lookup(1) }\n    fun viaLocal() { val store: OrderStore = orders; store.lookup(2) }\n    fun viaSuper() { super.close() }\n    fun make(): AccountStore = accounts\n    fun viaCallResult() { make().lookup(3) }\n}\n";

#[test]
fn kotlin_typed_receivers_pick_the_declared_type_method() {
    let callees = |caller| {
        owner_qualified_callees(&[("src/Desk.kt", Lang::Kotlin, STORES.as_bytes())], caller)
    };
    assert_eq!(callees("viaThisField"), ["AccountStore.lookup"]);
    assert_eq!(callees("viaField"), ["AccountStore.lookup"]);
    assert_eq!(callees("viaConstructorProperty"), ["OrderStore.lookup"]);
    assert_eq!(callees("viaLocal"), ["OrderStore.lookup"]);
    assert_eq!(callees("viaSuper"), ["Base.close"]);
    assert_eq!(
        callees("viaCallResult"),
        ["AccountStore.lookup", "Desk.make"]
    );
}
