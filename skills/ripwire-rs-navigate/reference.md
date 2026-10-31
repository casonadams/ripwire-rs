# ripwire-rs-navigate: reference

Rarely needed sections moved out of `SKILL.md`: struct layout before an edit, and `--query` versus `--for`.

## Editing a STRUCT whose bytes cross a boundary — `--layout=STRUCT`

For a plain function, the deep-dive above is the whole story. For a **struct/class whose bytes are a
contract with something outside this compiler** — a GPU uniform block, a wire/IPC/file-format record, a
type mirrored into a stub or a second checkout — the question before you edit is not "who calls it" but
"what is the byte layout, what pins it, and who else declares it":

```bash
ripwire-rs <dir> --layout=AudioUniforms --legend=compact         # file:name disambiguates, like --around/--lego
```

One call gives three things: the fields in declaration order with **computed** offsets/sizes and every byte
of padding made explicit; every `static_assert` in the index that mentions the type, with `agree="0"` when
a `sizeof(X)==N` tripwire contradicts the computed size; and **every same-name definition**, compared field
by field. It **exits 2** when the contract is broken — `mirror="mismatch"` (`kind="drift"`: two populated
definitions disagree) or a contradicted tripwire — so it works as a pre-commit check, not just a report.
`kind="stub"` (an empty placeholder) and `kind="spelling"` (`simd::float4` vs `float4` — the two arms of
one `#ifdef`) are reported but exit 0.

**Believe the caveats.** The offsets are a MODEL, not the ABI: a lexical walk under standard-layout
assumptions on a 64-bit LP64 target. When it cannot see the truth it says `modeled="0"` with a named
`<caveat>` instead of printing a number — `#pragma pack`, bitfields, virtuals, base classes, nested or
anonymous aggregates, `#if`-conditional members, templates, and any field type it cannot size. One unsized
field un-places every field after it. A `modeled="1"` number that agrees with the type's own
`static_assert` is trustworthy; a `modeled="0"` def is telling you to read the source.

## `--query` (lexical) vs `--for` (task lens)

- `ripwire-rs <dir> --query="terms" --legend=compact` — **pure BM25 relatedness**: the map re-ranked by lexical match against
  your terms, nothing else. Use it when you want *what mentions these words*, uncolored by importance —
  chasing a domain term, an error string's neighborhood, a concept's vocabulary.
- `ripwire-rs <dir> --for="task in words"` — the **task lens**: relevance-ranked *signatures* plus doc-comments
  and cx/in metrics, framed for reuse ("compose from these"). Use it when the question is "what should I
  build on / touch for THIS task".
- Rule of thumb: `--for` for a task you're about to do; `--query` for a vocabulary you're hunting. Both shine
  on specific technical wording; for broad common-word asks, plain `rg` + one read can still win.

**Budgeting trap:** a positive, explicit `--top-k` is inert on plain `--for`. Narrow plain `--for` with its own arguments: `--signatures-only` (no auto-bodies), `--token-budget=N` (shapes
the bundle to fit), `--detail=N` (full bodies for just the top N). `--top-k` does shape `--query`. `--expand`/`--outline` drop the map by default; pass `--top-k=N` only if you want the map back in the same call.

