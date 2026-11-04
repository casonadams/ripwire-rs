use super::harness::assert_call_result_binds_factory_return;
use ripwire_rs::Lang;

#[test]
fn java_call_result_receiver_uses_the_factory_return_type() {
    assert_call_result_binds_factory_return(
        Lang::Java,
        "src/Service.java",
        "class DiskStore { int open() { return 1; } }\n\
class MemoryStore { int open() { return 2; } }\n\
class Service {\n  DiskStore make() { return new DiskStore(); }\n  void run() { make().open(); }\n}\n",
    );
}
