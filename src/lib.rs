// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026, Marco Elver <me@marcoelver.com>

//! Public API for translating LLVM IR into CBMC GOTO binaries.
//!
//! The library surface stays intentionally small:
//!
//! - [`translate`] is the one-shot entry point used by the CLI
//! - [`Config`] carries stable translation options
//! - [`translator`] owns LLVM parsing, lowering, diagnostics, and emission
//! - [`contracts`] owns external contract manifests and contract injection
//!
//! More detailed lowering rules live next to the implementation so the code,
//! rather than a separate architecture document, remains the durable source of
//! truth.

pub mod contracts;
pub mod translator;
pub mod util;

use std::path::PathBuf;

use anyhow::Result;

pub use translator::{Diagnostic, TranslationInput, Translator};

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

/// Top-level translation configuration shared by the CLI and library API.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Config {
    /// Output `.goto` path. Defaults to `<input>.goto` or `a.goto` for stdin.
    pub output: Option<PathBuf>,
    /// TOML contract manifests loaded alongside the translated module.
    pub contracts: Vec<PathBuf>,
    /// Machine model used for emitted GOTO types and builtins. `Auto` infers from the module.
    pub machine_model: MachineModel,
    /// LLVM input format override. `Auto` detects bitcode vs textual IR.
    pub input_format: InputFormat,
    /// Enable explicitly unsound fallbacks for unsupported constructs when probing coverage.
    pub unsound: bool,
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
