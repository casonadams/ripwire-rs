# Changelog

## [0.5.4](https://github.com/casonadams/ripwire-rs/compare/v0.5.3...v0.5.4) (2026-10-18)


### Features

* **analysis:** close parity gaps in slicing, C++ extraction, pagination, and map ranking ([64a2639](https://github.com/casonadams/ripwire-rs/commit/64a2639c4d7f2d0a7af25fae4e208ac75ec1e657))
* **analysis:** unify corpus traits and improve cross-repo graph accuracy ([48c254c](https://github.com/casonadams/ripwire-rs/commit/48c254c2ff15a3ca9495cfb501003cabd7065840))
* **deps:** resolve Rust mod edges and super paths, and unify Flags/StrayContent/Mentions onto XmlReport ([e6f8f1b](https://github.com/casonadams/ripwire-rs/commit/e6f8f1b5b0e14ef7fbb60412f5498457641db978))
* **graph:** resolve polymorphic trait calls, trace file:line frames, and paginate match hits ([f8b2cbd](https://github.com/casonadams/ripwire-rs/commit/f8b2cbdabbc32499dc2551e847bc440201f82490))
* **ingest,navigate,gate:** improve knowledge precision and trait ergonomics across lenses ([a175fb9](https://github.com/casonadams/ripwire-rs/commit/a175fb9efbb8ee80f6d6d91c5594a8ce1b27d918))
* **languages:** tighten multi-language stdlib qualifiers and Dart AST extraction ([06ec114](https://github.com/casonadams/ripwire-rs/commit/06ec114ab60632d2a932f5b044e1023f98ed0757))
* **navigate:** close parity gaps on impact importers, doc-drift, C++ uses, and task budgets ([2946c5e](https://github.com/casonadams/ripwire-rs/commit/2946c5eca5094082fd9104c6f3f5a43611113de9))
* **navigate:** support qualified uses, lego method contracts, and generic workspace imports ([da8a0ed](https://github.com/casonadams/ripwire-rs/commit/da8a0ed3b2136a35fd4fe5c5ac783e662458601f))
* **resolve:** add ResolutionLookup trait, C-family macro blanking, and std-member gates ([e1b70d8](https://github.com/casonadams/ripwire-rs/commit/e1b70d8eb51a4ed643e9533af8bab58db51a4c89))
* **resolve:** tighten std/builtin call binding, entry filtering, and task ranking ([8541007](https://github.com/casonadams/ripwire-rs/commit/854100722e4e3f22c9b1916d4e103170a3f6d240))
* **resolve:** triage unresolved calls, extract C++ attribute methods, and refine doc-drift ([1ba9d00](https://github.com/casonadams/ripwire-rs/commit/1ba9d008f1a88dc5d6981d14241c7d78b636dfea))
* **retrieval,slice,resolve:** close parity gaps in signature spans, task lens, slice context, and disambiguation ([b45ca9c](https://github.com/casonadams/ripwire-rs/commit/b45ca9c5c57da61ff3f35b09a6b94067103a8743))
* **serialize:** add PathLayerExt and GitStampExt traits, layer() query filter, and exemplar callee signatures ([74c1fe8](https://github.com/casonadams/ripwire-rs/commit/74c1fe881989f690a07e86701b1e97a4fcca5b4f))
* **serialize:** add PathLayerExt::write_layer_attr, stream XML serializers, and report files_unlisted in --tree ([9cb62e6](https://github.com/casonadams/ripwire-rs/commit/9cb62e692e38265d4b9a5c2ce880949aeb514e78))
* **serialize:** close cross-repo XML parity and knowledge gaps via report traits ([ef012ff](https://github.com/casonadams/ripwire-rs/commit/ef012ffba24b67472951cd0b6b39d916a887f2a7))


### Bug Fixes

* **handoff,retrieval,metrics:** emit detached HEAD attribute and improve cross-repo lens precision ([55d4207](https://github.com/casonadams/ripwire-rs/commit/55d4207c2b54529eb009032d105e1419c822613d))


### Performance Improvements

* **metrics:** make hotspot symbol aggregation O(F+S) and simplify CorpusView lookups ([16ce361](https://github.com/casonadams/ripwire-rs/commit/16ce361fee5d0c38e3b5e69ca2f396bb0c628336))

## [0.5.3](https://github.com/casonadams/ripwire-rs/compare/v0.5.2...v0.5.3) (2026-10-17)


### Features

* **languages,lint,navigate,serialize:** filter C++ cast keywords, refine C-family lint precision, prune impact mega-hubs, and expand XmlReport ([dafb6fc](https://github.com/casonadams/ripwire-rs/commit/dafb6fceb3e1b45f7c46bbf1d9d5be39712a877a))
* **lint:** back every catalog rule with a FileLintRule or SymbolLintRule trait impl ([cb0c5a5](https://github.com/casonadams/ripwire-rs/commit/cb0c5a593442cb1e8ffc29a56108d8eb43f04a72))
* **serialize,graph:** extend XmlReport with graph_gauges/pr_iters and compute real resolver gauges ([f18e1e8](https://github.com/casonadams/ripwire-rs/commit/f18e1e8e21576974cb27d7cd7e84e3eecad2860b))


### Bug Fixes

* **ci,crap,parity:** enforce zero-warning make all/crap and v0.6.7 clean-tree parity ([4c8a875](https://github.com/casonadams/ripwire-rs/commit/4c8a875058b85e6e8e5f6ab6b01bd9804e1a77e2))
* **lint,quality-delta:** mask comments and literals before pattern checks ([16b9c5d](https://github.com/casonadams/ripwire-rs/commit/16b9c5d1a85e8a00575591635c1bd3cea0c86014))

## [0.5.2](https://github.com/casonadams/ripwire-rs/compare/v0.5.1...v0.5.2) (2026-10-16)


### Features

* **cli,gate,lint:** wire all CLI filter/scope/ack flags, 12 delta kinds, and 39 lint rules ([aeab792](https://github.com/casonadams/ripwire-rs/commit/aeab7921560e47438bfde07bb8aa2d075031ed4f))
* **flags,mcp:** scan all code files with root-relative paths in --flags and lazy-load MCP git history ([ec406bb](https://github.com/casonadams/ripwire-rs/commit/ec406bb524395533cc89288e47deefaa057a49ba))
* **metrics,doc-drift,navigate:** compute field-affinity, graph_ambiguous, and .scm doc-drift corroboration ([ccc44bb](https://github.com/casonadams/ripwire-rs/commit/ccc44bbd1f1057376f5e08e47316d01d64f3004a))
* **metrics,gate,navigate:** compute real values for comment-coherence, context-ratio, handoff, pr-context, grep, and impact ([4c67673](https://github.com/casonadams/ripwire-rs/commit/4c67673f95e4d3ea6a0e8986e42b3f274d87125c))
* **metrics,retrieval:** add body-reference corroboration for --for and C/C++ ConfusionFamily in --quality-panel ([f73bb48](https://github.com/casonadams/ripwire-rs/commit/f73bb4866bb6fa699ba2e4eb50168e26519da594))
* **retrieval:** improve --for multi-term ranking and invalidate stale cache on version bump ([42e51e3](https://github.com/casonadams/ripwire-rs/commit/42e51e3882d0bd7e6b52706c61ad3ac07857f55d))
* **validate,skills:** add computed-metric invariants, validate-knowledge suite, and repo-local parity skill ([113a522](https://github.com/casonadams/ripwire-rs/commit/113a522483d9da8ec5c310b4c64aec8177ba0280))


### Bug Fixes

* **cli,mcp:** propagate root path in notes, pr-context, and MCP slice ([9d4e62c](https://github.com/casonadams/ripwire-rs/commit/9d4e62cedd827a6f944203bf7da8d6e20b0640f0))
* **crawl:** apply --ignore-tests and --max-file-size at crawl time ([264aba7](https://github.com/casonadams/ripwire-rs/commit/264aba786cb22e790fcb8a4d255b15d5b00b9461))
* **for:** implement --sections and the counted &lt;lego&gt; stub ([abf9265](https://github.com/casonadams/ripwire-rs/commit/abf926532a177821d0e5698f91757f96ca4f457f))
* **metrics:** resolve root-relative file paths in comment-coherence and quality-panel ([4c14c16](https://github.com/casonadams/ripwire-rs/commit/4c14c16d336e75d859e55f27dec4cc485ec5000b))
* **quality-ack:** write real content ids instead of a zero cid ([2b0e261](https://github.com/casonadams/ripwire-rs/commit/2b0e261a0773bc68c03132b16ec562adcd18966d))
* **quality-baseline:** refuse dirty pins unless --allow-dirty ([04d9cf6](https://github.com/casonadams/ripwire-rs/commit/04d9cf6f11084d354d7cb6ef0213cb847e81e04e))
* **quality-baseline:** stamp the real HEAD sha ([3987528](https://github.com/casonadams/ripwire-rs/commit/39875282ad11ad009d16e58648373e8352ca3eae))
* **quality-delta:** honor --show-stale by collapsing stale-ack rows ([174ce2f](https://github.com/casonadams/ripwire-rs/commit/174ce2fbf9f84c9c0c77ee47a91d853a4ee4ac2a))
* **quality-delta:** report each regression's real symbol line ([5a4f628](https://github.com/casonadams/ripwire-rs/commit/5a4f628eed7846b9474a204ed47142f157167060))
* **zoom:** compute a real multi-level Louvain hierarchy ([3256065](https://github.com/casonadams/ripwire-rs/commit/32560650ca757d6b50dcc7b70d9b3a757ca6a9de))


### Performance Improvements

* **grep:** index symbols by file and caller counts in call_grep_with_context ([08656ad](https://github.com/casonadams/ripwire-rs/commit/08656ad249c40bcc04f236d709cc99b89b32cfee))

## [0.5.1](https://github.com/casonadams/ripwire-rs/compare/v0.5.0...v0.5.1) (2026-10-15)


### Features

* **doc-drift:** wire probe_name_history into --doc-drift --with-history ([8488dfd](https://github.com/casonadams/ripwire-rs/commit/8488dfd831278b5502a28f131bbb4615d1730918))
* **parity,legend:** achieve 67/67 DOM parity, add legend encyclopedia/dedup, and wire CLI knobs ([be38243](https://github.com/casonadams/ripwire-rs/commit/be38243411f0ed602326c6e4c1bd7da903891e45))


### Bug Fixes

* **contract,resolve:** ignore non-definition query matches in edit-check and resolve single-file Tier 3 candidates ([e341dd6](https://github.com/casonadams/ripwire-rs/commit/e341dd60c68d4510b64c8e26a02d06a0df084aa1))
* **dart,navigate:** resolve private method edit-check visibility, named param arity, and multi-def caller union ([cf3e769](https://github.com/casonadams/ripwire-rs/commit/cf3e769bf066865b12863a82984aa335472a0d37))
* **parity:** resolve DIRT.md discrepancies and unify language semantics behind trait ([8bf58c8](https://github.com/casonadams/ripwire-rs/commit/8bf58c88f0ee6cfc8b97be2325878a434fa92240))

## [0.5.0](https://github.com/casonadams/ripwire-rs/compare/v0.4.1...v0.5.0) (2026-10-14)


### Features

* **arch:** implement --arch layering rules, --mermaid diagrams, and --zoom ([eebcb78](https://github.com/casonadams/ripwire-rs/commit/eebcb783df86083847117b53756e46a284e11fb4))
* **bench:** add cargo bench pipeline suite and Makefile bench targets ([6456639](https://github.com/casonadams/ripwire-rs/commit/64566396814fbc573d4c07b695eb1bd9adfa75d7))
* **bench:** add performance and resource benchmark target to Makefile ([d3afaf3](https://github.com/casonadams/ripwire-rs/commit/d3afaf3791591d07d51c8eb12f84631102ac24bc))
* **cli:** add top-k, schema legend, test layering, and git-HEAD delta parity ([9d6368a](https://github.com/casonadams/ripwire-rs/commit/9d6368abd8f7c6b01048e4c9d5ac2714adb7ebb7))
* **cli:** complete 1-to-1 feature and flag parity with ripwire engine ([3b44f34](https://github.com/casonadams/ripwire-rs/commit/3b44f348e71d529c343bc13d102c59319e41fbe4))
* **cli:** complete 1:1 parity with CLI mutations, session tools, and advanced queries ([9f3907c](https://github.com/casonadams/ripwire-rs/commit/9f3907c02ac268201bad19fcbfa74b32792d697f))
* **cli:** expose all top-level Ripwire workflow flags and XML query serializers ([dce2f10](https://github.com/casonadams/ripwire-rs/commit/dce2f105e8c2ec36ab7c1c196d231ebf9c83fec5))
* **cli:** implement categorized clap CLI and core MCP stdio server ([f40da01](https://github.com/casonadams/ripwire-rs/commit/f40da015e1ac1ef7e7b08a39ea6f88d5b8cc6e2f))
* **cli:** implement SARIF export, cochange-groups, transactional edit-plan, global --json, --lsp, and arch globs ([b55bffe](https://github.com/casonadams/ripwire-rs/commit/b55bffea8db1715871e36253a791685cae4e2a27))
* **clones:** implement AST clone detector with Type-1/2/3 matching and idiom demotion ([cf47f85](https://github.com/casonadams/ripwire-rs/commit/cf47f85e908dc1889f2f02d0445118169e902404))
* **distribution:** rename crate to ripwire and binary to rw ([a840f28](https://github.com/casonadams/ripwire-rs/commit/a840f2818da638e1007324c1ae00ce2ffead08c6))
* **domain:** implement SoA model and thread-safe string interner ([42ee6cf](https://github.com/casonadams/ripwire-rs/commit/42ee6cfd59cc8aca67da2e135dafe9a7ab5f73b1))
* **gate:** implement --map-diff, --seams, --pr-context, and --merge-scout ([396d729](https://github.com/casonadams/ripwire-rs/commit/396d729967fdf9187be81b6a7845d27fc2ca342d))
* **gate:** implement pre-commit contract checker and affected test reachability mapper ([f28d1d1](https://github.com/casonadams/ripwire-rs/commit/f28d1d1e7264dffcd6ed02eb749dc1102af71cda))
* **gate:** implement quality delta pre-commit gate and baseline snapshotting ([2744531](https://github.com/casonadams/ripwire-rs/commit/274453162aa0823e68865b69adc6ff0c28ebc562))
* **gate:** implement short-horizon-churn, nesting, params, quality acks, and exit code 2 contract ([2c9db2f](https://github.com/casonadams/ripwire-rs/commit/2c9db2f4f1ce31499c7fd182c9b9bb08a26339a6))
* **git:** implement git commit history mining and shotgun surgery co-change detector ([815f95c](https://github.com/casonadams/ripwire-rs/commit/815f95cf96c300e9ff3b89fc8bb4c3ae5e58a7b8))
* **grammars:** add HTML and CSS tree-sitter grammars ([b7f1392](https://github.com/casonadams/ripwire-rs/commit/b7f1392dc20d19c2874c92dc58f79f09f8289601))
* **grammars:** add Scala and Erlang tree-sitter grammars ([2fe8403](https://github.com/casonadams/ripwire-rs/commit/2fe84035e677eb6fd5a8f4215fcab4c1c3e041a9))
* **grammars:** add SQL, Protobuf, GraphQL, and Dockerfile support, expanding to 35 languages ([c951c9a](https://github.com/casonadams/ripwire-rs/commit/c951c9a6681da097940b5d0a3e4bff76cb3dc617))
* **grammars:** add Zig, Solidity, and HCL tree-sitter grammars ([5020560](https://github.com/casonadams/ripwire-rs/commit/50205606f7f8f885aa5f942f5afc9b2504376010))
* **grammars:** expand language support to 31 languages across systems, cloud, functional, and web ([224fe31](https://github.com/casonadams/ripwire-rs/commit/224fe3106447c9a0ad5d212eb763affc8659c2ad))
* **graph:** implement reference resolution ladder and in-edge CSR construction ([aa9317f](https://github.com/casonadams/ripwire-rs/commit/aa9317f883d2dbdad5ce7142eef57893f8ef8972))
* **ingest:** add Objective-C and Objective-C++ support ([be0b0a2](https://github.com/casonadams/ripwire-rs/commit/be0b0a2c03ee9338a3ff8750b36295422df565db))
* **ingest:** add polyglot crawler and tree-sitter extractors for Tier-1 languages ([a2ce0c5](https://github.com/casonadams/ripwire-rs/commit/a2ce0c56bb2e631a4292aab37e95e3078443299d))
* **ingest:** add remaining 11 grammars and specialized language handlers for 24-language parity ([a1f2b30](https://github.com/casonadams/ripwire-rs/commit/a1f2b303609a7b240e31ac1b0edb3da7d02d3d79))
* **ingest:** add systems and enterprise tree-sitter grammars ([3bc2790](https://github.com/casonadams/ripwire-rs/commit/3bc279037d6635a85acc621ebea1c8fa4835e845))
* **make:** add make validate and make parity targets with cross-repo validator ([07bd1cc](https://github.com/casonadams/ripwire-rs/commit/07bd1cc911af447eac9c62af03138a855c6911ab))
* **man:** add manual pages for rw and ripwire with automatic cargo install ([beb21ab](https://github.com/casonadams/ripwire-rs/commit/beb21abb27c6f6ed466890477819c7bebcae379c))
* **mcp:** complete full 33 MCP tools suite and atomic edit operations ([182176f](https://github.com/casonadams/ripwire-rs/commit/182176f4eb33bf89e466a2707ffde50503ed7f04))
* **metrics:** compute code complexity, readability, and maintenance hotspots ([4f8c866](https://github.com/casonadams/ripwire-rs/commit/4f8c866433dff912826ba2b51ed2721229c19062))
* **metrics:** implement --biggest-first, --nonlocal-state, --lint, and --lint-catalog ([8e0a923](https://github.com/casonadams/ripwire-rs/commit/8e0a923b27e1cb48979d7bd7affb32131f468bea))
* **navigate:** format navigation commands as canonical Ripwire XML ([e6ca76c](https://github.com/casonadams/ripwire-rs/commit/e6ca76c8d0d8632a42671cc95423f6d3ec0e5a1c))
* **navigate:** implement --at, --verify, --pack-task, and --graph-query ([44a139a](https://github.com/casonadams/ripwire-rs/commit/44a139afe44af00b964a97277ac9e456639650b0))
* **packaging:** configure package include for crates.io publishing ([0a224eb](https://github.com/casonadams/ripwire-rs/commit/0a224eb82f3e6f0db9635499e134f721a61766b7))
* **parity:** achieve 100% 1:1 behavioral parity with upstream ripwire across 36 commands ([83c7742](https://github.com/casonadams/ripwire-rs/commit/83c77422750795658207ba1486215b331d0e03f5))
* **parity:** implement wrap command, arity filtering, and html canvas visualizer ([100a6de](https://github.com/casonadams/ripwire-rs/commit/100a6de86b328b7f61e28e9057db31fdf324ee5c))
* **rank:** implement deterministic personalized PageRank with block reductions ([df73fea](https://github.com/casonadams/ripwire-rs/commit/df73fea8d73e770648cbc79803762e055e04738a))
* **rank:** implement Kleinberg HITS algorithm and streaming token budgeting ([9e5c9d5](https://github.com/casonadams/ripwire-rs/commit/9e5c9d5745f0e7eaad19c1867fcea250b9124d86))
* **resolve:** add scope extraction, call qualifiers, and callee deduplication ([919bdde](https://github.com/casonadams/ripwire-rs/commit/919bdde2fed7c849260239b1d094f324d89eaa67))
* **serialize:** align XML schemas and child tags with Ripwire canonical dialect ([5ed0c00](https://github.com/casonadams/ripwire-rs/commit/5ed0c00dd6ad088dc001d87c0f1af9e56423c109))
* **serialize:** implement 64KB streaming minified XML serializer ([a18ae78](https://github.com/casonadams/ripwire-rs/commit/a18ae78d12f134fefc317e23bd42e176f8e7930b))
* **serialize:** implement interactive HTML/SVG call graph export and Louvain community clustering ([0bf1e62](https://github.com/casonadams/ripwire-rs/commit/0bf1e6242f67eb77d26ecb5e08bdfee33b8df94b))
* **skills:** package and install 18 ripwire agent skills with --add-skills ([7affe30](https://github.com/casonadams/ripwire-rs/commit/7affe30d01be7e479e4a1d7a2d5abb7f6bcc093f))
* **slice:** implement ARISE intra-procedural def-use variable slicing and dataflow traversal ([a19506a](https://github.com/casonadams/ripwire-rs/commit/a19506ad6b2979f881675c8599f733a5b95908ef))
* **sql,cli,docs:** implement embedded SQL table lineage, Round 5 CLI verbs, and sync docs ([7217bba](https://github.com/casonadams/ripwire-rs/commit/7217bbaae33ccdde013b998ea7c98590d357ee25))


### Bug Fixes

* **analysis:** resolve Dart analysis gaps, enforce scm query contracts, and minify XML ([43793b0](https://github.com/casonadams/ripwire-rs/commit/43793b06d5ac64f34a40cc3d4f69dfd903be1ebb))
* **build:** add MSVC C11 flags and update repository metadata ([fcecdb0](https://github.com/casonadams/ripwire-rs/commit/fcecdb0dd991e92bc139b79e05a02522dc56ef22))
* **build:** normalize embedded skill paths to forward slashes for windows msvc ([5eba075](https://github.com/casonadams/ripwire-rs/commit/5eba075534d8009b86b201f8574ac1c0a9ad82e4))
* **ci:** configure cccc language filter, bench nextest listing, and test coverage ([809b406](https://github.com/casonadams/ripwire-rs/commit/809b4066fa6a225eba6f2da27d350a5ebb4834b4))
* **ci:** resolve scan-skills exit code, crap score, and test disk io ([9d88076](https://github.com/casonadams/ripwire-rs/commit/9d880767621ae4346bc28e848204185197a9bd49))
* **ci:** update tree-sitter-kotlin upstream branch to main ([644a486](https://github.com/casonadams/ripwire-rs/commit/644a486eaad1c72ae31e215f2fe67e735ef1a61c))
* **cli:** default top_k to 200 in dispatch_default_run matching upstream ripwire ([3dc265d](https://github.com/casonadams/ripwire-rs/commit/3dc265dc6f53be1e51e4a9e66267dad034bdc005))
* **cli:** match Ripwire exit 1 and error reporting on missing navigation symbols ([3e98f34](https://github.com/casonadams/ripwire-rs/commit/3e98f343b815cd5f4d70027c19c1e132c7eb77d3))
* **cli:** resolve CRAP threshold failures on navigation and query serialization ([1404301](https://github.com/casonadams/ripwire-rs/commit/14043014d61addeddeac96be0e863847d824e5cf))
* **cli:** resolve Round 3 DIRT.md retrieval, subdirectory path, and XML schema gaps ([3632a9f](https://github.com/casonadams/ripwire-rs/commit/3632a9fdd9719ce64beffd4cbc672b7f3e09cdd8))
* **clones,cli:** resolve target repository paths in clone detector and fix endpoint splitting in path query ([7083915](https://github.com/casonadams/ripwire-rs/commit/7083915ecac75b43747d4e014bccfdcfb7dcdea5))
* **gate:** validate git refs in merge-scout and fetch full history in CI parity validation ([7137f4d](https://github.com/casonadams/ripwire-rs/commit/7137f4dd1850b9f09192bcaaf28b4fbbac84f01d))
* **git:** implement recency-weighted ownership, bus factor, and real git-derived tool responses ([d742a40](https://github.com/casonadams/ripwire-rs/commit/d742a40cb085065d8a792b11d2dc2f205acf8479))
* **mcp:** decompose scan_local_branch_hits and remove obsolete quality acks ([f233e5a](https://github.com/casonadams/ripwire-rs/commit/f233e5a0f803de8aebd75d007e2c6786fc4c7d48))
* **parity:** achieve 1:1 DOM parity and wall/CPU/RSS superiority across all 67 commands ([a4e4ed8](https://github.com/casonadams/ripwire-rs/commit/a4e4ed80ed8d7bd65971b536899c37c97105bdba))
* **parity:** align exemplar selection, graph-query DSL, and owners output ([a558268](https://github.com/casonadams/ripwire-rs/commit/a558268107ccd9d4edebcf04eb2552ed3d9f5192))
* **parity:** complete 24-repo parity validation and optimize whereis CPU ([f5c7d8f](https://github.com/casonadams/ripwire-rs/commit/f5c7d8fb57a8bbb5e22976d7116adbf6ba39efdd))
* **parity:** implement streaming xml minifier, fix ci parity bash arithmetic, and resolve crap coverage ([01732a9](https://github.com/casonadams/ripwire-rs/commit/01732a95d9f631586fb8a9dea311a273d843aa91))
* **parity:** resolve CRAP complexity scores and enforce unambiguous path matching for --at ([8ab1d36](https://github.com/casonadams/ripwire-rs/commit/8ab1d36cb149302703ab0fb2fc4298e9b41b4261))
* **parity:** resolve DIRT.md git verbs, enforce yq xml validation, and implement missing flags ([10f0383](https://github.com/casonadams/ripwire-rs/commit/10f03831dbc493996575f775366d7a7b84aac9dd))
* **release:** normalize asset naming and tag format ([727ca4a](https://github.com/casonadams/ripwire-rs/commit/727ca4a6141982fea3f91d0885f005e40b2da6b7))
* **release:** package Unix manpage ([c29548e](https://github.com/casonadams/ripwire-rs/commit/c29548ec7f6aa825db962d3b372083b8288bdeb5))
* **skills:** ensure all skills trigger on ripwire and invoke rw binary ([462ad06](https://github.com/casonadams/ripwire-rs/commit/462ad063788f7dd051ec3ef849a555e7a1449c23))
* **www:** restore github repository link in navbar ([46b9703](https://github.com/casonadams/ripwire-rs/commit/46b97030a7f267dc3f508f904b4dd1715be6c8d8))
* **www:** update rust api snippet to ripwire-rs and align branding ([80441bc](https://github.com/casonadams/ripwire-rs/commit/80441bcf3602ae52817c90b32b3e0be87df2f749))


### Performance Improvements

* **cache:** accelerate warm queries and optimize clone candidate extraction ([d7b7dad](https://github.com/casonadams/ripwire-rs/commit/d7b7dada11b333cd50559e7f50695e95e715337c))
* **clones:** cache file bytes in candidate extraction and fast-path co-change capability ([4c23c25](https://github.com/casonadams/ripwire-rs/commit/4c23c257b42de54c811762dc5fb5289079591d33))
* **clones:** inverted index stop-gram filtering and lazy static coupling ([5eba504](https://github.com/casonadams/ripwire-rs/commit/5eba5049d3b1bb768d99715df31da05d2948ba63))
* **clones:** parallelize type-3 clone pair evaluation and direct-scan readability bytes ([c9dd57f](https://github.com/casonadams/ripwire-rs/commit/c9dd57f6ad4f031106177ce53a1c9cba2cb73076))
* **dispatch:** extract primary workflow dispatcher and improve test coverage ([9bec4f2](https://github.com/casonadams/ripwire-rs/commit/9bec4f2869278bb66ac4d8352c73f05d5d286531))
* **hits:** direct CSR slice indexing in HITS power iteration ([457aa92](https://github.com/casonadams/ripwire-rs/commit/457aa92255fa45a3fded2fec4489d2745749ae6b))
* **ingest:** optimize directory crawl, parser reuse, and reference span lookup ([b90f7c4](https://github.com/casonadams/ripwire-rs/commit/b90f7c43478a78e8cad733d43cb67364acc77f6a))
* **ingest:** parallelize symbol and reference extraction via prefix sums ([af6aa10](https://github.com/casonadams/ripwire-rs/commit/af6aa102f5c981f90f0c355431bb05f78b4bb8b4))
* **kgrams:** sort-dedup flat k-gram vectors and thread-local markdown parser ([0a6c9fe](https://github.com/casonadams/ripwire-rs/commit/0a6c9fed57ce4b7f7529e7eb0c3977eaa7262265))
* **metrics:** lazy git history walking and zero-allocation token stream ids ([1d115d0](https://github.com/casonadams/ripwire-rs/commit/1d115d0d382e2f13c420cd4a62b9aa100c78fffe))
* **rank:** precompute reciprocal out-degrees and direct index CSR gather ([a4c14ae](https://github.com/casonadams/ripwire-rs/commit/a4c14aeb3872994b8fcb04abf8face0dce8f61c2))
* **resolve:** stack candidate buffers, parallel reference resolution, and spur matching ([80647f5](https://github.com/casonadams/ripwire-rs/commit/80647f5a7174d4241ae299c8f120933bfc002bba))


### Documentation

* **sync:** update README, gates, workflows, man pages, and skills catalog ([02f7337](https://github.com/casonadams/ripwire-rs/commit/02f73373479121a01bc06fe3fabec3e8c290b043))
* **wrap:** document wrap command and synchronize docs with www ([f93f960](https://github.com/casonadams/ripwire-rs/commit/f93f960bfb58f5a6de7dc12d7109054e52b5090e))
* **www:** format CLI commands as structured cards with operational mechanics ([884e0e8](https://github.com/casonadams/ripwire-rs/commit/884e0e872256fff7b86104cb81294861d7a7418f))


### Miscellaneous Chores

* **docs:** align with code ([8842d11](https://github.com/casonadams/ripwire-rs/commit/8842d1159715faf3a9495228250db7ca880860ad))


### Code Refactoring

* **gate:** format pr_context and reduce cyclomatic complexity ([05f9b1f](https://github.com/casonadams/ripwire-rs/commit/05f9b1f81cb1c0000491bd3862ffdea4aef9e886))

## [0.4.2](https://github.com/casonadams/ripwire-rs/compare/ripwire-v0.4.1...ripwire-v0.4.2) (2026-10-14)


### Bug Fixes

* **analysis:** resolve Dart analysis gaps, enforce scm query contracts, and minify XML ([43793b0](https://github.com/casonadams/ripwire-rs/commit/43793b06d5ac64f34a40cc3d4f69dfd903be1ebb))
* **build:** normalize embedded skill paths to forward slashes for windows msvc ([5eba075](https://github.com/casonadams/ripwire-rs/commit/5eba075534d8009b86b201f8574ac1c0a9ad82e4))
* **cli:** resolve Round 3 DIRT.md retrieval, subdirectory path, and XML schema gaps ([3632a9f](https://github.com/casonadams/ripwire-rs/commit/3632a9fdd9719ce64beffd4cbc672b7f3e09cdd8))
* **www:** restore github repository link in navbar ([46b9703](https://github.com/casonadams/ripwire-rs/commit/46b97030a7f267dc3f508f904b4dd1715be6c8d8))
* **www:** update rust api snippet to ripwire-rs and align branding ([80441bc](https://github.com/casonadams/ripwire-rs/commit/80441bcf3602ae52817c90b32b3e0be87df2f749))

## [0.4.1](https://github.com/casonadams/ripwire-rs/compare/ripwire-v0.4.0...ripwire-v0.4.1) (2026-10-14)


### Bug Fixes

* **ci:** resolve scan-skills exit code, crap score, and test disk io ([9d88076](https://github.com/casonadams/ripwire-rs/commit/9d880767621ae4346bc28e848204185197a9bd49))
* **git:** implement recency-weighted ownership, bus factor, and real git-derived tool responses ([d742a40](https://github.com/casonadams/ripwire-rs/commit/d742a40cb085065d8a792b11d2dc2f205acf8479))
* **mcp:** decompose scan_local_branch_hits and remove obsolete quality acks ([f233e5a](https://github.com/casonadams/ripwire-rs/commit/f233e5a0f803de8aebd75d007e2c6786fc4c7d48))
* **parity:** achieve 1:1 DOM parity and wall/CPU/RSS superiority across all 67 commands ([a4e4ed8](https://github.com/casonadams/ripwire-rs/commit/a4e4ed80ed8d7bd65971b536899c37c97105bdba))
* **parity:** align exemplar selection, graph-query DSL, and owners output ([a558268](https://github.com/casonadams/ripwire-rs/commit/a558268107ccd9d4edebcf04eb2552ed3d9f5192))
* **parity:** complete 24-repo parity validation and optimize whereis CPU ([f5c7d8f](https://github.com/casonadams/ripwire-rs/commit/f5c7d8fb57a8bbb5e22976d7116adbf6ba39efdd))
* **parity:** implement streaming xml minifier, fix ci parity bash arithmetic, and resolve crap coverage ([01732a9](https://github.com/casonadams/ripwire-rs/commit/01732a95d9f631586fb8a9dea311a273d843aa91))
* **parity:** resolve DIRT.md git verbs, enforce yq xml validation, and implement missing flags ([10f0383](https://github.com/casonadams/ripwire-rs/commit/10f03831dbc493996575f775366d7a7b84aac9dd))


### Performance Improvements

* **clones:** inverted index stop-gram filtering and lazy static coupling ([5eba504](https://github.com/casonadams/ripwire-rs/commit/5eba5049d3b1bb768d99715df31da05d2948ba63))
* **metrics:** lazy git history walking and zero-allocation token stream ids ([1d115d0](https://github.com/casonadams/ripwire-rs/commit/1d115d0d382e2f13c420cd4a62b9aa100c78fffe))

## [0.4.0](https://github.com/casonadams/ripwire-rs/compare/ripwire-v0.3.1...ripwire-v0.4.0) (2026-10-13)


### Features

* **bench:** add cargo bench pipeline suite and Makefile bench targets ([6456639](https://github.com/casonadams/ripwire-rs/commit/64566396814fbc573d4c07b695eb1bd9adfa75d7))
* **bench:** add performance and resource benchmark target to Makefile ([d3afaf3](https://github.com/casonadams/ripwire-rs/commit/d3afaf3791591d07d51c8eb12f84631102ac24bc))
* **parity:** implement wrap command, arity filtering, and html canvas visualizer ([100a6de](https://github.com/casonadams/ripwire-rs/commit/100a6de86b328b7f61e28e9057db31fdf324ee5c))


### Bug Fixes

* **ci:** configure cccc language filter, bench nextest listing, and test coverage ([809b406](https://github.com/casonadams/ripwire-rs/commit/809b4066fa6a225eba6f2da27d350a5ebb4834b4))


### Performance Improvements

* **cache:** accelerate warm queries and optimize clone candidate extraction ([d7b7dad](https://github.com/casonadams/ripwire-rs/commit/d7b7dada11b333cd50559e7f50695e95e715337c))
* **dispatch:** extract primary workflow dispatcher and improve test coverage ([9bec4f2](https://github.com/casonadams/ripwire-rs/commit/9bec4f2869278bb66ac4d8352c73f05d5d286531))
* **ingest:** parallelize symbol and reference extraction via prefix sums ([af6aa10](https://github.com/casonadams/ripwire-rs/commit/af6aa102f5c981f90f0c355431bb05f78b4bb8b4))


### Documentation

* **wrap:** document wrap command and synchronize docs with www ([f93f960](https://github.com/casonadams/ripwire-rs/commit/f93f960bfb58f5a6de7dc12d7109054e52b5090e))

## [0.3.1](https://github.com/casonadams/ripwire/compare/ripwire-v0.3.0...ripwire-v0.3.1) (2026-10-12)


### Features

* **ingest:** add Objective-C and Objective-C++ support ([be0b0a2](https://github.com/casonadams/ripwire/commit/be0b0a2c03ee9338a3ff8750b36295422df565db))

## [0.3.0](https://github.com/casonadams/ripwire/compare/ripwire-v0.2.0...ripwire-v0.3.0) (2026-10-12)


### Features

* **arch:** implement --arch layering rules, --mermaid diagrams, and --zoom ([eebcb78](https://github.com/casonadams/ripwire/commit/eebcb783df86083847117b53756e46a284e11fb4))
* **cli:** add top-k, schema legend, test layering, and git-HEAD delta parity ([9d6368a](https://github.com/casonadams/ripwire/commit/9d6368abd8f7c6b01048e4c9d5ac2714adb7ebb7))
* **cli:** complete 1-to-1 feature and flag parity with ripwire engine ([3b44f34](https://github.com/casonadams/ripwire/commit/3b44f348e71d529c343bc13d102c59319e41fbe4))
* **cli:** complete 1:1 parity with CLI mutations, session tools, and advanced queries ([9f3907c](https://github.com/casonadams/ripwire/commit/9f3907c02ac268201bad19fcbfa74b32792d697f))
* **gate:** implement --map-diff, --seams, --pr-context, and --merge-scout ([396d729](https://github.com/casonadams/ripwire/commit/396d729967fdf9187be81b6a7845d27fc2ca342d))
* **gate:** implement short-horizon-churn, nesting, params, quality acks, and exit code 2 contract ([2c9db2f](https://github.com/casonadams/ripwire/commit/2c9db2f4f1ce31499c7fd182c9b9bb08a26339a6))
* **grammars:** add HTML and CSS tree-sitter grammars ([b7f1392](https://github.com/casonadams/ripwire/commit/b7f1392dc20d19c2874c92dc58f79f09f8289601))
* **grammars:** add Scala and Erlang tree-sitter grammars ([2fe8403](https://github.com/casonadams/ripwire/commit/2fe84035e677eb6fd5a8f4215fcab4c1c3e041a9))
* **grammars:** add SQL, Protobuf, GraphQL, and Dockerfile support, expanding to 35 languages ([c951c9a](https://github.com/casonadams/ripwire/commit/c951c9a6681da097940b5d0a3e4bff76cb3dc617))
* **grammars:** add Zig, Solidity, and HCL tree-sitter grammars ([5020560](https://github.com/casonadams/ripwire/commit/50205606f7f8f885aa5f942f5afc9b2504376010))
* **grammars:** expand language support to 31 languages across systems, cloud, functional, and web ([224fe31](https://github.com/casonadams/ripwire/commit/224fe3106447c9a0ad5d212eb763affc8659c2ad))
* **make:** add make validate and make parity targets with cross-repo validator ([07bd1cc](https://github.com/casonadams/ripwire/commit/07bd1cc911af447eac9c62af03138a855c6911ab))
* **man:** add manual pages for rw and ripwire with automatic cargo install ([beb21ab](https://github.com/casonadams/ripwire/commit/beb21abb27c6f6ed466890477819c7bebcae379c))
* **metrics:** implement --biggest-first, --nonlocal-state, --lint, and --lint-catalog ([8e0a923](https://github.com/casonadams/ripwire/commit/8e0a923b27e1cb48979d7bd7affb32131f468bea))
* **navigate:** implement --at, --verify, --pack-task, and --graph-query ([44a139a](https://github.com/casonadams/ripwire/commit/44a139afe44af00b964a97277ac9e456639650b0))
* **parity:** achieve 100% 1:1 behavioral parity with upstream ripwire across 36 commands ([83c7742](https://github.com/casonadams/ripwire/commit/83c77422750795658207ba1486215b331d0e03f5))
* **skills:** package and install 18 ripwire agent skills with --add-skills ([7affe30](https://github.com/casonadams/ripwire/commit/7affe30d01be7e479e4a1d7a2d5abb7f6bcc093f))


### Bug Fixes

* **cli:** default top_k to 200 in dispatch_default_run matching upstream ripwire ([3dc265d](https://github.com/casonadams/ripwire/commit/3dc265dc6f53be1e51e4a9e66267dad034bdc005))
* **clones,cli:** resolve target repository paths in clone detector and fix endpoint splitting in path query ([7083915](https://github.com/casonadams/ripwire/commit/7083915ecac75b43747d4e014bccfdcfb7dcdea5))
* **gate:** validate git refs in merge-scout and fetch full history in CI parity validation ([7137f4d](https://github.com/casonadams/ripwire/commit/7137f4dd1850b9f09192bcaaf28b4fbbac84f01d))
* **parity:** resolve CRAP complexity scores and enforce unambiguous path matching for --at ([8ab1d36](https://github.com/casonadams/ripwire/commit/8ab1d36cb149302703ab0fb2fc4298e9b41b4261))
* **skills:** ensure all skills trigger on ripwire and invoke rw binary ([462ad06](https://github.com/casonadams/ripwire/commit/462ad063788f7dd051ec3ef849a555e7a1449c23))


### Documentation

* **sync:** update README, gates, workflows, man pages, and skills catalog ([02f7337](https://github.com/casonadams/ripwire/commit/02f73373479121a01bc06fe3fabec3e8c290b043))


### Code Refactoring

* **gate:** format pr_context and reduce cyclomatic complexity ([05f9b1f](https://github.com/casonadams/ripwire/commit/05f9b1f81cb1c0000491bd3862ffdea4aef9e886))

## [0.2.0](https://github.com/casonadams/ripwire/compare/ripwire-v0.1.2...ripwire-v0.2.0) (2026-10-11)


### Features

* **cli:** expose all top-level Ripwire workflow flags and XML query serializers ([dce2f10](https://github.com/casonadams/ripwire/commit/dce2f105e8c2ec36ab7c1c196d231ebf9c83fec5))
* **navigate:** format navigation commands as canonical Ripwire XML ([e6ca76c](https://github.com/casonadams/ripwire/commit/e6ca76c8d0d8632a42671cc95423f6d3ec0e5a1c))
* **serialize:** align XML schemas and child tags with Ripwire canonical dialect ([5ed0c00](https://github.com/casonadams/ripwire/commit/5ed0c00dd6ad088dc001d87c0f1af9e56423c109))


### Bug Fixes

* **build:** add MSVC C11 flags and update repository metadata ([fcecdb0](https://github.com/casonadams/ripwire/commit/fcecdb0dd991e92bc139b79e05a02522dc56ef22))
* **cli:** match Ripwire exit 1 and error reporting on missing navigation symbols ([3e98f34](https://github.com/casonadams/ripwire/commit/3e98f343b815cd5f4d70027c19c1e132c7eb77d3))
* **cli:** resolve CRAP threshold failures on navigation and query serialization ([1404301](https://github.com/casonadams/ripwire/commit/14043014d61addeddeac96be0e863847d824e5cf))


### Performance Improvements

* **clones:** cache file bytes in candidate extraction and fast-path co-change capability ([4c23c25](https://github.com/casonadams/ripwire/commit/4c23c257b42de54c811762dc5fb5289079591d33))
* **clones:** parallelize type-3 clone pair evaluation and direct-scan readability bytes ([c9dd57f](https://github.com/casonadams/ripwire/commit/c9dd57f6ad4f031106177ce53a1c9cba2cb73076))
* **hits:** direct CSR slice indexing in HITS power iteration ([457aa92](https://github.com/casonadams/ripwire/commit/457aa92255fa45a3fded2fec4489d2745749ae6b))
* **ingest:** optimize directory crawl, parser reuse, and reference span lookup ([b90f7c4](https://github.com/casonadams/ripwire/commit/b90f7c43478a78e8cad733d43cb67364acc77f6a))
* **kgrams:** sort-dedup flat k-gram vectors and thread-local markdown parser ([0a6c9fe](https://github.com/casonadams/ripwire/commit/0a6c9fed57ce4b7f7529e7eb0c3977eaa7262265))
* **rank:** precompute reciprocal out-degrees and direct index CSR gather ([a4c14ae](https://github.com/casonadams/ripwire/commit/a4c14aeb3872994b8fcb04abf8face0dce8f61c2))
* **resolve:** stack candidate buffers, parallel reference resolution, and spur matching ([80647f5](https://github.com/casonadams/ripwire/commit/80647f5a7174d4241ae299c8f120933bfc002bba))


### Documentation

* **www:** format CLI commands as structured cards with operational mechanics ([884e0e8](https://github.com/casonadams/ripwire/commit/884e0e872256fff7b86104cb81294861d7a7418f))

## [0.1.2](https://github.com/casonadams/ripwire/compare/ripwire-v0.1.1...ripwire-v0.1.2) (2026-10-11)


### Features

* **cli:** implement categorized clap CLI and core MCP stdio server ([f40da01](https://github.com/casonadams/ripwire/commit/f40da015e1ac1ef7e7b08a39ea6f88d5b8cc6e2f))
* **clones:** implement AST clone detector with Type-1/2/3 matching and idiom demotion ([cf47f85](https://github.com/casonadams/ripwire/commit/cf47f85e908dc1889f2f02d0445118169e902404))
* **distribution:** rename crate to ripwire and binary to rw ([a840f28](https://github.com/casonadams/ripwire/commit/a840f2818da638e1007324c1ae00ce2ffead08c6))
* **domain:** implement SoA model and thread-safe string interner ([42ee6cf](https://github.com/casonadams/ripwire/commit/42ee6cfd59cc8aca67da2e135dafe9a7ab5f73b1))
* **gate:** implement pre-commit contract checker and affected test reachability mapper ([f28d1d1](https://github.com/casonadams/ripwire/commit/f28d1d1e7264dffcd6ed02eb749dc1102af71cda))
* **gate:** implement quality delta pre-commit gate and baseline snapshotting ([2744531](https://github.com/casonadams/ripwire/commit/274453162aa0823e68865b69adc6ff0c28ebc562))
* **git:** implement git commit history mining and shotgun surgery co-change detector ([815f95c](https://github.com/casonadams/ripwire/commit/815f95cf96c300e9ff3b89fc8bb4c3ae5e58a7b8))
* **graph:** implement reference resolution ladder and in-edge CSR construction ([aa9317f](https://github.com/casonadams/ripwire/commit/aa9317f883d2dbdad5ce7142eef57893f8ef8972))
* **ingest:** add polyglot crawler and tree-sitter extractors for Tier-1 languages ([a2ce0c5](https://github.com/casonadams/ripwire/commit/a2ce0c56bb2e631a4292aab37e95e3078443299d))
* **ingest:** add remaining 11 grammars and specialized language handlers for 24-language parity ([a1f2b30](https://github.com/casonadams/ripwire/commit/a1f2b303609a7b240e31ac1b0edb3da7d02d3d79))
* **ingest:** add systems and enterprise tree-sitter grammars ([3bc2790](https://github.com/casonadams/ripwire/commit/3bc279037d6635a85acc621ebea1c8fa4835e845))
* **mcp:** complete full 33 MCP tools suite and atomic edit operations ([182176f](https://github.com/casonadams/ripwire/commit/182176f4eb33bf89e466a2707ffde50503ed7f04))
* **metrics:** compute code complexity, readability, and maintenance hotspots ([4f8c866](https://github.com/casonadams/ripwire/commit/4f8c866433dff912826ba2b51ed2721229c19062))
* **packaging:** configure package include for crates.io publishing ([0a224eb](https://github.com/casonadams/ripwire/commit/0a224eb82f3e6f0db9635499e134f721a61766b7))
* **rank:** implement deterministic personalized PageRank with block reductions ([df73fea](https://github.com/casonadams/ripwire/commit/df73fea8d73e770648cbc79803762e055e04738a))
* **rank:** implement Kleinberg HITS algorithm and streaming token budgeting ([9e5c9d5](https://github.com/casonadams/ripwire/commit/9e5c9d5745f0e7eaad19c1867fcea250b9124d86))
* **resolve:** add scope extraction, call qualifiers, and callee deduplication ([919bdde](https://github.com/casonadams/ripwire/commit/919bdde2fed7c849260239b1d094f324d89eaa67))
* **serialize:** implement 64KB streaming minified XML serializer ([a18ae78](https://github.com/casonadams/ripwire/commit/a18ae78d12f134fefc317e23bd42e176f8e7930b))
* **serialize:** implement interactive HTML/SVG call graph export and Louvain community clustering ([0bf1e62](https://github.com/casonadams/ripwire/commit/0bf1e6242f67eb77d26ecb5e08bdfee33b8df94b))
* **slice:** implement ARISE intra-procedural def-use variable slicing and dataflow traversal ([a19506a](https://github.com/casonadams/ripwire/commit/a19506ad6b2979f881675c8599f733a5b95908ef))


### Bug Fixes

* **ci:** update tree-sitter-kotlin upstream branch to main ([644a486](https://github.com/casonadams/ripwire/commit/644a486eaad1c72ae31e215f2fe67e735ef1a61c))

## [0.1.1](https://github.com/casonadams/ripwire/compare/ripwire-v0.1.0...ripwire-v0.1.1) (2026-10-11)


### Features

* **cli:** implement categorized clap CLI and core MCP stdio server ([f40da01](https://github.com/casonadams/ripwire/commit/f40da015e1ac1ef7e7b08a39ea6f88d5b8cc6e2f))
* **clones:** implement AST clone detector with Type-1/2/3 matching and idiom demotion ([cf47f85](https://github.com/casonadams/ripwire/commit/cf47f85e908dc1889f2f02d0445118169e902404))
* **distribution:** rename crate to ripwire and binary to rw ([a840f28](https://github.com/casonadams/ripwire/commit/a840f2818da638e1007324c1ae00ce2ffead08c6))
* **domain:** implement SoA model and thread-safe string interner ([42ee6cf](https://github.com/casonadams/ripwire/commit/42ee6cfd59cc8aca67da2e135dafe9a7ab5f73b1))
* **gate:** implement pre-commit contract checker and affected test reachability mapper ([f28d1d1](https://github.com/casonadams/ripwire/commit/f28d1d1e7264dffcd6ed02eb749dc1102af71cda))
* **gate:** implement quality delta pre-commit gate and baseline snapshotting ([2744531](https://github.com/casonadams/ripwire/commit/274453162aa0823e68865b69adc6ff0c28ebc562))
* **git:** implement git commit history mining and shotgun surgery co-change detector ([815f95c](https://github.com/casonadams/ripwire/commit/815f95cf96c300e9ff3b89fc8bb4c3ae5e58a7b8))
* **graph:** implement reference resolution ladder and in-edge CSR construction ([aa9317f](https://github.com/casonadams/ripwire/commit/aa9317f883d2dbdad5ce7142eef57893f8ef8972))
* **ingest:** add polyglot crawler and tree-sitter extractors for Tier-1 languages ([a2ce0c5](https://github.com/casonadams/ripwire/commit/a2ce0c56bb2e631a4292aab37e95e3078443299d))
* **ingest:** add remaining 11 grammars and specialized language handlers for 24-language parity ([a1f2b30](https://github.com/casonadams/ripwire/commit/a1f2b303609a7b240e31ac1b0edb3da7d02d3d79))
* **ingest:** add systems and enterprise tree-sitter grammars ([3bc2790](https://github.com/casonadams/ripwire/commit/3bc279037d6635a85acc621ebea1c8fa4835e845))
* **mcp:** complete full 33 MCP tools suite and atomic edit operations ([182176f](https://github.com/casonadams/ripwire/commit/182176f4eb33bf89e466a2707ffde50503ed7f04))
* **metrics:** compute code complexity, readability, and maintenance hotspots ([4f8c866](https://github.com/casonadams/ripwire/commit/4f8c866433dff912826ba2b51ed2721229c19062))
* **packaging:** configure package include for crates.io publishing ([0a224eb](https://github.com/casonadams/ripwire/commit/0a224eb82f3e6f0db9635499e134f721a61766b7))
* **rank:** implement deterministic personalized PageRank with block reductions ([df73fea](https://github.com/casonadams/ripwire/commit/df73fea8d73e770648cbc79803762e055e04738a))
* **rank:** implement Kleinberg HITS algorithm and streaming token budgeting ([9e5c9d5](https://github.com/casonadams/ripwire/commit/9e5c9d5745f0e7eaad19c1867fcea250b9124d86))
* **resolve:** add scope extraction, call qualifiers, and callee deduplication ([919bdde](https://github.com/casonadams/ripwire/commit/919bdde2fed7c849260239b1d094f324d89eaa67))
* **serialize:** implement 64KB streaming minified XML serializer ([a18ae78](https://github.com/casonadams/ripwire/commit/a18ae78d12f134fefc317e23bd42e176f8e7930b))
* **serialize:** implement interactive HTML/SVG call graph export and Louvain community clustering ([0bf1e62](https://github.com/casonadams/ripwire/commit/0bf1e6242f67eb77d26ecb5e08bdfee33b8df94b))
* **slice:** implement ARISE intra-procedural def-use variable slicing and dataflow traversal ([a19506a](https://github.com/casonadams/ripwire/commit/a19506ad6b2979f881675c8599f733a5b95908ef))


### Bug Fixes

* **ci:** update tree-sitter-kotlin upstream branch to main ([644a486](https://github.com/casonadams/ripwire/commit/644a486eaad1c72ae31e215f2fe67e735ef1a61c))
