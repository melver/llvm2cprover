# llvm2cprover

`llvm2cprover` translates supported [LLVM](https://llvm.org/) bitcode and
textual LLVM IR into [CBMC](https://www.cprover.org/cbmc/) GOTO binaries. This
lets LLVM-emitting languages use CBMC as a bounded model checking backend.
Unsupported LLVM features produce translation errors; a small set of
intentionally unsound fallbacks are available as an opt-in feature.

## Quick start

### Prerequisites

- **LLVM 23 or later**
- **Rust stable** and Cargo
- **CBMC**
- *Development dependencies:*
    - **Clang**
    - **Python 3**

Build the release binary from a checkout:

```sh
git submodule update --init
make release
```

The binary is written to `target/release/llvm2cprover`. Either invoke it there
or place it in your `PATH`; commands after the quick start assume the latter.

Create a small verification target:

```c
/* example.c */
extern void __CPROVER_assert(_Bool, const char *);

int add(int lhs, int rhs) {
    return lhs + rhs;
}

int main(void) {
    __CPROVER_assert(add(2, 3) == 5, "addition");
    return 0;
}
```

Compile, translate, and verify it:

```sh
clang -emit-llvm -c -O0 example.c -o example.bc
./target/release/llvm2cprover example.bc -o example.goto
cbmc example.goto --function main
```

CBMC is a bounded model checker. For programs with loops or recursion, choose
appropriate unwind bounds and review CBMC's unwinding results as part of the
verification claim.

## Translation

The input may be LLVM bitcode, textual LLVM IR, or standard input. The default
output is the input path with its extension replaced by `.goto`; standard input
defaults to `a.goto`.

```sh
# Bitcode
llvm2cprover input.bc -o input.goto

# Textual LLVM IR
llvm2cprover --input-format ll input.ll -o input.goto

# Bitcode from standard input
clang -emit-llvm -c -O0 foo.c -o - \
  | llvm2cprover - -o foo.goto
```

`--input-format auto|bc|ll` overrides input detection when necessary. Repeat
`--entry <function>` to restrict lowered function bodies, globals, and manifest
contracts to the cone of influence of the selected entry points. Run
`llvm2cprover --help` for the complete command-line reference.

### Machine model

`--machine-model auto` infers the machine model from the LLVM data layout and
target triple. Use `--machine-model ilp32|lp64|llp64` to override it
explicitly. The resolved model and raw LLVM target information are included in
translation reports.

### Symbol renaming

`--rename-prefix <prefix>` prefixes module-level function and global
definitions, together with all references to them. This is useful when
independently translated program variants must coexist:

```sh
llvm2cprover --rename-prefix old_ old.bc -o old.goto
cbmc old.goto --function old_main
```

External declarations, LLVM intrinsics, `__CPROVER_*` builtins, and
llvm2cprover runtime symbols remain shared. A nonempty prefix cannot currently
be combined with contract manifests because manifests refer to original LLVM
symbol names.

## Soundness and limitations

Translation fails when llvm2cprover cannot preserve the semantics of an LLVM
construct. In particular:

- unmatched stateful inline assembly always fails closed; exact manifest
  mappings can route it to compiled model functions
- non-assembly `callbr`, unwind-capable inline assembly, and overlapping
  inline-assembly mappings fail closed
- contract replacement rejects address-taken targets, indirect uses, and
  `invoke` call sites
- contract enforcement rejects recursive, variadic, external, indirectly used,
  and `invoke`-called targets
- contract enforcement currently rejects input uses or definitions of `free`
- void enforcement targets currently require `assigns = []`
- contract manifests cannot currently be combined with a nonempty symbol prefix

Unsupported approximations are opt-in by feature. The only current selector is
`--unsound=varargs`, which ignores `va_start`, `va_end`, and `va_copy`. Bare
`--unsound` enables all available approximations and prints a warning;
automation should use explicit selectors. Every approximation actually taken is
included in `--report` output.

## Contracts

Contracts keep their logic in ordinary compiled helper functions. TOML
manifests map those helper symbols to target functions; they do not define
another expression language. Pass one or more manifests by repeating
`--contracts <file.toml>`.

There are three ways a function contract participates in verification:

| Workflow | `requires` | Function behavior | `ensures` | Writable frame |
| --- | --- | --- | --- | --- |
| Ordinary call | assume | real body, or trusted `model` | assert | attach metadata |
| Replacement | assert | havoc frame and result | assume | havoc returned objects |
| Enforcement | assume | preserved real body | assert | check with CBMC DFCC |

Replacement is a caller-side abstraction. Enforcement is a separate proof
obligation that checks the abstraction against the preserved implementation. A
trusted `model` is not discharged by either mechanism.

### Complete example

The following contract says that `increment` writes only `counter`, increments
it by one, and returns the new value:

```c
/* increment.c */
extern void __CPROVER_assert(_Bool, const char *);

int counter;

int increment(void) {
    return ++counter;
}

_Bool requires_increment(void) {
    return counter < 100;
}

int snapshot_increment(void) {
    return counter;
}

int *assigns_increment(void) {
    return &counter;
}

_Bool ensures_increment(int before, int result) {
    return counter == before + 1 && result == counter;
}

int main(void) {
    counter = 41;
    int result = increment();
    __CPROVER_assert(counter == 42, "post-state");
    __CPROVER_assert(result == 42, "result");
    return 0;
}
```

```toml
# increment.toml
[functions.increment]
requires = "requires_increment"
snapshot = "snapshot_increment"
ensures = "ensures_increment"
assigns = ["assigns_increment"]
```

Compile the target and helpers into the same LLVM module:

```sh
clang -emit-llvm -c -O0 increment.c -o increment.bc
```

Ordinary instrumentation assumes the precondition before a real call and
asserts the postcondition afterward:

```sh
llvm2cprover increment.bc --contracts increment.toml -o increment.goto
cbmc increment.goto --function main
```

### Helper conventions

Helpers receive the target's fixed parameters in their original order:

- `requires(parameters...) -> value`
- `snapshot(parameters...) -> non-void value`
- `ensures(parameters..., snapshot?, result?) -> value`
- `model(parameters...) -> target return type`
- each `assigns(parameters...) -> pointer to one writable object`

The snapshot is captured after `requires` and before the target, model, or
replacement frame havoc.  When present, it is passed to `ensures` after the
target parameters and before the non-void target result. A `snapshot` without
`ensures` is rejected, because the captured value would have no consumer.

Every nonempty `assigns` helper must be a compiled, observationally read-only
definition returning a pointer to one concrete, nonlocal writable object. Use
multiple helpers for multiple objects and `assigns = []` for an explicitly pure
function. Translation rejects nonlocal helper stores, volatile or atomic
accesses, fences, memory-writing intrinsics, potentially side-effecting calls,
and pointers into the helper's own stack frame.

LLVM uses opaque pointers, so an assigns helper's IR must retain enough
information to recover the pointee type. In C, `return p + 0` retains the
element type where a direct `return p` may not.

For an external declaration that is not selected for replacement, a nonempty
`assigns` frame also generates a stub body that havocs the declared objects, so
plain `cbmc` sees the intended writes without contract-mode flags. Defined
function bodies are never modified.

### Replacement

Repeat `--replace-with-contract <function>` to replace selected direct calls:

```sh
llvm2cprover increment.bc --contracts increment.toml \
  --replace-with-contract increment -o increment.goto
cbmc increment.goto --function main
```

The original target body remains in the GOTO program. A replacement asserts
`requires`, captures the snapshot, evaluates each `assigns` helper and havocs
its returned object, creates a nondeterministic result, assumes `ensures`, and
returns that result. The manifest must contain an explicit `assigns` key.
Replacement and `model` are mutually exclusive.

For variadic targets, helpers summarize the fixed parameters and the
replacement ignores extra arguments. Native assigns metadata is not available
for variadic symbols, so selected variadic replacement enforces its frame in
the synthesized replacement body; unselected variadic contracts with `assigns`
fail closed.

### Enforcement

Select exactly one preserved target with `--enforce-contract <function>`. Each
translation contains one enforcement obligation because the CBMC workflow
verifies one harness/specification pair at a time.

For the example above:

```sh
llvm2cprover increment.bc --contracts increment.toml \
  --enforce-contract increment -o increment.goto
goto-cc increment.goto \
  --function __llvm2cprover_contract_enforcement_increment \
  -o increment.linked.goto
goto-instrument \
  --dfcc __llvm2cprover_contract_enforcement_increment \
  --no-malloc-may-fail \
  --enforce-contract increment/__llvm2cprover_contract_spec_increment \
  increment.linked.goto increment.enforced.goto
cbmc increment.enforced.goto
```

The generated harness assumes `requires`, captures `snapshot`, computes the
writable frame, calls the real target body, and asserts `ensures`. `goto-cc
--function` also creates CBMC's entry point and initialization function. The
DFCC pass checks that writes stay inside the attached frame; running the
uninstrumented harness alone is not a frame proof.

Generated harness and specification names use the sanitized target symbol:

- `__llvm2cprover_contract_enforcement_<symbol>`
- `__llvm2cprover_contract_spec_<symbol>`

Every helper selected for enforcement must be a compiled definition.
Enforcement cannot be combined with replacement or a trusted `model`.

### Trusted function models

A `model` redirects ordinary direct calls to a helper with the target's
parameter and return types:

```toml
[functions.read_counter]
model = "__model_read_counter"
```

Models are trusted assumptions, not contract proofs. Any model or unresolved
helper relied upon by a translation is recorded by `--report`.

## Inline-assembly models

The same manifest format can map exact LLVM inline-assembly metadata to a
defined model function:

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

All assembly properties are matched exactly. The model must be defined and
non-variadic, and its parameter and return types must exactly match the
assembly call. LLVM represents multiple assembly outputs as an aggregate
return, so the model must return the identical aggregate type.

Generate deterministic stubs for every unmatched inline-assembly template in a
module before writing models by hand:

```sh
llvm2cprover --emit-asm-manifest input.bc > inline-asm.toml
```

Repeat `--contracts` to omit sites already covered by existing manifests. The
generated TOML is written to standard output, while template and site counts go
to standard error so redirection remains safe. Each entry records the enclosing
functions and required LLVM model signature in comments. Unwind-capable and
module-level assembly are reported but omitted because call-site model mappings
cannot replace them. Templates used with different signatures in different
functions receive function-qualified entries. If one function uses the same
exact template with incompatible signatures, the generator reports that in a
comment because the current exact-match format cannot distinguish those sites.

For `callbr`/asm-goto, the model executes once for the assembly effects, then
verification considers the fallthrough and every indirect successor with normal
LLVM PHI-edge semantics. Empty and comment-only compiler barriers in ordinary
calls remain no-ops. A `callbr` always requires a model because it may transfer
control to an indirect destination.

## Automation

### Translation reports

`--report <file.json>` writes a deterministic assumption and stub inventory
after successful translation. It includes:

- referenced-but-undefined input functions, omitting unused declarations
- every enabled approximation actually taken, with its enclosing function and
  best-effort source location
- every compiled inline-assembly model actually used
- contracts used for ordinary instrumentation or replacement
- the selected enforcement obligation
- the resolved machine model, raw target triple, and data layout

The report has a numeric `schema_version`. Contract entries identify their
`reliance` as `instrumentation`, `replacement`, or `enforcement`. The current
`discharged: true` value means the emitted artifact contains the selected
enforcement obligation; it is not a CBMC proof result. Trusted mappings and
replacements report `discharged: false`.

### Machine-readable diagnostics

`--diagnostics json` writes exactly one JSON object to standard error when
translation fails. Every object contains `kind` and `message`;
unsupported-feature errors also contain `unsupported_feature` and may include
`symbol` and structured `location` context. The default, `--diagnostics text`,
preserves human-readable errors.

## Design

```text
LLVM IR / bitcode
  -> inkwell
  -> llvm2cprover lowering
  -> cprover_bindings
  -> GOTO binary
  -> CBMC
```

Contract logic remains in compiled helper functions, while manifests only bind
helper symbols to targets. Unsupported features fail at the point where
lowering would otherwise become unsound. Explicit `--unsound=<feature>`
selectors gate and report the small set of documented approximations.

## References

- [CBMC](https://www.cprover.org/cbmc/), the bounded model checker that consumes the generated GOTO
  binaries
- [Kani](https://model-checking.github.io/kani/), whose `cprover_bindings` crate provides the GOTO
  representation used here
- [llvm2goto](https://github.com/sbjoshi/llvm2goto), another LLVM-to-CPROVER translator

## License

llvm2cprover is licensed under the [Apache License 2.0](LICENSE).

The bundled `cprover_bindings` crate, provided through the Kani submodule in
`third_party/`, is licensed under Apache-2.0 OR MIT.
