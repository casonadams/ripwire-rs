# ripwire MCP reference — edit-verb safety contract + server internals

Reference for `ripwire-rs-mcp`. Load this when you're about to call one of the 3 edit verbs (need the exact
failure-mode contract) or need to reason about server staleness/rebuild/personalization internals.

## The 3 MCP edit verbs — the warm-server counterpart to the preferred CLI

`replace_symbol_body`, `insert_before_symbol`, `insert_after_symbol` are
ripwire's MCP WRITE verbs. The preferred shell front door is
`ripwire-rs ROOT --replace-symbol-body=SYM --edit-payload=FILE|-` (or the matching insert flag); use MCP when
the server is already warm or a client has no shell. Both fronts call the same safety engine. Each locates
a symbol's definition in the parsed index and splices text at its byte span: `replace_symbol_body`
swaps signature-through-closing-brace verbatim, `insert_before_symbol`/`insert_after_symbol` splice text at
the def's first/final byte (auto-adding the separating newline so you don't have to guess).

**The receipt already answers "what now?"** — it carries `lines={start,end}` for the applied text,
`edit_check` (status / callers / incompatible / each broken caller's call lines) and `tests_to_run` with the
run recipe: the same answers a separate `edit_check` verb and `--affected` would give, computed on the index
the edit just refreshed. Do not spend two more calls on them. `post_check:false` (CLI `--no-post-check`)
opts out — reach for it only when the next thing you do is another edit to the same tree.

Read **[`mcp-reference.md`](mcp-reference.md)** before calling one of these 3 (full args table + the exact
newline rule + the safety contract for not-found / ambiguous / stale-index / insane-span failures + the
atomic-write guarantee) or when you need server internals (`_index` staleness
stamps, warm content-hash rebuilds, working-set PageRank personalization, the background `qsnap` HEAD-warm on
large workspaces). Short version: every edit-verb failure leaves the file byte-for-byte unchanged, writes are
atomic (`tmp` + `rename(2)`), and the server never goes stale silently — before every tool call it compares
git HEAD plus the mtime/size of dirty and untracked files with the indexed state and re-indexes on change. **Lead with this MCP form when the body already came from `fetch_body` this
session**; after a CLI `--expand`, keep the zero-standing-schema CLI path and use `--edit-payload=-`.
Several harnesses' native Edit tool
needs a fresh Read of the file immediately before the edit, even when you already have the body from
elsewhere in the session — these verbs check their own staleness hash instead of requiring one, so under any
harness the file gets read at most once, never twice.

**Workspace pin (stdio, D3/D4):** when the server was started `ripwire-rs <root> mcp`, an omitted
`path` on ANY verb (read or edit) defaults to that startup root; the 3 edit verbs above additionally
REFUSE (file byte-identical, same contract as every other refusal above) if an explicit `path` resolves
outside it — `path outside workspace; start the server on that root or pass an absolute in-root path`.
Read verbs stay unrestricted at any `path`. A bare `ripwire-rs mcp` with no startup root is unchanged: every
verb still requires its own `path`.

## The 3 MCP edit verbs — span-addressed writes, no read-then-diff

CLI is the preferred front door when a shell is available:

```bash
ripwire-rs ROOT --replace-symbol-body=SYM --edit-payload=FILE
printf '%s' "$BLOCK" | ripwire-rs ROOT --insert-before-symbol=SYM --edit-payload=-
```

`--insert-after-symbol` is the third form; `--edit-target-file=PATH` disambiguates same-named definitions.
These flags and the MCP verbs below call the same transaction-safe edit engine. MCP is useful when its
workspace is already warm or the client has no shell.

`replace_symbol_body`, `insert_before_symbol`, `insert_after_symbol` are ripwire's WRITE verbs.
Each locates a symbol's definition in the already-parsed index and splices text at its byte span:

| Verb | Args | Does |
|---|---|---|
| `replace_symbol_body` | `path, symbol, new_body, file?` | replaces signature-through-closing-brace with `new_body` verbatim; bytes outside the span are untouched |
| `insert_before_symbol` | `path, symbol, text, file?` | inserts `text` at the def's first byte |
| `insert_after_symbol` | `path, symbol, text, file?` | inserts `text` past the def's final byte |

**Newline rule** (so you don't have to guess): `replace_symbol_body` adds/removes nothing — `new_body` must
be a complete, well-formed definition. `insert_before_symbol` appends a `\n` to `text` only if it lacks a
trailing one, so the inserted block and the definition land on separate lines. `insert_after_symbol`
prepends a `\n` only if `text` lacks a leading one — same separation, other direction. Verified: inserting
`"int sub(...) {...}"` (no trailing newline) after `add` in a fixture produced `add`'s body immediately
followed by `\nint sub(...) {...}` on its own line, with every other byte of the file unchanged.

**Safety contract — every failure path leaves the file byte-for-byte unchanged:**
- **Not found** → JSON-RPC error listing the nearest symbol names by shared-prefix score (a typo hint),
  e.g. `symbol 'nosuchsymbolxyz' not found; nearest: normalCaller, makesWidget, PoolThing, ...`.
- **Ambiguous** (>1 definition with that name) → error listing every candidate as `file:line`; retry the
  call with `file` set to a disambiguating path substring.
- **Stale index** → the server hashes each indexed file's bytes at index-build time; if a fresh read of the
  target file no longer matches that hash (edited outside this session, or by a prior call), the edit
  refuses rather than splicing at offsets that no longer address the right bytes. Call any read verb first
  — that refreshes the index — then retry the edit.
- **Insane span** (`a>=b` or `b>filesize`) or an unreadable file → refuses; never splices out of bounds.
- **Atomic write**: the new bytes are written to `<path>.<pid>.tmp` then `rename(2)`d over the original — a
  reader never sees a torn file, and a crash mid-write leaves the original intact.
- **On success**: the in-memory index is invalidated (`valid=false`), so the very next verb call — read or
  edit — rebuilds from the just-written disk state. The response JSON includes the applied `span`,
  `old_file_bytes`/`new_file_bytes`, the pre-edit `stale_index` stamp, and a note that the index will refresh.

**When to reach for these vs your normal editor tool** — lead with the harness-independent case: **if you
already have the body** — `fetch_body` or `--expand` served it this session — these verbs never re-read the
file. They splice directly at the already-known span, checked against their own staleness hash (see above),
not a fresh Read. Several coding-agent harnesses require their native edit tool to Read a file immediately
before an Edit on it, even when the agent already has the text from a prior call in the same session — under
that kind of harness, a served body plus a native Edit still pays for the file twice. `replace_symbol_body`/
`insert_before_symbol`/`insert_after_symbol` were never routed through the native edit tool, so they don't
inherit that requirement under any harness. Reach for the edit verb over native Edit whenever the body
already arrived via ripwire this session; once you've opened the file in your own editor tool anyway, the
advantage is gone. (Whether your specific harness enforces a pre-edit Read is worth checking against its own
behavior rather than assuming — this reasoning holds either way, since the edit verbs skip a Read regardless
of whether one would otherwise be required.)

The other unique value: a **span-addressed edit without reading the file first at all** — you already know
the symbol name from a prior ripwire call (e.g. `--callers` told you who to fix), so you can edit it in one
round-trip instead of read → locate → edit. The trade-off: **name ambiguity needs the `file` arg** — if the
codebase has multiple symbols with that name (overloads, same-named methods on different types), the first
call will refuse and hand you the candidate list; a native editor tool addressed by file:line doesn't have
this failure mode. Prefer your editor tool when you're already looking at the file, or when the edit isn't a
whole-definition replace/insert (a mid-body tweak isn't representable — `replace_symbol_body` is
signature-through-closing-brace or nothing).

## Server Behavior Worth Knowing

- **Single tool registry**: tool schemas and dispatch share the registry in `src/mcp/schema.rs` and `src/mcp/dispatch.rs`.
- **Canonical read reports**: overlapping tools return the CLI XML/text in `result.content[0].text`, including canonical XML legend metadata rather than JSON summary objects.
- **Automatic cache invalidation on edit**: `replace_symbol_body`, `insert_before_symbol`, and `insert_after_symbol` perform atomic write-rename operations and keep the workspace consistent.
- **Graceful JSON-RPC errors**: unknown tools, missing parameters, or unresolvable symbols return structured JSON-RPC `-32602` / `-32603` error objects without terminating the stdio server.
