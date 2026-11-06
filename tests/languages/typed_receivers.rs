use super::harness::owner_qualified_callees;
use ripwire_rs::Lang;

/// What one caller's callees must satisfy.
enum Expect {
    Exactly(&'static [&'static str]),
    Includes(&'static str),
    Excludes(&'static str),
}

/// One fixture and the callers to check in it.
struct Row {
    files: &'static [(&'static str, Lang, &'static str)],
    cases: &'static [(&'static str, Expect)],
}

const CS_STORES: &str = "class DiskStore { public int Open() { return 1; } }\n\
class MemoryStore { public int Open() { return 2; } }\n";

const CS_SERVICE: &str = "class Service {\n  private DiskStore store;\n\
  void ViaField() { store.Open(); }\n\
  void ViaThisField() { this.store.Open(); }\n\
  void ViaParam(MemoryStore memory) { memory.Open(); }\n\
  void ViaLocal() { var local = new DiskStore(); local.Open(); }\n}\n";

const CS_ENCODERS: &str = "class Encoder {\n  public virtual void Encode(int v) {}\n}\n\
class Sink {\n  public void Encode(int v) {}\n}\n\
class CompactEncoder : Encoder, IDisposable {\n  public override void Encode(int v) {}\n  public void Emit(int v) { base.Encode(v); }\n}\n";

const JAVA_STORES: &str = "class DiskStore { int open() { return 1; } }\n\
class MemoryStore { int open() { return 2; } }\n\
class Base { void close() {} }\n\
class Pooled extends Base {}\n";

const JAVA_SERVICE: &str = "class Service {\n  private DiskStore store;\n\
  void viaField() { store.open(); }\n\
  void viaThisField() { this.store.open(); }\n\
  void viaParam(MemoryStore memory) { memory.open(); }\n\
  void viaLocal() { var local = new DiskStore(); local.open(); }\n\
  void viaInherited(Pooled pooled) { pooled.close(); }\n}\n";

const JAVA_POOL: &str = "class Base { void close() {} void helper() {} }\n\
class Other { void close() {} void helper() {} }\n\
class Pooled extends Base { void work() { helper(); } }\n\
class User { void run(Pooled pooled) { pooled.close(); } }\n";

const JAVA_ARGS: &str = "class Single { void stash(int a) {} }\n\
class Pair { void stash(int a, int b) {} }\n\
class Client {\n  void one(Object store) { store.stash(1); }\n  void two(Object store) { store.stash(1, 2); }\n}\n";

const JAVA_IFACE: &str = "interface Store { void stash(int a); }\n\
class Disk implements Store { public void stash(int a) {} }\n\
class Client {\n  void run(Store store) { store.stash(1); }\n}\n";

const KT_STORES: &str = "interface AccountStore {\n    fun lookup(id: Int): Account\n}\n\
interface OrderStore {\n    fun lookup(id: Int): Order\n}\n\
open class Base {\n    fun close() {}\n}\n\
class Other {\n    fun close() {}\n}\n\
class Account\nclass Order\n\
class Desk(private val orders: OrderStore) : Base() {\n    lateinit var accounts: AccountStore\n    fun viaThisField() { this.accounts.lookup(1) }\n    fun viaField() { accounts.lookup(1) }\n    fun viaConstructorProperty() { orders.lookup(1) }\n    fun viaLocal() { val store: OrderStore = orders; store.lookup(2) }\n    fun viaSuper() { super.close() }\n    fun make(): AccountStore = accounts\n    fun viaCallResult() { make().lookup(3) }\n}\n";

const RUBY_PIPE: &str = "class Sink\n  def write(line)\n    1\n  end\n\n  def respond_to?(name)\n    true\n  end\nend\n\
DEFAULT_LOG = Sink.new\n\
class Pipe\n  def initialize\n    @sink = Sink.new\n  end\n\n\
  def typed(line)\n    @sink.write(line)\n  end\n\n\
  def untyped(io, line)\n    io.write(line)\n  end\n\n\
  def probe(obj)\n    obj.respond_to?(:write)\n  end\n\n\
  def constant_receiver\n    DEFAULT_LOG.write(\"x\")\n  end\nend\n";

const RUBY_TASKS: &str = "class Task\n  def call(env)\n    1\n  end\n\n\
  def run(app)\n    app.call(2)\n  end\nend\n";

const RUBY_RUNNER: &str = "class Runner\n  def go(app)\n    app.call(1)\n  end\nend\n";

const SCALA_STORES: &str = "trait AccountStore {\n  def lookup(id: Int): Account\n}\n\
trait OrderStore {\n  def lookup(id: Int): Order\n}\n\
class Base {\n  def close(): Unit = ()\n}\n\
class Other {\n  def close(): Unit = ()\n}\n\
class Account\nclass Order\n\
class Desk(val orders: OrderStore) extends Base {\n  val accounts: AccountStore = null\n  def viaThisField(): Unit = this.accounts.lookup(1)\n  def viaField(): Unit = accounts.lookup(1)\n  def viaConstructorProperty(): Unit = orders.lookup(1)\n  def viaLocal(): Unit = {\n    val store: OrderStore = orders\n    store.lookup(2)\n  }\n  def viaSuper(): Unit = super.close()\n}\n";

const PHP_STORES: &str = "<?php\nclass DiskStore { public function open() { return 1; } }\n\
class MemoryStore { public function open() { return 2; } }\n\
class ReadFailed { public static function forPath($p) { return new self(); } }\n\
class WriteFailed { public static function forPath($p) { return new self(); } }\n";

const PHP_SERVICE: &str = "<?php\nclass Service {\n  private DiskStore $store;\n\
  public function viaThisField() { $this->store->open(); }\n\
  public function viaParam(MemoryStore $memory) { $memory->open(); }\n\
  public function viaLocal() { $local = new DiskStore(); $local->open(); }\n\
  public function viaStatic() { throw WriteFailed::forPath('p'); }\n}\n";

const PY_TEXT: &str = "class Text:\n    def split(self):\n        return 1\n\n    @classmethod\n    def from_markup(cls, s) -> Text:\n        return cls()\n\n\nclass Store:\n    def get(self, key):\n        return key\n";

const PY_HOLDER: &str = "from text_mod import Text\n\nprogress = Text()\n\n\nclass Holder:\n    def __init__(self, text: Text):\n        self.text = text\n        self.size = Text.from_markup(\"x\")\n        self.store = Store()\n\n    def via_field(self):\n        self.text.split()\n\n    def via_call_result(self):\n        self.size.split()\n\n    def via_module(self):\n        progress.split()\n\n    def via_with(self):\n        with Text() as handle:\n            handle.split()\n\n    def via_builtin(self, mapping: dict):\n        return mapping.get(1)\n\n    def via_own_get(self):\n        self.store.get(2)\n";

const PY_RESULT: &str = "class Text:\n    def split(self):\n        return 1\n\n    @classmethod\n    def from_markup(cls, s) -> Text:\n        return cls()\n\nclass H:\n    def __init__(self):\n        self.size = Text.from_markup(\"x\")\n\n    def via(self):\n        self.size.split()\n";

const RUBY_APP: &str = "module Web\n  class FakeRequest\n    def get(uri)\n      1\n    end\n  end\n  class Router\n    def get(uri)\n      2\n    end\n  end\n  class Logger\n    def write(line)\n      3\n    end\n  end\n  class Sink\n    def write(line)\n      4\n    end\n  end\nend\n\
class App\n  def initialize\n    @log = Web::Logger.new\n  end\n  def via_chain\n    Web::FakeRequest.new(self).get(\"/\")\n  end\n  def via_local\n    fake = Web::FakeRequest.new(self)\n    fake.get(\"/\")\n  end\n  def via_ivar\n    @log.write(\"x\")\n  end\nend\n";

const RUBY_ENV: &str = "class Fake\n  def self.build_env(uri = \"\", opts = {})\n    1\n  end\nend\n\
def build\n  Fake.build_env(\"/\", \"HOST\" => \"example.org\", \"PORT\" => \"8080\")\nend\n\
def build_kw\n  Fake.build_env(\"/\", method: \"POST\", input: \"x\")\nend\n";

const SWIFT_RUNNER: &str = "struct Info {\n    func summary() -> String { \"\" }\n}\n\
struct Runner {\n    let name: String = \"\"\n    func run(info: Info) {\n        let summary = info.summary()\n        print(summary)\n    }\n}\n";

const CPP_SUB: &str = "class Base {\n public:\n  void refill() {}\n};\n\
class Other {\n public:\n  void refill() {}\n};\n\
class Store {\n public:\n  void stash(int v) {}\n};\n\
class Cache {\n public:\n  void stash(int v) {}\n};\n\
class Sub : public Base {\n public:\n  void viaThis() { this->refill(); }\n  void bare() { refill(); }\n  void viaRef(Store& store) { store.stash(1); }\n  void viaPtr(Store* store) { store->stash(2); }\n};\n";

const CPP_CANVAS_IN: &str =
    "static void release_all() {}\nclass Canvas {\npublic:\n  ~Canvas() { release_all(); }\n};\n";

const CPP_CANVAS_OUT: &str = "static void release_all() {}\nclass Canvas {\npublic:\n  void flush();\n};\nCanvas::~Canvas() { release_all(); }\n";

const PY_CACHE: &str = "class Cache:\n    def fetch_one(self, k):\n        return k\n\nclass Queue:\n    def fetch_one(self, k):\n        return k\n\ndef use(c: Cache):\n    return c.fetch_one(3)\n\ndef build():\n    cache = Cache()\n    return cache.fetch_one(1)\n\ndef mixed(c: Cache):\n    c = Queue()\n    return c.fetch_one(2)\n";

const RUBY_MIXIN: &str = "module Helpers\n  def escape(value)\n    value\n  end\nend\n\
class Renderer\n  include Helpers\n  def render(text)\n    escape(text)\n  end\nend\n";

const TYPED_ROWS: &[Row] = &[
    Row {
        files: &[("src/sub.cpp", Lang::Cpp, CPP_SUB)],
        cases: &[
            ("viaThis", Expect::Exactly(&["Base.refill"])),
            ("bare", Expect::Exactly(&["Base.refill"])),
            ("viaRef", Expect::Exactly(&["Store.stash"])),
            ("viaPtr", Expect::Exactly(&["Store.stash"])),
        ],
    },
    Row {
        files: &[
            ("src/Stores.cs", Lang::CSharp, CS_STORES),
            ("src/Service.cs", Lang::CSharp, CS_SERVICE),
        ],
        cases: &[
            ("ViaField", Expect::Exactly(&["DiskStore.Open"])),
            ("ViaThisField", Expect::Exactly(&["DiskStore.Open"])),
            ("ViaParam", Expect::Exactly(&["MemoryStore.Open"])),
            ("ViaLocal", Expect::Includes("DiskStore.Open")),
            ("ViaLocal", Expect::Excludes("MemoryStore.Open")),
        ],
    },
    Row {
        files: &[
            ("src/Stores.java", Lang::Java, JAVA_STORES),
            ("src/Service.java", Lang::Java, JAVA_SERVICE),
        ],
        cases: &[
            ("viaField", Expect::Exactly(&["DiskStore.open"])),
            ("viaThisField", Expect::Exactly(&["DiskStore.open"])),
            ("viaParam", Expect::Exactly(&["MemoryStore.open"])),
            ("viaLocal", Expect::Includes("DiskStore.open")),
            ("viaLocal", Expect::Excludes("MemoryStore.open")),
            ("viaInherited", Expect::Exactly(&["Base.close"])),
        ],
    },
    Row {
        files: &[("src/Desk.kt", Lang::Kotlin, KT_STORES)],
        cases: &[
            ("viaThisField", Expect::Exactly(&["AccountStore.lookup"])),
            ("viaField", Expect::Exactly(&["AccountStore.lookup"])),
            (
                "viaConstructorProperty",
                Expect::Exactly(&["OrderStore.lookup"]),
            ),
            ("viaLocal", Expect::Exactly(&["OrderStore.lookup"])),
            ("viaSuper", Expect::Exactly(&["Base.close"])),
            (
                "viaCallResult",
                Expect::Exactly(&["AccountStore.lookup", "Desk.make"]),
            ),
        ],
    },
];

const TYPED_ROWS_SCALA: &[Row] = &[
    Row {
        files: &[("src/Desk.scala", Lang::Scala, SCALA_STORES)],
        cases: &[
            ("viaThisField", Expect::Exactly(&["AccountStore.lookup"])),
            ("viaField", Expect::Exactly(&["AccountStore.lookup"])),
            (
                "viaConstructorProperty",
                Expect::Exactly(&["OrderStore.lookup"]),
            ),
            ("viaLocal", Expect::Exactly(&["OrderStore.lookup"])),
            ("viaSuper", Expect::Exactly(&["Base.close"])),
        ],
    },
    Row {
        files: &[
            ("src/Stores.php", Lang::Php, PHP_STORES),
            ("src/Service.php", Lang::Php, PHP_SERVICE),
        ],
        cases: &[
            ("viaThisField", Expect::Exactly(&["DiskStore.open"])),
            ("viaParam", Expect::Exactly(&["MemoryStore.open"])),
            ("viaLocal", Expect::Includes("DiskStore.open")),
            ("viaLocal", Expect::Excludes("MemoryStore.open")),
            ("viaStatic", Expect::Exactly(&["WriteFailed.forPath"])),
        ],
    },
    Row {
        files: &[("lib/app.rb", Lang::Ruby, RUBY_APP)],
        cases: &[
            ("via_chain", Expect::Exactly(&["FakeRequest.get"])),
            ("via_local", Expect::Exactly(&["FakeRequest.get"])),
            ("via_ivar", Expect::Exactly(&["Logger.write"])),
        ],
    },
    Row {
        files: &[("src/Runner.swift", Lang::Swift, SWIFT_RUNNER)],
        cases: &[("run", Expect::Exactly(&["Info.summary"]))],
    },
    Row {
        files: &[("pkg/cache.py", Lang::Python, PY_CACHE)],
        cases: &[
            ("use", Expect::Exactly(&["Cache.fetch_one"])),
            ("build", Expect::Exactly(&[".Cache", "Cache.fetch_one"])),
            (
                "mixed",
                Expect::Exactly(&[".Queue", "Cache.fetch_one", "Queue.fetch_one"]),
            ),
        ],
    },
];

const TYPED_ROWS_TAIL: &[Row] = &[
    Row {
        files: &[
            ("pkg/text_mod.py", Lang::Python, PY_TEXT),
            ("pkg/holder.py", Lang::Python, PY_HOLDER),
        ],
        cases: &[
            ("via_field", Expect::Exactly(&["Text.split"])),
            ("via_module", Expect::Exactly(&["Text.split"])),
            ("via_with", Expect::Exactly(&[".Text", "Text.split"])),
            ("via_builtin", Expect::Excludes("Store.get")),
            ("via_own_get", Expect::Exactly(&["Store.get"])),
        ],
    },
    Row {
        files: &[("pkg/result.py", Lang::Python, PY_RESULT)],
        cases: &[("via", Expect::Exactly(&["Text.split"]))],
    },
    Row {
        files: &[("lib/pipe.rb", Lang::Ruby, RUBY_PIPE)],
        cases: &[
            ("typed", Expect::Exactly(&["Sink.write"])),
            ("untyped", Expect::Exactly(&[])),
            ("probe", Expect::Excludes("Sink.respond_to?")),
            ("constant_receiver", Expect::Exactly(&["Sink.write"])),
        ],
    },
    Row {
        files: &[("lib/task.rb", Lang::Ruby, RUBY_TASKS)],
        cases: &[("run", Expect::Exactly(&["Task.call"]))],
    },
    Row {
        files: &[
            ("lib/task.rb", Lang::Ruby, RUBY_TASKS),
            ("lib/runner.rb", Lang::Ruby, RUBY_RUNNER),
        ],
        cases: &[("go", Expect::Exactly(&[]))],
    },
    Row {
        files: &[("lib/renderer.rb", Lang::Ruby, RUBY_MIXIN)],
        cases: &[("render", Expect::Exactly(&["Helpers.escape"]))],
    },
];

const ARGUMENT_ROWS: &[Row] = &[
    Row {
        files: &[("src/Pool.java", Lang::Java, JAVA_POOL)],
        cases: &[
            ("run", Expect::Exactly(&["Base.close"])),
            ("work", Expect::Exactly(&["Base.helper"])),
        ],
    },
    Row {
        files: &[("src/Client.java", Lang::Java, JAVA_ARGS)],
        cases: &[
            ("one", Expect::Exactly(&["Single.stash"])),
            ("two", Expect::Exactly(&["Pair.stash"])),
        ],
    },
    Row {
        files: &[("src/Client.java", Lang::Java, JAVA_IFACE)],
        cases: &[("run", Expect::Includes("Store.stash"))],
    },
    Row {
        files: &[("src/Encoders.cs", Lang::CSharp, CS_ENCODERS)],
        cases: &[("Emit", Expect::Exactly(&["Encoder.Encode"]))],
    },
    Row {
        files: &[("lib/fake.rb", Lang::Ruby, RUBY_ENV)],
        cases: &[
            ("build", Expect::Exactly(&["Fake.build_env"])),
            ("build_kw", Expect::Exactly(&["Fake.build_env"])),
        ],
    },
    Row {
        files: &[("src/canvas.cpp", Lang::Cpp, CPP_CANVAS_IN)],
        cases: &[("~Canvas", Expect::Exactly(&[".release_all"]))],
    },
    Row {
        files: &[("src/canvas.cpp", Lang::Cpp, CPP_CANVAS_OUT)],
        cases: &[("~Canvas", Expect::Exactly(&[".release_all"]))],
    },
];

fn check_rows(rows: &[Row]) {
    for row in rows {
        let files: Vec<(&str, Lang, &[u8])> = row
            .files
            .iter()
            .map(|(path, lang, source)| (*path, *lang, source.as_bytes()))
            .collect();
        for (caller, expect) in row.cases {
            let callees = owner_qualified_callees(&files, caller);
            match expect {
                Expect::Exactly(want) => assert_eq!(callees, *want, "{caller}"),
                Expect::Includes(want) => {
                    assert!(callees.contains(&want.to_string()), "{caller}: {callees:?}")
                }
                Expect::Excludes(not) => {
                    assert!(!callees.contains(&not.to_string()), "{caller}: {callees:?}")
                }
            }
        }
    }
}

#[test]
fn typed_receivers_bind_their_declared_owner_method() {
    check_rows(TYPED_ROWS);
    check_rows(TYPED_ROWS_SCALA);
    check_rows(TYPED_ROWS_TAIL);
}

#[test]
fn arguments_bases_and_destructors_select_one_member() {
    check_rows(ARGUMENT_ROWS);
}
