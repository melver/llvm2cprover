// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use llvm2cprover::{
    Config, InputFormat, JsonDiagnostic, MachineModel, TranslationInput, UnsoundFeature,
    UnsoundOptions, translate,
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

    /// Prefix translated LLVM function and global definitions.
    #[arg(long)]
    rename_prefix: Option<String>,

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
        contracts: cli.contracts,
        rename_prefix: cli.rename_prefix,
        machine_model: cli.machine_model.into(),
        input_format: cli.input_format.into(),
        unsound,
        report: cli.report,
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
        Err(error) => {
            match diagnostics {
                DiagnosticFormat::Text => eprintln!("Error: {error:?}"),
                DiagnosticFormat::Json => {
                    let diagnostic = JsonDiagnostic::from_error(&error);
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
    }
}

fn input_from_cli(input: Option<PathBuf>) -> TranslationInput {
    match input {
        Some(path) if path == Path::new("-") => TranslationInput::Stdin,
        Some(path) => TranslationInput::Path(path),
        None => TranslationInput::Stdin,
    }
}
