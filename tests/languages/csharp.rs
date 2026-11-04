use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

const CS_STORES: &str = "class DiskStore { public int Open() { return 1; } }\n\
class MemoryStore { public int Open() { return 2; } }\n";

const CS_SERVICE: &str = "class Service {\n  private DiskStore store;\n\
  void ViaField() { store.Open(); }\n\
  void ViaThisField() { this.store.Open(); }\n\
  void ViaParam(MemoryStore memory) { memory.Open(); }\n\
  void ViaLocal() { var local = new DiskStore(); local.Open(); }\n}\n";

#[test]
fn csharp_typed_receivers_pick_the_declared_class_method() {
    let callees = |caller| {
        owner_qualified_callees(
            &[
                ("src/Stores.cs", Lang::CSharp, CS_STORES.as_bytes()),
                ("src/Service.cs", Lang::CSharp, CS_SERVICE.as_bytes()),
            ],
            caller,
        )
    };
    assert_eq!(callees("ViaField"), ["DiskStore.Open"]);
    assert_eq!(callees("ViaThisField"), ["DiskStore.Open"]);
    assert_eq!(callees("ViaParam"), ["MemoryStore.Open"]);
    assert!(callees("ViaLocal").contains(&"DiskStore.Open".to_string()));
    assert!(!callees("ViaLocal").contains(&"MemoryStore.Open".to_string()));
}

#[test]
fn csharp_base_receiver_calls_the_base_class_method_not_the_override() {
    let source = "class Encoder {\n  public virtual void Encode(int v) {}\n}\n\
class Sink {\n  public void Encode(int v) {}\n}\n\
class CompactEncoder : Encoder, IDisposable {\n  public override void Encode(int v) {}\n  public void Emit(int v) { base.Encode(v); }\n}\n";
    let callees = owner_qualified_callees(
        &[("src/Encoders.cs", Lang::CSharp, source.as_bytes())],
        "Emit",
    );
    assert_eq!(callees, ["Encoder.Encode"]);
}
