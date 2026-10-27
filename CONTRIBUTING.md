# Contributing

## Build And Test

Use the repository `Makefile`:

```sh
make
```

That runs formatting checks, unit tests, the E2E suite, and clippy.

## Code Style

- Prefer simple, maintainable, idiomatic Rust over extra feature coverage that
  would add disproportionate complexity.
- Avoid speculative abstractions.
- Prefer explicit error handling over panics in production paths. Use concrete
  error types at internal domain boundaries where callers branch on error kind.
  `Result<_, String>` can be used at display boundaries where callers never
  inspect the error variant (e.g. CLI entry points and trait methods that
  terminate in user-facing output).
- Add dependencies incrementally, only when the implementation needs them.

## Tests

Unit tests live alongside the source in `src/` and run via `cargo test`.

End-to-end tests live under `tests/<category>/` as standalone `.c`, `.cc`,
`.ll`, or `.rs` files. Each test embeds TOML metadata between comment markers:

```c
// ---
// description = "Integer addition lowers and verifies"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---
```

`tests/run.py` discovers these files, compiles them, translates via
llvm2cprover, runs CBMC, and checks the outcome against `expected`. Output is
TAP. Run with `make test-e2e`.

### Fuzzing (Optional)

`tests/fuzz/csmith.py` differentially fuzzes the translator with
[Csmith](https://github.com/csmith-project/csmith): the native `-O0` checksum
is the oracle; the same program must verify under `clang → llvm2cprover → cbmc`.
It is not part of `make` and is not required for ordinary changes; run it
deliberately (e.g. before a release or after touching instruction lowering).
It needs `csmith`, `clang`, and `cbmc` on `PATH` plus a built
`target/debug/llvm2cprover`:

```sh
tests/fuzz/csmith.py run --out /tmp/fuzz --seeds 1-500 -j 16   # exit 1 iff actionable
tests/fuzz/csmith.py reduce --out /tmp/fuzz --seed 18           # creduce a failing seed
```

Failing seeds keep their artifacts under `<out>/seed-N/`; every outcome is
appended to `<out>/results.jsonl`.

## Documentation

- Keep public rustdoc minimal and useful.
- Add executable rustdoc examples for public APIs where a short example clarifies
  intended use or important semantics.
- Add short rationale comments on non-trivial code paths when future readers
  would otherwise have to reconstruct the invariant or semantic constraint from
  scratch.
- Prefer code-adjacent documentation and module docs over long external design
  prose when they disagree.
