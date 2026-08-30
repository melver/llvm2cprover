// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use llvm2cprover::{
    Config, InputFormat, JsonDiagnostic, MachineModel, TranslationInput, translate,
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

    #[arg(long)]
    unsound: bool,

    /// Select human-readable or machine-readable error diagnostics.
    #[arg(long, value_enum, default_value_t = DiagnosticFormat::Text)]
    diagnostics: DiagnosticFormat,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let diagnostics = cli.diagnostics;
    let config = Config {
        output: cli.output,
        contracts: cli.contracts,
        rename_prefix: cli.rename_prefix,
        machine_model: cli.machine_model.into(),
        input_format: cli.input_format.into(),
        unsound: cli.unsound,
    };

    match translate(input_from_cli(cli.input), config) {
        Ok(()) => std::process::ExitCode::SUCCESS,
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
