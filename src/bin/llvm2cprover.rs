// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use llvm2cprover::{
    Config, InputFormat, JsonDiagnostic, MachineModel, TranslationInput, UnsoundFeature,
    UnsoundOptions, emit_asm_manifest, translate,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum CliInputFormat {
    #[default]
    Auto,
    Bc,
    Ll,
}

impl From<CliInputFormat> for InputFormat {
    fn from(value: CliInputFormat) -> Self {
        match value {
            CliInputFormat::Auto => InputFormat::Auto,
            CliInputFormat::Bc => InputFormat::Bitcode,
            CliInputFormat::Ll => InputFormat::LlvmIr,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum CliMachineModel {
    #[default]
    Auto,
    Ilp32,
    Lp64,
    Llp64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum DiagnosticFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliUnsoundFeature {
    All,
    Varargs,
}

impl From<CliMachineModel> for MachineModel {
    fn from(value: CliMachineModel) -> Self {
        match value {
            CliMachineModel::Auto => MachineModel::Auto,
            CliMachineModel::Ilp32 => MachineModel::Ilp32,
            CliMachineModel::Lp64 => MachineModel::Lp64,
            CliMachineModel::Llp64 => MachineModel::Llp64,
        }
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: Option<PathBuf>,

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long = "contracts")]
    contracts: Vec<PathBuf>,

    /// Emit unmatched inline-assembly model stubs to standard output without lowering.
    #[arg(
        long,
        conflicts_with_all = [
            "output",
            "entry",
            "fp_groups",
            "replace_with_contract",
            "enforce_contract",
            "rename_prefix",
            "machine_model",
            "unsound",
            "report",
            "pre_passes"
        ]
    )]
    emit_asm_manifest: bool,

    /// Restrict lowered function bodies and globals to the cone of influence of this entry function.
    #[arg(long = "entry")]
    entry: Vec<String>,

    /// Replace direct calls to this function with its manifest contract.
    #[arg(long = "replace-with-contract")]
    replace_with_contract: Vec<String>,

    /// Emit one harness that checks this function's contract against its real body.
    #[arg(long = "enforce-contract")]
    enforce_contract: Vec<String>,

    /// Prefix translated LLVM function and global definitions.
    #[arg(long)]
    rename_prefix: Option<String>,

    /// Lower indirect calls to a dispatch restricted by these function groups (TOML, `[groups]`).
    #[arg(long, requires = "entry")]
    fp_groups: Option<PathBuf>,

    /// Run this LLVM pass pipeline (`opt -passes=` syntax, e.g. `sroa`) first; the optimized
    /// module is what gets verified, so passes may remove undefined behavior from it.
    #[arg(long, value_name = "PIPELINE")]
    pre_passes: Option<String>,

    /// Write the transformed LLVM module as bitcode to -o instead of GOTO.
    #[arg(
        long,
        requires = "output",
        conflicts_with_all = [
            "contracts",
            "entry",
            "replace_with_contract",
            "enforce_contract",
            "unsound",
            "report",
            "emit_asm_manifest"
        ]
    )]
    emit_llvm: bool,

    /// Also emit goto-converted function bodies, which verifiers other than CBMC require.
    #[arg(long, conflicts_with_all = ["emit_llvm", "emit_asm_manifest"])]
    goto_convert: bool,

    /// Emit GOTO for ESBMC: implies --goto-convert; contract enforcement needs CBMC's DFCC.
    #[arg(long, conflicts_with_all = ["emit_llvm", "emit_asm_manifest", "enforce_contract"])]
    esbmc: bool,

    #[arg(long, value_enum, default_value_t = CliMachineModel::Auto)]
    machine_model: CliMachineModel,

    #[arg(long, value_enum, default_value_t = CliInputFormat::Auto)]
    input_format: CliInputFormat,

    /// Enable selected unsound approximations. Bare --unsound enables all and warns.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        num_args = 0..=1,
        default_missing_value = "all",
        require_equals = true
    )]
    unsound: Vec<CliUnsoundFeature>,

    /// Write a JSON assumption and stub inventory after successful translation.
    #[arg(long)]
    report: Option<PathBuf>,

    /// Select human-readable or machine-readable error diagnostics.
    #[arg(long, value_enum, default_value_t = DiagnosticFormat::Text)]
    diagnostics: DiagnosticFormat,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let diagnostics = cli.diagnostics;
    if cli.emit_asm_manifest {
        return match emit_asm_manifest(
            input_from_cli(cli.input),
            cli.input_format.into(),
            &cli.contracts,
        ) {
            Ok(manifest) => {
                print!("{}", manifest.toml);
                eprintln!(
                    "Emitted {} distinct inline-assembly template(s) from {} unmatched site(s).",
                    manifest.distinct_templates, manifest.sites
                );
                if manifest.unwind_sites != 0 {
                    eprintln!(
                        "Warning: omitted {} unwind-capable inline-assembly site(s); compiled models cannot replace them.",
                        manifest.unwind_sites
                    );
                }
                if manifest.has_module_asm {
                    eprintln!(
                        "Warning: omitted module-level inline assembly; call-site model mappings cannot replace it."
                    );
                }
                std::process::ExitCode::SUCCESS
            }
            Err(error) => report_error(&error, diagnostics),
        };
    }
    let bare_unsound = cli.unsound.contains(&CliUnsoundFeature::All);
    let unsound = if bare_unsound {
        UnsoundOptions::all()
    } else {
        UnsoundOptions::new(cli.unsound.into_iter().filter_map(|feature| match feature {
            CliUnsoundFeature::All => None,
            CliUnsoundFeature::Varargs => Some(UnsoundFeature::VarArgs),
        }))
    };
    let config = Config {
        output: cli.output,
        entry: cli.entry.into_iter().collect(),
        contracts: cli.contracts,
        replace_with_contract: cli.replace_with_contract.into_iter().collect(),
        enforce_contract: cli.enforce_contract.into_iter().collect(),
        rename_prefix: cli.rename_prefix,
        emit_llvm: cli.emit_llvm,
        machine_model: cli.machine_model.into(),
        input_format: cli.input_format.into(),
        unsound,
        report: cli.report,
        fp_groups: cli.fp_groups,
        goto_convert: cli.goto_convert || cli.esbmc,
        pre_passes: cli.pre_passes,
    };

    match translate(input_from_cli(cli.input), config) {
        Ok(()) => {
            if bare_unsound {
                eprintln!(
                    "Warning: bare --unsound enables every available approximation; prefer --unsound=varargs"
                );
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => report_error(&error, diagnostics),
    }
}

fn report_error(error: &anyhow::Error, diagnostics: DiagnosticFormat) -> std::process::ExitCode {
    match diagnostics {
        DiagnosticFormat::Text => eprintln!("Error: {error:?}"),
        DiagnosticFormat::Json => {
            let diagnostic = JsonDiagnostic::from_error(error);
            // This schema contains only serializable owned strings and integers.
            eprintln!(
                "{}",
                serde_json::to_string(&diagnostic)
                    .expect("JSON diagnostic serialization cannot fail")
            );
        }
    }
    std::process::ExitCode::FAILURE
}

fn input_from_cli(input: Option<PathBuf>) -> TranslationInput {
    match input {
        Some(path) if path == Path::new("-") => TranslationInput::Stdin,
        Some(path) => TranslationInput::Path(path),
        None => TranslationInput::Stdin,
    }
}
