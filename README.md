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
selectors. Unmatched stateful inline assembly has no bounded approximation and
remains fail-closed in every mode; an exact manifest mapping may instead route
it to compiled model code.

`--report <file.json>` writes a deterministic assumption and stub inventory
after successful translation. It contains:

- referenced-but-undefined input functions (unused declarations are omitted)
- every enabled approximation actually taken, including its enclosing function
  and best-effort source location
- every compiled inline-assembly model actually used; these trusted mappings
  report `discharged: false`
- every contract applied at a direct call site, whether it was instrumentation or an explicit
  replacement, plus the single selected enforcement obligation. `discharged: true` means the
  emitted artifact contains that obligation; it does not mean the proof succeeded. Only a
  successful CBMC run proves it. Replacement entries remain `discharged: false`; discharge uses a
  separate enforcement translation
- the resolved machine model and the input module's raw target triple and data layout

The report has a numeric `schema_version` field for consumers. A report path
cannot be the same as the GOTO output path.

Machine model:

- `--machine-model auto` infers the target model from the module's data layout
  and target triple
- `--machine-model ilp32|lp64|llp64` overrides that inference

## Contracts

`--contracts <file.toml>` attaches one or more model manifests. Function
contracts map targets to helper symbols, and inline-assembly entries map exact
LLVM assembly metadata to model symbols. All helper and model implementations
are ordinary compiled LLVM functions in the linked input.

Example manifest:

```toml
[functions.abs]
requires = "__contract_requires_abs"
ensures = "__contract_ensures_abs"
snapshot = "__contract_snapshot_abs"
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
asserts `requires`, evaluates the `assigns` helpers and havocs each returned object, creates a
nondeterministic return value, and assumes `ensures`. The manifest must explicitly include an
`assigns` key: each named helper returns a pointer to one writable object, and the replacement
havocs the pointed-to object. Use `assigns = []` to declare that the replacement is pure.
Replacement and `model` are mutually exclusive. For variadic targets, helpers summarize the fixed
parameters and the replacement ignores extra arguments. The current bindings cannot attach native
assigns metadata to a variadic symbol, so a selected variadic replacement enforces its frame through
the replacement body; an unselected variadic contract with `assigns` fails closed.
Address-taken/indirect uses and `invoke` uses of selected functions fail closed until their
replacement semantics are supported.

To check a contract against its preserved implementation, select exactly one target with
`--enforce-contract <function>`. Each translation accepts one enforcement obligation because the
generated CBMC workflow proves one harness/specification pair at a time. The translation contains
a harness named `__llvm2cprover_contract_enforcement_<symbol>` and a frame specification named
`__llvm2cprover_contract_spec_<symbol>`, where `<symbol>` is the sanitized target name used by the
translator. Prepare and verify it with CBMC's dynamic-frame pass:

```sh
llvm2cprover input.bc --contracts contracts.toml \
  --enforce-contract target -o target.goto
goto-cc target.goto --function __llvm2cprover_contract_enforcement_target \
  -o target.linked.goto
goto-instrument --dfcc __llvm2cprover_contract_enforcement_target \
  --no-malloc-may-fail \
  --enforce-contract target/__llvm2cprover_contract_spec_target \
  target.linked.goto target.enforced.goto
cbmc target.enforced.goto
```

The harness assumes `requires`, captures `snapshot`, computes the frame, calls the real target body,
and asserts `ensures`. `goto-cc --function` is required: besides linking, it creates CBMC's entry
point and initialization function. `goto-instrument --dfcc` enforces writes against the attached
frame; running the uninstrumented harness alone checks only the postcondition and is not a frame
proof. Direct or mutual recursion, variadic or external targets, indirect uses, and `invoke` calls
fail closed. Enforcement cannot be combined with replacement or a trusted `model`, and every
selected contract helper must be a compiled definition. Input uses or definitions of `free` are
currently rejected in enforcement artifacts because CBMC's DFCC runtime must supply its own model.
Void targets currently require `assigns = []`; CBMC's DFCC rejects their nonempty generated frame.

Stateful postconditions can use `snapshot` to capture pointer-reachable pre-state. The snapshot
helper is called after `requires` and before either the real target/model call or replacement
`assigns` helpers. It receives the target parameters and returns one non-void LLVM value. The
`ensures` helper receives the target parameters first, that captured value next, and the target
result last when the target is non-void. A snapshot without `ensures` is rejected.

Supported directives:

- `requires`: inserts a pre-call `__CPROVER_assume(...)`
- `ensures`: inserts a post-call `__CPROVER_assert(...)`
- `snapshot`: calls a helper after `requires` but before the target and preserves its non-void
  result for `ensures`; the `ensures` argument order is target parameters, snapshot, then target
  result when non-void
- `model`: redirects direct calls to a helper function
- `assigns`: attaches CBMC assigns metadata to defined and external functions, generates stub bodies
  only for unselected external declarations with a nonempty frame, and supplies the writable frame
  for explicit contract replacement; defined function bodies remain intact. Each helper must be a
  compiled, observationally read-only definition returning a pointer to one concrete, nonlocal
  writable object. The helper is an lvalue expression: replacements and synthesized external stubs
  havoc the pointed-to object, while DFCC checks preserved implementations against the dereferenced
  lvalue. Translation rejects nonlocal stores, volatile or atomic accesses
  (including fences), memory-writing intrinsics, potentially side-effecting calls, and pointers to
  the helper's own stack frame. With LLVM opaque pointers, the helper's IR must retain enough type
  information to recover the pointee. For example, C `return p + 0` retains the element type where
  a direct `return p` may not. Use `assigns = []` for a pure function

The same manifest can map target inline assembly to defined model functions in
the input module:

```toml
[[inline_asm]]
asm = "opaque $0"
constraints = "=r,r"
dialect = "att"
side_effects = true
align_stack = false
function = "read_counter" # optional enclosing-function qualifier
model = "__model_read_counter"
```

All assembly properties in the entry are matched exactly. The model must be a
defined, non-variadic function whose parameter and return types exactly match
the assembly call. LLVM represents multiple assembly outputs as an aggregate
return, so the corresponding model returns the identical aggregate type.
For `callbr`/asm-goto, the model is executed once for the assembly's effects,
then verification considers every declared fallthrough and indirect successor;
normal LLVM PHI-edge semantics are preserved on each path. Overlapping matches,
unwind-capable assembly, and non-assembly `callbr` fail closed. Empty and
comment-only compiler barriers in ordinary calls retain their no-op lowering
and do not need a model; a `callbr` always needs one because it may transfer
control to an indirect destination.

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
Inline-assembly models are ordinary compiled functions selected by exact raw
LLVM assembly metadata, so their behavior can be reviewed and tested without
embedding target-specific semantics in the translator.

## Related Work

- [llvm2goto](https://github.com/sbjoshi/llvm2goto)

## License

llvm2cprover is licensed under the Apache License 2.0. See LICENSE for details.

The bundled `cprover_bindings` crate (via the Kani submodule in `third_party/`)
is licensed under Apache-2.0 OR MIT.
