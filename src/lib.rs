// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Public API for translating LLVM IR into CBMC GOTO binaries.
//!
//! The library surface stays intentionally small:
//!
//! - [`translate`] is the one-shot entry point used by the CLI
//! - [`Config`] carries stable translation options
//! - [`translator`] owns LLVM parsing, lowering, diagnostics, and emission
//! - [`contracts`] owns external contract and inline-assembly model manifests
//!
//! More detailed lowering rules live next to the implementation so the code,
//! rather than a separate architecture document, remains the durable source of
//! truth.

pub mod contracts;
pub mod translator;
pub mod util;

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;

pub use translator::{Diagnostic, JsonDiagnostic, TranslationInput, Translator};

/// Input encoding selection for LLVM ingestion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputFormat {
    #[default]
    Auto,
    Bitcode,
    LlvmIr,
}

/// Machine model selection for emitted GOTO programs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MachineModel {
    #[default]
    Auto,
    Ilp32,
    Lp64,
    Llp64,
}

/// An explicitly selectable translation approximation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UnsoundFeature {
    /// Ignore LLVM variadic-list setup and teardown intrinsics.
    VarArgs,
}

impl UnsoundFeature {
    /// Every approximation class implemented by this version of llvm2cprover.
    pub const ALL: &'static [UnsoundFeature] = &[Self::VarArgs];

    /// Stable command-line and report name for this approximation class.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VarArgs => "varargs",
        }
    }
}

impl MachineModel {
    /// Stable command-line and report name for this model.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Ilp32 => "ilp32",
            Self::Lp64 => "lp64",
            Self::Llp64 => "llp64",
        }
    }
}

/// Explicitly enabled unsound translation approximations.
///
/// The default is empty and therefore fail-closed. Stateful inline assembly is
/// deliberately not represented here because it has no bounded approximation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnsoundOptions {
    features: BTreeSet<UnsoundFeature>,
}

impl UnsoundOptions {
    /// Enable every approximation implemented by this version of llvm2cprover.
    pub fn all() -> Self {
        Self::new(UnsoundFeature::ALL.iter().copied())
    }

    /// Enable exactly the listed approximation classes.
    pub fn new(features: impl IntoIterator<Item = UnsoundFeature>) -> Self {
        Self {
            features: features.into_iter().collect(),
        }
    }

    /// Return whether one approximation class is enabled.
    pub fn contains(&self, feature: UnsoundFeature) -> bool {
        self.features.contains(&feature)
    }

    /// Return whether no unsound approximations are enabled.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

/// Top-level translation configuration shared by the CLI and library API.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Config {
    /// Output `.goto` path. Defaults to `<input>.goto` or `a.goto` for stdin.
    pub output: Option<PathBuf>,
    /// TOML contract and inline-assembly model manifests loaded alongside the module.
    pub contracts: Vec<PathBuf>,
    /// Functions whose direct calls are replaced by their manifest contracts.
    ///
    /// Replacement is explicit per translation. Each selected function must have a contract
    /// with an explicitly present `assigns` clause, including `assigns = []` for a pure function.
    pub replace_with_contract: BTreeSet<String>,
    /// The function for which a contract-checking harness is emitted.
    ///
    /// At most one function may be selected. Enforcement preserves and calls the real
    /// implementation. The selected function must be
    /// defined, non-variadic, non-recursive, and have an explicit `assigns` clause. The generated
    /// harness checks pre/postconditions; CBMC's dynamic-frame pass checks the attached frame.
    pub enforce_contract: BTreeSet<String>,
    /// Prefix applied to translated LLVM function and global definitions.
    ///
    /// External declarations, LLVM intrinsics, and verifier/runtime symbols remain unprefixed.
    /// Manifests cannot currently be combined with renaming because their symbol bindings use
    /// original LLVM names.
    pub rename_prefix: Option<String>,
    /// Machine model used for emitted GOTO types and builtins. `Auto` infers from the module.
    pub machine_model: MachineModel,
    /// LLVM input format override. `Auto` detects bitcode vs textual IR.
    pub input_format: InputFormat,
    /// Explicitly enabled, documented unsound fallbacks for coverage probing.
    ///
    /// Constructs whose effects cannot be bounded, such as unmatched target inline assembly,
    /// still fail closed.
    pub unsound: UnsoundOptions,
    /// Optional path for a deterministic JSON assumption and stub inventory.
    pub report: Option<PathBuf>,
}

/// Translate one LLVM module into a `.goto` binary.
///
/// The input may come from a file or stdin. Parsing, lowering, contract
/// injection, and final serialization are all performed in one pass through the
/// [`translator::Translator`] entry point.
pub fn translate(input: TranslationInput, config: Config) -> Result<()> {
    Translator::new(config).translate(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn translate_writes_minimal_goto_file() {
        let input = std::env::temp_dir().join(format!(
            "llvm2cprover-{}-{}.ll",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        let path = std::env::temp_dir().join(format!(
            "llvm2cprover-{}-{}.goto",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        fs::write(&input, "define void @f() {\nentry:\n  ret void\n}\n")
            .expect("failed to write temporary LLVM IR input");

        let result = translate(
            TranslationInput::Path(input.clone()),
            Config {
                output: Some(path.clone()),
                ..Config::default()
            },
        );

        assert!(result.is_ok());
        assert!(path.is_file());

        fs::remove_file(input).expect("failed to remove temporary LLVM IR file");
        fs::remove_file(path).expect("failed to remove temporary goto file");
    }
}
