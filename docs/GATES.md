# Pre-Commit Safety Gates

`ripwire-rs` includes zero-configuration verification gates for pre-commit hooks, developer workflows, and CI pipelines.

Top-level gate verbs and the `gate` subcommand share CLI analysis and output
settings, including SCIP overlays, cache/crawl options, JSON, and legend removal.
For example: `ripwire-rs --json gate . --quality-delta` or
`ripwire-rs --legend=none --scip=index.scip gate . --edit-check=compute`.

CI runs `make ci`, the deterministic set: format check, clippy, the cccc complexity gate, the plugin registry check, and the tests (`make test`). Every input is pinned (Rust toolchain in `rust-toolchain.toml`, vendored grammars by commit in each `grammar.txt`, tool versions in the workflow) and no check reads git history, another branch, a downloaded binary, or machine state. `make all` runs everything in `make ci` plus the checks that depend on the environment: `quality` (regressions against git `HEAD`) and `parity` (the comparison with an upstream `ripwire` binary on this repository). Run `make all` before pushing.

---

## 1. Contract Checker (`--edit-check=SYM`)

Verifies whether an edited symbol's signature changed in parameters, arity, or public visibility compared to git `HEAD`. Detects and reports all 1-hop callers incompatible with the new contract.

```xml
<edit-check schema="ripwire-rs.edit-check/v1" sym="compute" t="fn" p="src/service.rs:10" status="contract-change" defs="1" callers="2" incompatible="1">
  <c n="legacy_worker" p="src/legacy_caller.rs:15" incompatible="1">
    <site line="18" args="2" required="3"/>
  </c>
  <c n="modern_worker" p="src/modern_caller.rs:20"/>
</edit-check>
```

- **Acknowledged**: a row the `.ripwire-rs_quality_acks` ledger acknowledges is suppressed entirely and counted in `acked=`.
- **Exit Code**: Non-zero if `incompatible > 0`.
- **Disclosures**: Explicitly reports argument counts and required parameter counts at each callsite.
- **Receivers**: a method's receiver (`self`, `&mut self`, Python `self`/`cls`) is not written at `recv.method(args)`, so `required=` excludes it there; a path call that writes it (`Type::method(recv, args)`) is compared against the full parameter list. A Python `@staticmethod` has no receiver.
- **Optional parameters**: a parameter with a default value (Python `x=1`, `x: int = 1`, C++ `int x = 1`, C# and PHP defaults, TypeScript `x?` and `x = 1`, Ruby `x = 1` and `k: 1`) is not required; a call binds any count from `required=` up to the full list. `*args`, `**kwargs`, `*xs`, `...xs` and Ruby `&blk` are open-ended or not positional, and a `*` or `/` separator takes no slot. Dart's bracketed and braced optional groups are all optional, including a `{required x}` member (so `required=` is a lower bound there). Kotlin and Swift do not expose a default-value field in their grammars, so their defaults count as required.
- **Binding**: a call is compared only when the resolver binds it to this definition alone; a call that binds another definition of the name, or several, is skipped. When the contract changed, a call the resolver could not bind is listed as `<site via="name">` if it passes a count HEAD's arity accepted and the current arity rejects; any other unbound call is not blamed on the edit.
- **Rows**: Every caller row is emitted; incompatible callers always print, and `--limit=N --offset=M` pages the compatible ones (`shown_unflagged=`, `total=`, `has_more=`, `next_offset=`).

---

## 2. Test Reachability Gate (`--affected=FILES`)

Maps modified files and symbols to transitively reachable test files across the in-process call graph.

```xml
<affected schema="ripwire-rs.affected/v1" changed="src/service.rs" seeded_by="file" seeds="1" seed_test_files="0" tests="2" reached="4" script_gates_unmodelled="0" order="evidence" partners="1">
  <test p="tests/test_service.rs" partner="1" hops="2" run_unknown="1"/>
  <test p="tests/test_billing.rs" hops="3" run_unknown="1"/>
</affected>
```

- **Ranking**: Test files are prioritized by hop distance and coupling evidence.
- **Transitive Reach**: Identifies tests that execute modified symbols even when located multiple calls away.
- **Partner Tests**: A test named after a changed file is listed with `partner="1"` even without a call edge (scripts run by a test harness): `test_<x>`, `<x>_test`, `<x>_spec`, `<x>.test`, `<x>.spec` in any language, plus `<X>Test` inside a `test/`/`tests/` dir. Each language states its convention through `LanguageSemantics::test_partner_subject`; `supports_test_partners` gates it. Every candidate is listed, even when several share a subject. A changed source file seeds partner tests even with no symbols; a changed test with no symbols gets no row. A changed test that is also a partner keeps its row and gets `partner="1"`. Runner-less rows that agree on every flag fold into one `<g n= p=>` row. `--test-gate` exits 4 while any test row exists.
- **Assertion Scripts**: `x.check.ts`, `x.check.tsx`, and `x.check.js` are test-like files named after the subject `x` (`LanguageSemantics::is_assertion_script`, `test_partner_subject`). They are partner tests of `x` and count as tests in graph reach. A script that only imports a changed source file is listed one hop away when it is an assertion script (static import reach); other importers of the file are never classified as tests. These rows are static reachability: a listed script is not proof it executed, and an assertion script run by its own header command (`pnpm dlx tsx <file>`) carries `run_unknown="1"`.

---

## 3. Quality Delta Gate (`--quality-delta`)

Enforces regression-free commits across 10 architectural quality dimensions against git `HEAD` or a pinned baseline (`--quality-baseline`); `baseline="sidecar"` with `sidecar="<file>"` names the pinned file read, and `baseline="git-HEAD"` means none was found. `--quality-delta=REV` (or `REV..HEAD`) instead rebuilds the baseline from the merge base of `REV` and `HEAD`, ignoring any sidecar, and reports `baseline="git-ref" base_ref="<sha>"`: the delta of everything since that base, for CI or review bots that check out a fresh head. The value needs `=`, so `--quality-delta .` still names the path:
1. **Cognitive Complexity** ($CCX \le 15$ Campbell standard)
2. **Cyclomatic Complexity** (McCabe paths)
3. **Function Length / Line Count**
4. **AST Duplication / Clones** (Type-1, Type-2, Type-3)
5. **Nesting Depth**
6. **Parameter Count**
7. **Dead Code Detection** (functions and methods, in any language, that nothing reaches)
8. **Short-Horizon Churn Spans** (frequently touched code regions)
9. **New Public API Surface**
10. **Baseline Regression Tracking** (supports `.ripwire-rs_quality_acks` suppression ledger)

```xml
<quality-delta schema="ripwire-rs.quality-delta/v1" baseline="git-HEAD" regressions="0" minor="0" acked="0" stale="0" preexisting-worse="0" new-symbol="0" gating="0" api-new-surface="0" at="9e5c9d5+dirty">
</quality-delta>
```

- **Radical Honesty**: Discloses working copy status (`+dirty`), new public API surface, and separates non-blocking new-symbol debt from blocking pre-existing regressions.
- **Dirty Pins**: `--quality-baseline` refuses on a dirty tree that already gates against `HEAD`; `--allow-dirty` pins anyway, stamps the absorbed count, and later deltas carry `baseline_absorbed="N"` (green then means clean since the pin).
- **Stale Acks**: a regression-free delta collapses stale `.ripwire-rs_quality_acks` rows to `stale_by_kind=` plus a `stale_next=` replay; `--show-stale` prints every row.
- **Renames**: a callable that disappears from a changed file and a callable that appears in one are one symbol renamed when both are functions or methods of the same kind and parameter arity, and their bodies match under the clone detector's rule: an identical normalized body, or Type-3 similarity at or above the clone floor (the stricter floor for short bodies). Pairing is one-to-one; a tie or an ambiguous candidate leaves a removed symbol and a new one. A renamed symbol is compared with its old values, so worsening gates exactly as it would without the rename, and its row carries `renamed_from="old"`. A rename with no worsening yields no row, and an acknowledgement keyed by the old name carries over to the new one (`acked_by_rename="N"` counts the carried acknowledgements). `renames="N"` on the root counts the pairs. A renamed public function is not new API surface, so `api-new-surface` does not count it, and it is not reported as removed either: this gate has no removed-API row, so the old name's disappearance shows only through `renames=` and `renamed_from=`. Limits: pairing reads bodies from the HEAD baseline, so a pinned `--quality-baseline` sidecar (metrics only) pairs no renames; untracked files are outside the changed set, so a move into one is not paired.
- **Materiality**: numeric rows are judged against `bar=` (complexity 15, verbosity 60, nesting 4, params 5), as upstream ripwire does. On a symbol that existed at the baseline, crossing the bar or growing by at least a quarter while above it is major and gates, unless the step is a single unit; a smaller step above the bar is `sev="minor"`; below the bar only a doubling that reaches two thirds of it is reported, as minor. A new symbol is reported once it is above the bar and never gates.
- **Dead Code**: a function or method with a body is dead when it has no resolved caller, no reference anywhere uses its name (a declined or unresolved call, a read, a value, a macro argument), and its name appears as no other identifier token in the tree. In Python a bare mention does not count when its own file defines the name, or binds it as a local variable or parameter in a scope around the mention. A bare mention in a file that imports another definition of the name, and not this one, does not count either. A reference the resolver bound to another definition by lexical or import evidence (not by name alone, and not a call on a receiver) does not keep a same-named free function alive; methods are exempt because a call through a protocol or base class reaches every implementation. A reference in another language never keeps a Python function alive. Python imports resolve relative levels (`..x`) and absolute modules rooted at any directory above the importer, so a service's `src/` package root binds. `from pkg import mod` binds calls written `mod.f()` when `pkg.mod` is an indexed module. A bound Python reference still keeps a symbol of another language with the same name alive. TypeScript and JavaScript follow the same rules, with `const`/`let`/`var` declarators, parameters, `catch` and `for` targets as local bindings (a `require()` or `await import()` declarator is an import, not a local). Functions in Next.js app-router files (`page`, `layout`, `route`, ... under `app/`, and `middleware`) are entrypoints, and a file loaded with a dynamic `import()` has reached only the exports the importer uses: `lazy(() => import(..))` takes `default`, `.then((m) => m.Foo)` and `const { Foo } = await import(..)` take `Foo`, and a module used opaquely reaches every export. `let`, `const` and `catch` bindings are scoped to their block. A string literal that is a SQL statement (it starts with `SELECT`, `WITH`, `CALL`, `PERFORM`, `INSERT`, `UPDATE` or `DELETE`) makes each `name(` in it a call of the SQL function `name`. So does `client.rpc("name", ..)` in TypeScript and JavaScript. A reference bound to a definition in one language no longer keeps a same-named symbol of another language alive. Entrypoints, constructors, dunder protocol methods, callables a decorator registers (a route, a hook, a task; not `@staticmethod`-style wrappers), methods a supertype dispatches to (inside a trait implementation, or named by an in-tree supertype, or any member when the supertype is outside the tree), methods with no enclosing type (object-literal members) and test files never count; inherent methods of a type that also implements a trait are checked. A symbol that existed at the baseline and had a caller there gates, including when its last caller was removed in another file.
- **Clones**: groups of at least 40 normalized tokens (the `--clones` default; smaller bodies match by shape alone) are reported only when one of its members is new or has a changed body; `members=` lists the group and `tokens=` its shared token count. It is `origin="new-symbol"` only when every member is new, and otherwise gates. An idiom collision is reported `sev="minor"` with `idiom=` instead of gating: every member spells one recognized shape (`threshold-ladder`, `switch-name-table`, `builder-chain`), the group is under 80 tokens, the members sit in distinct enclosing contexts, and no two share an identifier. A group that copies an untouched helper with two or more callers adds a gating `new-clone-of-reused-helper` row whose `now=` is that helper's caller count.
- **Exit Code**: Returns exit code 2 if blocking quality regressions exist (`gating > 0`).

---

## 4. Test Obligation Gate (`--test-gate`)

The definitive pre-commit verification gate combining changed symbols, affected test discovery, and untested blast radius analysis.

```xml
<test-gate schema="ripwire-rs.test-gate/v1" changed="1" impacted="12" tests="1" untested="1" untested_modscope="0" shown_tests="1" tests_capped="0" shown_untested="1" untested_capped="0" script_gates_unmodelled="0" ccx_bar="15" run_first="1" at="6e9b85eb8" root="." next="python3 tests/test_service.py">
  <t p="tests/test_service.py" partner="1" run="python3 tests/test_service.py"/>
  <u sym="new_helper" p="src/service.py" l="42" ccx="9"/>
</test-gate>
```

- **Exit Code**: Returns exit code 4 while any `<t>` test row or `<u>` untested impacted symbol row exists.
- **Runners**: Derives `run=` from the test file's language (`python3` when `__main__` is present, `bash` for `.sh`, `npx vitest run` / `npx jest` from the nearest `package.json`, `node --test` when `require('node:test')` is present) and emits `run_unknown="1"` otherwise. An assertion script (`x.check.ts`, `.tsx`, `.js`) takes the command its leading comment block documents (`pnpm dlx tsx`, `npx tsx`, `tsx`, `node`, `bun`, or `deno run` followed by its file) only when that file is the script itself, named by repo-relative path or basename; the derived `run=` uses the repo-relative path. Without such a header it stays `run_unknown="1"`: no runner is guessed.
- **Untested Blast Radius**: `<u>` rows list impacted non-test symbols that no test transitively reaches, sorted by cognitive complexity descending and paged by `--limit`/`--offset`; `<file-scope>` owners are counted separately in `untested_modscope=`.

---

## 5. Integration Seams Gate (`--seams`)

Identifies call edges crossing module or directory boundaries that have zero direct test coverage.

```xml
<seams schema="ripwire-rs.seams/v1" root="." seams="12">
  <seam caller="src/api::handle" callee="src/db::query" cross="api->db"/>
</seams>
```

- **Document Links**: `doc_links=N` counts Markdown links from a prose document to another indexed prose document in a different directory (anchors allowed; scheme links, same-directory links and links to unindexed files are not counted). Data-key links, which upstream also counts, are not modelled, so the number covers Markdown links only.
- **Architecture Audit**: Pinpoints vulnerable boundary interfaces before integration testing.

---

## 6. Map Diff Gate (`--map-diff`)

Re-ranks the architecture map toward git-modified files so recent work automatically floats to the top of the context window.

```bash
ripwire-rs . --map-diff --top-k=30
```

- **Focus**: Keeps LLM context windows concentrated on currently active development tasks without losing structural context.

---

## 7. Composite PR Context (`--pr-context=BASE`)

Assembles a complete pull request review context bundle for the changed files. `<changed-symbols>` lists the symbols that own a changed line (the innermost definition spanning it; a pure deletion counts for the definition holding the lines on both sides), not every symbol of a changed file. `<impact>` lists caller files outside the changed file as `<f p= deps=>` and `<tests>` lists `<test p= run=>` rows (`run_unknown="1"` when no runner is derivable); each list shows at most 5 rows with `shown=`/`capped=` while the counts cover all. `--token-budget=N` budgets the file rows at 2.75 bytes per token (measured with the o200k tokenizer), so `budget_tokens=` echoes N; `est_tokens=` is measured from the emitted rows (an upper bound under minified output). `<owners bf=>` is the `--owners` single-owner flag: 1 when one author holds at least 80% of the file's recency-weighted commits. Changed files are the working tree compared with the merge base of REF and `HEAD` (`--pr-context=REF`, `direction="worktree-since-merge-base"`, `base="REF"`). Without REF they are tracked edits against `HEAD` (`base="working-tree"`). A value that resolves as no revision but names a directory is the repository path, so `--pr-context .` reads the working tree of `.`; a value that is both stays a revision. An unknown REF, or a root outside a git repository, exits nonzero. A file with no commit history gets no `<cochange>` or `<owners>` element, and `at` (the HEAD short sha, with `+dirty` when the tree is dirty) is omitted outside git.

```xml
<pr-context schema="ripwire-rs.pr-context/v1" base="main" files="2">
  <file p="src/service.rs" symbols="14">
    <changed-symbols count="1">
      <s n="execute"/>
    </changed-symbols>
    <impact dependents="5" files_other="2">
      <f p="src/api.rs" deps="3"/>
      <f p="src/cli.rs"/>
    </impact>
    <tests count="2">
      <test p="tests/service.rs" run="cargo test --test service"/>
      <test p="tests/api.rs" run="cargo test --test api"/>
    </tests>
    <cochange commits="12" partners="1"/>
    <owners authors="2" bf="1"/>
  </file>
</pr-context>
```

- **One-Turn Review**: Combines changed symbols, transitive impact, tests to run, co-change partners, and authorship in a single query.

---

## 8. Branch Merge Scout (`--merge-scout=REF1,REF2,...`)

Symbol-level overlap of divergent refs against HEAD, read-only and in-process through gix (no checkout, no git subprocess per file). For each REF:

- `base`: `merge-base(HEAD, REF)`. A ref that shares no history with HEAD reports `ok="0"` and is not compared. An unknown ref is an error (exit 1).
- `changed`: the indexed symbols (functions, methods, classes, structs, interfaces, macros, sections) whose lines the diff from the base to REF touched. Each side is indexed at its own version: the old side names what REF removed or rewrote, the new side what it added or rewrote. A changed line belongs to its innermost symbol, so editing a method names the method, not its `impl`. Constants, fields and locals name no symbol; their file still counts in `files`.
- `head_conflicts`: of the changed symbols, those HEAD also changed since the same merge base.
- `<no-work/>`: REF changed no path since its merge base. It takes no landing slot.
- `<pair>`: one for every two arms that ran, work or not (as upstream pairs them). `conflicts` counts the symbols both changed (listed as `<sym>`, a real merge fights over them); `risks` counts files both changed in which the two arms' changed symbols do not overlap (same file, different symbols: a textual merge may still need a look).
- `<landing order="…"/>`: arms with work, fewest conflicts first (conflicts with HEAD and with other arms), then the smaller change (symbols, then files), then request order. `order=""` when no arm has work.
- A dirty checkout (`git status` lists a tracked change or an untracked, non-ignored file) adds one arm last, `working-tree`: its tracked changes against HEAD, read from disk. Untracked files are not work, so an arm of only untracked files is `<no-work/>`.
- `head` is the short HEAD sha; `at` is that sha plus `+dirty` for a dirty checkout and `+shallow` for a shallow clone, where merge bases may be missing.
- Symbol rows are windowed by the default pack budget per list (`shown=`/`capped=` on the enclosing element); every count is exact.

Real output for three refs against HEAD 81550c4 (arm A changed branch_verdict and classify_file, arm B changed classify_file, arm C changed line_counts; the refs are named armA, armB and armC):

```xml
<merge-scout schema="ripwire-rs.merge-scout/v1" arms="3" head="81550c4ef" at="81550c4ef"><arm ref="armA" base="358264c5f" ok="1" changed="2" head_conflicts="0" files="1"><sym p="src/query/stray_lines.rs" n="branch_verdict"/><sym p="src/query/stray_lines.rs" n="classify_file"/></arm><arm ref="armB" base="358264c5f" ok="1" changed="1" head_conflicts="0" files="1"><sym p="src/query/stray_lines.rs" n="classify_file"/></arm><arm ref="armC" base="358264c5f" ok="1" changed="1" head_conflicts="0" files="1"><sym p="src/query/stray_lines.rs" n="line_counts"/></arm><pair a="armA" b="armB" conflicts="1" risks="0"><sym p="src/query/stray_lines.rs" n="classify_file"/></pair><pair a="armA" b="armC" conflicts="0" risks="1"/><pair a="armB" b="armC" conflicts="0" risks="1"/><landing order="armC,armB,armA"/></merge-scout>
```

Parity: `--merge-scout=main` (with the base resolved from `main`, `master`, `origin/main` or `origin/master`, in that order) is compared with upstream on the base itself, which has no work and so matches upstream's `<no-work/>` shape. Refs with work are not covered by `make parity` (the fixture runner writes no git history), so their symbol rows are verified by hand, not measured against upstream.

---

## 9. Architectural Layering Rules (`--arch=RULES`)

Enforces directional dependency constraints between architectural layers from a simple configuration file.

```text
# rules.txt
layer domain <= core
layer core <= api
deny api -> domain
```

```bash
ripwire-rs . --arch=rules.txt
```

- **Exit Code**: Returns exit code 2 if any forbidden cross-layer dependency edge is introduced.

---

## 10. Situational Awareness (`--situ`)

Emits canonical `ripwire-rs.situ/v1` XML summarizing a working tree diff: blast radius, tests to run, forgotten co-change partners, maintenance hotspots touched, and modules modified. The MCP `situational_awareness` tool returns the same report in `result.content[0].text`.

---

## 11. Enforcing the Gates with a Hook

An instruction asking to run the gates gets skipped; a hook does not. A hook runs a command
*before* `git commit` and blocks it on a non-zero result.

For oh-my-pi (`omp`), the [`gate` plugin](https://github.com/casonadams/omp-plugins/tree/main/plugins/gate)
does this:

```bash
omp plugin marketplace add casonadams/omp-plugins
omp plugin install gate@casonadams-plugins
```

It runs `ripwire-rs` (falling back to `ripwire`) over what differs from `HEAD`, one step at a time, and stops at
the first step that fails:

1. `--edit-check` for each public symbol the diff adds or changes. Blocks when a contract changed and a caller
   no longer fits.
2. `--quality-delta`. Blocks when it exits `2`.

Once both are green it lists, without blocking, the untested rows from `--test-gate` and the files `--situ`
reports as usual co-changes. Run `--quality-baseline` at the start of a task or `--quality-delta` has nothing
to compare with. `OMP_GATE_DISABLE=1` turns the plugin off.

Any tool with a hook that runs before a shell command can do the same: trigger on a command that runs
`git commit`, run the steps in order in the repository being committed, stop at the first non-zero result, and
block with that step's output. The plugin's source is a reference implementation.

---

## 12. Parity Ledger (`make parity`)

`scripts/validate-parity.py` compares every command, and the gate runs on explicit changed sets, against upstream ripwire. A child-tag count difference or a structural mismatch is a divergence and must be recorded in `scripts/parity-divergences.toml`, except paged row lists: a root carrying `total`, `has_more`, or `next_offset` follows one generic rule with no entry. That rule requires the same schema, `total`/`count` equal to upstream's, every shown row (keys `p`/`n`) present in upstream's full `--limit` run, rows in our ranking order, and bytes at or below upstream's or richer by density. Structure (root attributes, row attributes, child tags) of a paged list is judged on both tools' full `--limit` runs, because a default window is a byte budget and says nothing about which attributes a row can carry; default answer sizes are held by `make size` (`scripts/output-budgets.toml`). An entry whose run no longer diverges is stale and fails. Kinds: `smaller` (our bytes <= upstream), `richer` (our facts > upstream and facts-per-byte >= upstream), `fixes-upstream-defect` (evidence names an existing regression test), `defect` (our known defect: printed as a warning and counted; `defect_budget` in the ledger header must equal the count). Fixture entries run in a temp directory outside the tree.

Root attributes: every entry's root attribute keys must equal upstream's (outside the optional set), except `richer` and `smaller` entries, which may also carry ours-only root attributes as disclosure but must never miss an upstream one. A `defect` entry gets the same structural checks as any entry, so a structural mismatch fails the run instead of hiding behind the warning. Checker unit tests: `python3 scripts/test_validate_parity.py`.

Declared omissions: a `smaller` entry may carry `omits = ["row:attr", "element", ...]` to name upstream attributes and elements that ours deliberately does not emit. A `row:attr` token names the upstream child row by its `n=` value (for example `"cache-dir:locks"`). The checker exempts exactly those tokens and nothing else: an omitted attribute counts only when that upstream row would otherwise fail and passes without it, a bare element token must be absent from our output, and every token must be needed. An entry that declares `omits` is checked exactly: any upstream element that ours lacks must be listed, and any listed token that ours does not need fails as stale. Each omitted name must be named in `why` with its reason; `omits` on any other kind fails the ledger load.

Upstream timeout: the upstream run of each command gets 4 seconds. A command that exceeds it is not compared and the checker prints a `note:` line, and a ledger entry for it then reports as unmeasurable. Upstream keeps a cache directory (`--doctor` reports it) that grows with use and slows its own `--doctor`; set `PARITY_UPSTREAM_TIMEOUT=SECONDS` to measure such commands on a loaded machine. Parity is not part of CI because it compares a moving target (the installed upstream release and this repository's own history) and so cannot be deterministic.

## 13. Resolver Corpus (`make corpus-*`, local only)

Call resolution scored against an oracle over real public repositories. Two oracle kinds exist. A **SCIP indexer** is a compiler's own index (rust-analyzer, scip-typescript, scip-python, scip-ruby, scip-dart, scip-java for Java, Kotlin and Scala, scip-clang, scip-dotnet, and scip-php). An **LSP bridge** (`scripts/corpus/lsp2scip.py`) asks a language server's go-to-definition for each conservative `name(` call site and writes a SCIP index from the answers. Precision is the headline metric for bridge entries: recall is measured only over the call sites the bridge enumerates, so a bridge recall is lower-confidence than a compiler index recall. Not in `make ci` or `make all`; it needs Docker and network.

**Daily use.** Setup once: `make corpus-image` and `make corpus-image-lsp` (Docker, linux/amd64, 3 GiB per indexer), then `make verify-warm` builds the missing indexes into `~/.cache/ripwire-corpus`. Then:

- `make verify` scores every entry with a cached index, plus the Rust self-check (`scripts/check-resolution.py`), against `scripts/corpus/baseline.toml` and the floors in `corpus.toml`. It prints one table and one verdict, and exits 1 on a drop beyond 0.0005 or a floor miss. An entry with no cached index is skipped loudly.
- `make verify-changed` runs only the entries whose languages `git diff HEAD` and untracked files touch (`src/languages/<lang>/`, `tests/languages/<lang>*.rs`; a change to other `src/` code touches every entry). `LANG=ruby` or `NAME=rack` narrow any target.
- `make verify-diff [OLD=path]` adds the sites that flipped right->wrong against an older binary, with ours and scip for each. The default baseline is the binary that `make verify-keep-bin` kept.
- `make verify-watch LANG=dart` rebuilds the fast binary on each `src/` change and prints only the entries whose numbers moved. `make verify-status` shows stale or missing indexes, Docker and image state, and cache size.
- `make verify-ratchet` records the measured numbers in `baseline.toml` and raises the floors to the measured value minus 0.002. It only raises: it writes nothing, and refuses, when any number dropped or any entry failed.
- To add or retire a repository, see [Adding a repository](#adding-a-repository).

Reference: `make corpus-index`, `corpus-eval`, `corpus-measure` and `corpus-diff OLD=path/to/old/ripwire-rs` (per-site verdict changes between two binaries) remain for single-purpose runs; `NAME=<entry>` limits any target to one entry.

An entry whose language server cannot run under emulation sets `platform = "linux/arm64"` and a native image (`make corpus-image-swift-arm64`, `make corpus-image-godot-arm64`); the `platform` field is documented in `scripts/corpus/corpus.toml`.

Measured on a release build of this tree. Ranges span the entries of a language; a single value is one entry. Floors are set for the LSP-bridge entries (just under measurement); SCIP entries keep their floors until ratcheted.

| Language | Oracle | Status | Precision | Recall |
|---|---|---|---|---|
| bash | LSP bridge (bash-language-server) | measured | 0.9983 | 0.9960 |
| brightscript | LSP bridge (brighterscript 0.73.8 `bsc --lsp --stdio`, rodash) | measured | 1.0000 | 1.0000 |
| c | SCIP indexer (scip-clang) | measured | 1.0000 | 1.0000 |
| cobol | LSP bridge (COBOL language server on a JRE) | blocked: analysis fails with NPE `dialects is null` even when the settings answer `[]`; expected settings shape undetermined | - | - |
| cpp | SCIP indexer (scip-clang) | measured | 0.9376 | 0.8100 |
| csharp | SCIP indexer (scip-dotnet) | measured | 0.8448-0.9882 | 0.8448-0.9796 |
| cuda | LSP bridge (clangd) | measured: 307 covered call sites of 11541. Of 66 false edges, the sample shows resolver name binding (qualified `layout::Data(...)` constructor calls bound to the type, same-named members, macros); the bridge pins the constructor, so such calls count against precision | 0.5796 | 1.0000 |
| dart | SCIP indexer (scip_dart) | measured | 0.9726 | 0.7593 |
| elixir | LSP bridge (elixir-ls) | blocked: elixir-ls mix install fails on Elixir 1.18.3 (vendored dependency does not compile) | - | - |
| erlang | LSP bridge (ELP 2026-10-14) | blocked: the ELP tarball has no pinned sha256 and its download was blocked; the repo has 34 source files | - | - |
| fortran | LSP bridge (fortls) | measured | 0.9980 | 0.9845 |
| gdscript | LSP bridge (Godot 4.7.2 language server) | blocked: a native linux/arm64 build also answers none of 1067 definition requests in 60 minutes (emulation is not the cause) | - | - |
| go | LSP bridge (gopls) | measured | 0.9534 | 0.9586 |
| graphql | none wired | not attempted: no MIT repo with fragment-spread references found yet | - | - |
| hcl | not applicable: reference edges are builtin function calls only | not applicable | - | - |
| java | SCIP indexer (scip-java) | measured | 0.9650 | 0.9138 |
| javascript | SCIP indexer (scip-typescript) | measured | 1.0000 | 1.0000 |
| json | not applicable: no reference captures | not applicable | - | - |
| kotlin | SCIP indexer (scip-java, Kotlin) | measured | 0.9132 | 0.8170 |
| lua | LSP bridge (lua-language-server) | measured | 0.9817 | 0.9553 |
| markdown | not applicable: no call resolution | not applicable | - | - |
| metal | LSP bridge (clangd, compiled as C++ with Metal keywords stubbed) | measured | 0.9590 | 0.9590 |
| objc | LSP bridge (clangd; candidates include message sends) | measured | 0.7429 | 0.6500 |
| php | SCIP indexer (scip-php) | measured | 0.9367-0.9838 | 0.8792-0.9794 |
| proto | not applicable to --eval=scip: type references are not scored | not applicable | - | - |
| python | SCIP indexer (scip-python) | measured | 0.9860 | 0.9160 |
| ruby | SCIP indexer (scip-ruby) | measured | 0.9864 | 0.9943 |
| rust | SCIP indexer (rust-analyzer, this repository) | measured | 0.9868 | 0.9789 |
| scala | SCIP indexer (scip-java, Scala) | measured | 0.9017 | 0.8439 |
| solidity | LSP bridge (solidity language server) | measured | 0.9576 | 0.8966 |
| sql | not applicable to --eval=scip: table and column references are not scored | not applicable | - | - |
| swift | LSP bridge (sourcekit-lsp 6.1; native linux/arm64 image, scripts/corpus/Dockerfile.swift) | blocked: the package builds natively, then sourcekitd crashes in IDE inspection of one ArgumentParser file (Swift 6.1 compiler crash in overload solving) | - | - |
| toml | not applicable: no reference captures | not applicable | - | - |
| typescript | SCIP indexer (scip-typescript) | measured | 0.9882 | 0.9882 |
| yaml | not applicable: no reference captures | not applicable | - | - |
| zig | LSP bridge (zls) | measured | 0.8857 | 0.8759 |

Not covered: languages marked not attempted or blocked above have no floor and no measurement; a corpus floor says nothing about a language it does not index. Languages covered only by in-memory fixtures remain so until an entry measures them.

**Dataform (`.sqlx`).** The SCIP scorer only sees references the parser extracted, so it cannot see a read the parser
missed. `scripts/corpus/dataform_eval.py` compares edge sets instead: the truth is what `dataform compile --json`
implies each model reads (compiled dependencies, plus any compiled table name found in a query outside comments),
the found set is what `--uses` reports. It prints precision, recall and the missed and false edges, needs Node, and is
run by hand on a checkout (`--root DIR --bin ripwire-rs`); it has no floors or baseline row yet. A model name or
`ref()` argument written `dataform.projectConfig.vars.KEY` is resolved from the nearest `workflow_settings.yaml` or
`dataform.json`; any other computed name stays unresolved and shows up as `tables_without_one_definition`.

### Adding a repository

```bash
make corpus-add URL=https://github.com/OWNER/REPO LANG=python   # license, SHA and file count checks; writes an unverified entry
make corpus-index NAME=<name> && make corpus-measure NAME=<name> # index and score it (Docker)
make verify-ratchet                                              # record the numbers and raise the floors
```

`corpus-add` refuses a license outside MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unlicense and 0BSD, a
SHA that is not a full commit, fewer than 10 source files, and a language whose template is blocked. It warns
outside 50 to 300 source files; `SUBDIR=path` indexes one directory instead. `MEASURE=1` indexes and scores in one
step and writes the floors on success (the language's matrix row is updated); `DRY_RUN=1` prints the entry and
changes nothing. `make corpus-remove NAME=<name>` retires an entry with its cache, index and baseline row
(`YES=1` skips the prompt). Templates live in `scripts/corpus/languages.toml`; `corpus.py add --list-languages`
lists them with the blocked and no-indexer languages. Repo names and SHAs stay in `scripts/corpus/corpus.toml`.


