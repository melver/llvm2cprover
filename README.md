# llvm2cprover

`llvm2cprover` translates LLVM bitcode or textual LLVM IR into CBMC's GOTO
binary format. That lets CBMC act as a verification back-end for any language
that can emit LLVM IR.

```text
LLVM IR / bitcode
  -> inkwell
  -> llvm2cprover lowering
  -> cprover_bindings
  -> GOTO binary
  -> CBMC
```

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

Supported directives:

- `requires`: inserts a pre-call `__CPROVER_assume(...)`
- `ensures`: inserts a post-call `__CPROVER_assert(...)`
- `model`: redirects direct calls to a helper function
- `assigns`: attaches CBMC assigns metadata and generates extern stub bodies

## License

llvm2cprover is licensed under the Apache License 2.0. See LICENSE for details.

The bundled `cprover_bindings` crate (via the Kani submodule in `third_party/`)
is licensed under Apache-2.0 OR MIT.
