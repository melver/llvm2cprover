# llvm2cprover

`llvm2cprover` translates LLVM bitcode or textual LLVM IR into CBMC's GOTO
binary format. That lets CBMC act as a verification back-end for any language
that can emit LLVM IR.

## Prerequisites

- **LLVM 22 or later** — inkwell links against LLVM via llvm-sys; the headers
  and libraries must be installed and discoverable (e.g. via `llvm-config`)
- **Rust stable** toolchain
- **CBMC** — for running verification on the produced `.goto` files
- **Python 3** — only needed for the E2E test suite (`tests/run.py`)

## Usage

Build:

```sh
git submodule update --init
make release
```

Translate a file:

```sh
llvm2cprover input.bc -o input.goto
cbmc input.goto --function main
```

Translate from stdin:

```sh
clang -emit-llvm -c -O0 foo.c -o - | llvm2cprover - -o foo.goto
cbmc foo.goto --function main
```

Translate textual LLVM IR:

```sh
llvm2cprover --input-format ll input.ll -o input.goto
```

Prefix module-level symbols for independently translated variants:

```sh
llvm2cprover --rename-prefix old_ old.bc -o old.goto
cbmc old.goto --function old_main
```

Renaming applies to function and global definitions and every reference to those definitions.
External declarations remain shared so unresolved dependencies keep their normal runtime and
library bindings. LLVM intrinsics, `__CPROVER_*` builtins, and llvm2cprover runtime symbols also
remain shared. An empty prefix is a no-op. Contract manifests cannot currently be combined with a
nonempty prefix because manifests refer to original LLVM symbol names; the translator rejects that
combination instead of silently dropping contracts.

For automation, `--diagnostics json` writes exactly one JSON object to stderr
when translation fails. The object always has `kind` and `message` fields.
Unsupported-feature errors additionally have an `unsupported_feature` field,
and may include `symbol` and structured `location` context when available.
The default, `--diagnostics text`, preserves human-readable error output.

Unsupported approximations are opt-in by feature. The only currently available
selector is `--unsound=varargs`, which ignores `va_start`, `va_end`, and
`va_copy`. When `--report` is requested, every ignored intrinsic is recorded.
Bare `--unsound` remains accepted as shorthand for all available approximations,
but prints a warning after successful translation; scripts should use explicit
selectors. Stateful inline assembly has no bounded approximation and remains
fail-closed in every mode.

`--report <file.json>` writes a deterministic assumption and stub inventory
after successful translation. It contains:

- referenced-but-undefined input functions (unused declarations are omitted)
- every enabled approximation actually taken, including its enclosing function
  and best-effort source location
- every contract applied at a direct call site, whether it was instrumentation or an explicit
  replacement, and whether the contract has been checked against the preserved implementation;
  replacements currently report `discharged: false`
- the resolved machine model and the input module's raw target triple and data layout

The report has a numeric `schema_version` field for consumers. A report path
cannot be the same as the GOTO output path.

Machine model:

- `--machine-model auto` infers the target model from the module's data layout
  and target triple
- `--machine-model ilp32|lp64|llp64` overrides that inference

## Contracts

`--contracts <file.toml>` attaches one or more contract manifests. The manifest
maps target functions to helper symbols, while the helper implementations
themselves are ordinary compiled LLVM functions in the linked input.

Example manifest:

```toml
[functions.abs]
requires = "__contract_requires_abs"
ensures = "__contract_ensures_abs"
model = "__contract_model_abs"
assigns = ["__contract_assigns_abs"]
```

Contract workflow:

1. Write helper functions in C, C++, Rust, or another LLVM-emitting language.
2. Compile and link them into the module under test.
3. Pass the manifest with `--contracts`.

To replace selected direct calls with their contracts, repeat
`--replace-with-contract <function>` as needed. Replacement is deliberately opt-in per run; the
original function body remains in the GOTO program for separate contract checking. A replacement
asserts `requires`, executes the `assigns` helpers to havoc exactly the declared frame, creates a
nondeterministic return value, and assumes `ensures`. The manifest must explicitly include an
`assigns` key: use `assigns = []` to declare that the replacement is pure. Replacement and `model`
are mutually exclusive. For variadic targets, helpers summarize the fixed parameters and the
replacement ignores extra arguments. The current bindings cannot attach native assigns metadata to
a variadic symbol, so a selected variadic replacement enforces its frame through the replacement
body; an unselected variadic contract with `assigns` fails closed. Address-taken/indirect uses and
`invoke` uses of selected functions fail closed until their replacement semantics are supported.

Supported directives:

- `requires`: inserts a pre-call `__CPROVER_assume(...)`
- `ensures`: inserts a post-call `__CPROVER_assert(...)`
- `model`: redirects direct calls to a helper function
- `assigns`: attaches CBMC assigns metadata to defined and external functions, generates stub bodies
  only for unselected external declarations with a nonempty frame, and supplies the writable frame
  for explicit contract replacement; defined function bodies remain intact

## Design

Overview of the current design:

```text
LLVM IR / bitcode
  -> inkwell
  -> llvm2cprover lowering
  -> cprover_bindings
  -> GOTO binary
  -> CBMC
```

For contracts, instead of inventing a new expression language, `llvm2cprover`
expects contract helpers to be ordinary compiled LLVM functions and uses a
small TOML manifest only to map those helper symbols onto target functions.

Unsupported LLVM features produce deterministic translation errors at the point
where lowering would become unsound, rather than silently approximating away
behavior. Explicit `--unsound=<feature>` selectors gate the small set of
documented approximations and `--report` inventories each approximation used.

## Related Work

- [llvm2goto](https://github.com/sbjoshi/llvm2goto)

## License

llvm2cprover is licensed under the Apache License 2.0. See LICENSE for details.

The bundled `cprover_bindings` crate (via the Kani submodule in `third_party/`)
is licensed under Apache-2.0 OR MIT.
