# AGENTS.md

Start with `README.md` for usage and `CONTRIBUTING.md` for build, test, and contribution workflow.

Treat code-adjacent rustdoc and comments in `src/` as the source of truth for translation semantics. When behavior changes, update the relevant docs in the same change.

## Validation

- Run `make` before considering a task complete.
- Add or update tests for behavior changes.

## Repo-specific rules

- Preserve fail-closed handling for unsupported LLVM features.
- Do not silently approximate unsupported semantics unless explicitly gated by `--unsound`.
- Keep the CLI thin; keep lowering logic in the library.
- Keep contract logic in compiled helper functions; manifests only map helper symbols to target functions.
- Prefer `.ll` tests when frontend compilation would optimize away the LLVM construct under test.
- Avoid editing `third_party/` unless the task is explicitly about vendored dependencies.
