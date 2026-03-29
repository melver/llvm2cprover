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
- Keep translation code direct. Avoid speculative abstractions.
- Prefer explicit error handling over panics in production paths.
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

## Documentation

- Keep public rustdoc minimal and useful.
- Add short rationale comments on non-trivial code paths when future readers
  would otherwise have to reconstruct the invariant or semantic constraint from
  scratch.
- Prefer code-adjacent documentation and module docs over long external design
  prose when they disagree.
