// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Front-end translation pipeline and shared diagnostics.
//!
//! This module owns the outer translation shape:
//!
//! 1. load LLVM IR or bitcode from a file or stdin
//! 2. parse it with inkwell
//! 3. reject truly module-level unsupported constructs
//! 4. build the symbol table and lower bodies
//! 5. attach native contract metadata
//! 6. serialize the final GOTO program
//!
//! Unsupported-feature detection is intentionally distributed across the
//! lowering code instead of maintained as a separate validator. That keeps the
//! implementation fail-closed without duplicating feature knowledge in two
//! places.

use std::fmt;
use std::path::PathBuf;
use std::{fs, io::Read};

use anyhow::{Context as _, Result, anyhow, bail};
use cprover_bindings::goto_program::Location;
use cprover_bindings::irep::goto_binary_serde::write_goto_binary_file;
use inkwell::context::Context;
use inkwell::llvm_sys::core::LLVMGetModuleInlineAsm;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;
use serde::Serialize;

use crate::contracts;
use crate::util;
use crate::{Config, InputFormat};

pub mod globals;
pub mod instructions;
mod rename;
pub mod symbols;
pub mod types;

pub(crate) const EH_ACTIVE_SYMBOL: &str = "__llvm2cprover_eh_active";
pub(crate) const EH_PTR_SYMBOL: &str = "__llvm2cprover_eh_ptr";
pub(crate) const EH_SELECTOR_SYMBOL: &str = "__llvm2cprover_eh_selector";

use self::symbols::SymbolBuilder;

/// Where LLVM input should be loaded from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranslationInput {
    Path(PathBuf),
    Stdin,
}

/// Structured translation diagnostics.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Coarse diagnostic class for callers and tests.
    pub kind: DiagnosticKind,
    /// Human-readable explanation of the failure.
    pub message: String,
    /// Best-effort symbol context for the failure.
    pub symbol: Option<String>,
    /// Best-effort source location for the failure.
    pub location: Option<Box<Location>>,
}

/// High-level kinds of translation diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    UnsupportedFeature(UnsupportedFeature),
    UnsupportedInitializer,
    UnsupportedType,
}

/// Phase 1 feature classes that currently fail closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedFeature {
    ControlFlow,
    Instruction,
    Intrinsic,
    InlineAsm,
    VarArgs,
}

/// Stable machine-readable representation of a translation failure.
///
/// Callers should use [`JsonDiagnostic::from_error`] rather than serializing
/// [`Diagnostic`] directly. This also gives non-translation failures, such as
/// malformed input or unreadable contract manifests, a consistent shape.
#[derive(Debug, Serialize)]
pub struct JsonDiagnostic {
    /// Stable, coarse failure category.
    pub kind: &'static str,
    /// Human-readable error including its context chain.
    pub message: String,
    /// More specific class for `unsupported_feature` diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_feature: Option<&'static str>,
    /// LLVM symbol being lowered when the error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Source coordinates recovered from LLVM debug metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
}

/// Stable source-location fields exposed in JSON diagnostics.
#[derive(Debug, Default, Serialize)]
pub struct JsonLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u64>,
}

/// Translation driver for one LLVM module.
///
/// A translator instance is cheap to create and only stores the requested
/// configuration. All LLVM state is rebuilt per translation.
#[derive(Debug)]
pub struct Translator {
    config: Config,
}

impl Translator {
    /// Create a translator for the provided configuration.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Parse, lower, and emit a GOTO binary for one input module.
    pub fn translate(&self, input: TranslationInput) -> Result<()> {
        let bytes = read_input(&input)?;
        let context = Context::create();
        let module = parse_module(&context, &input, self.config.input_format, &bytes)?;
        reject_module_inline_asm(&module)?;
        let contracts = contracts::load_contracts(&self.config.contracts)?;
        let machine_model = util::resolve_machine_model(&module, self.config.machine_model)?;
        if let Some(prefix) = self.config.rename_prefix.as_deref()
            && !prefix.is_empty()
        {
            if !contracts.is_empty() {
                bail!(
                    "--rename-prefix cannot currently be combined with --contracts; contract manifests name original LLVM symbols"
                );
            }
            rename::rename_module_symbols(&module, &machine_model, prefix)?;
        }
        let output = util::output_path(&input, self.config.output.as_deref());
        let mut symbols = SymbolBuilder::default().build(
            &module,
            machine_model,
            &contracts,
            self.config.unsound,
        )?;
        contracts::inject::inject_contracts(&mut symbols, &contracts)?;
        write_goto_binary_file(output.as_path(), &symbols);
        Ok(())
    }
}

fn read_input(input: &TranslationInput) -> Result<Vec<u8>> {
    match input {
        TranslationInput::Path(path) => {
            fs::read(path).with_context(|| format!("failed to read input file {}", path.display()))
        }
        TranslationInput::Stdin => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .context("failed to read LLVM input from stdin")?;
            Ok(bytes)
        }
    }
}

fn parse_module<'ctx>(
    context: &'ctx Context,
    input: &TranslationInput,
    requested_format: InputFormat,
    bytes: &[u8],
) -> Result<Module<'ctx>> {
    let format = util::detect_input_format(input.path_hint(), requested_format, bytes)?;
    let name = input
        .path_hint()
        .and_then(|path| path.as_path().to_str())
        .unwrap_or("stdin");
    // Inkwell 0.10 passes the complete slice length to LLVM and arranges the buffer's trailing
    // NUL itself. Supplying a terminator here would make it part of the bitcode payload.
    let buffer = MemoryBuffer::create_from_memory_range_copy(bytes, name);

    match format {
        InputFormat::Bitcode => Module::parse_bitcode_from_buffer(&buffer, context)
            .map_err(|err| anyhow!("failed to parse LLVM bitcode: {err}")),
        InputFormat::LlvmIr => context
            .create_module_from_ir(buffer)
            .map_err(|err| anyhow!("failed to parse LLVM IR: {err}")),
        InputFormat::Auto => unreachable!("input format should be resolved before parsing"),
    }
}

fn reject_module_inline_asm(module: &Module<'_>) -> Result<()> {
    if module_has_inline_asm(module) {
        return Err(Diagnostic::unsupported_feature(
            UnsupportedFeature::InlineAsm,
            "unsupported LLVM feature: module-level inline asm",
        )
        .into());
    }
    Ok(())
}

fn module_has_inline_asm(module: &Module<'_>) -> bool {
    let mut len = 0;
    unsafe { !LLVMGetModuleInlineAsm(module.as_mut_ptr(), &mut len).is_null() && len != 0 }
}

impl TranslationInput {
    fn path_hint(&self) -> Option<&PathBuf> {
        match self {
            TranslationInput::Path(path) => Some(path),
            TranslationInput::Stdin => None,
        }
    }
}

impl Diagnostic {
    /// Construct a diagnostic for an unsupported feature class.
    pub fn unsupported_feature(feature: UnsupportedFeature, message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::UnsupportedFeature(feature),
            message: message.into(),
            symbol: None,
            location: None,
        }
    }

    /// Construct a diagnostic for an unsupported lowered type.
    pub fn unsupported_type(message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::UnsupportedType,
            message: message.into(),
            symbol: None,
            location: None,
        }
    }

    /// Construct a diagnostic for an unsupported global initializer.
    pub fn unsupported_initializer(message: impl Into<String>) -> Self {
        Self {
            kind: DiagnosticKind::UnsupportedInitializer,
            message: message.into(),
            symbol: None,
            location: None,
        }
    }

    /// Attach symbol context when the caller knows which declaration failed.
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Attach source location context when the caller can recover it from LLVM metadata.
    pub fn with_location(mut self, location: Location) -> Self {
        if !location.is_none() {
            self.location = Some(Box::new(location));
        }
        self
    }
}

impl JsonDiagnostic {
    /// Convert any translation pipeline error into the stable JSON schema.
    pub fn from_error(error: &anyhow::Error) -> Self {
        let Some(diagnostic) = error.downcast_ref::<Diagnostic>() else {
            return Self {
                kind: "error",
                message: format!("{error:#}"),
                unsupported_feature: None,
                symbol: None,
                location: None,
            };
        };

        let (kind, unsupported_feature) = match diagnostic.kind {
            DiagnosticKind::UnsupportedFeature(feature) => {
                ("unsupported_feature", Some(feature.as_str()))
            }
            DiagnosticKind::UnsupportedInitializer => ("unsupported_initializer", None),
            DiagnosticKind::UnsupportedType => ("unsupported_type", None),
        };
        Self {
            kind,
            message: diagnostic.message.clone(),
            unsupported_feature,
            symbol: diagnostic.symbol.clone(),
            location: diagnostic.location.as_deref().map(JsonLocation::from),
        }
    }
}

impl UnsupportedFeature {
    fn as_str(self) -> &'static str {
        match self {
            Self::ControlFlow => "control_flow",
            Self::Instruction => "instruction",
            Self::Intrinsic => "intrinsic",
            Self::InlineAsm => "inline_asm",
            Self::VarArgs => "varargs",
        }
    }
}

impl From<&Location> for JsonLocation {
    fn from(location: &Location) -> Self {
        match location {
            Location::None | Location::PropertyUnknownLocation { .. } => Self::default(),
            Location::BuiltinFunction {
                function_name,
                line,
            } => Self {
                function: Some(function_name.to_string()),
                start_line: *line,
                ..Self::default()
            },
            Location::Loc {
                file,
                function,
                start_line,
                start_col,
                end_line,
                end_col,
                ..
            } => Self {
                file: Some(file.to_string()),
                function: function.map(|name| name.to_string()),
                start_line: Some(*start_line),
                start_column: *start_col,
                end_line: Some(*end_line),
                end_column: *end_col,
            },
            Location::Property {
                file,
                function,
                line,
                col,
                ..
            } => Self {
                file: Some(file.to_string()),
                function: function.map(|name| name.to_string()),
                start_line: Some(*line),
                start_column: *col,
                ..Self::default()
            },
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(symbol) = &self.symbol {
            write!(f, " [symbol: {symbol}]")?;
        }
        if let Some(location) = &self.location {
            write!(f, " [location: {}]", location.short_string())?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translator::symbols::SymbolBuilder;
    use inkwell::values::InstructionOpcode;

    const SIMPLE_IR: &str = "define void @f() {\nentry:\n  ret void\n}\n";

    #[test]
    fn parses_text_ir_from_bytes() {
        let context = Context::create();
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            SIMPLE_IR.as_bytes(),
        )
        .expect("failed to parse text IR");

        assert_eq!(module.get_name().to_str(), Ok("stdin"));
    }

    #[test]
    fn parses_bitcode_from_bytes() {
        let context = Context::create();
        let module = context.create_module("bitcode");
        let fn_type = context.void_type().fn_type(&[], false);
        let function = module.add_function("f", fn_type, None);
        let block = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(block);
        builder.build_return(None).expect("failed to build return");

        let bitcode = module.write_bitcode_to_memory();
        let payload = bitcode.as_slice();
        let parsed = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::Bitcode,
            payload,
        )
        .expect("failed to parse bitcode");

        assert_eq!(parsed.get_name().to_str(), Ok("stdin"));
    }

    #[test]
    fn parses_bitcode_payloads_that_end_in_zero() {
        let context = Context::create();
        let module = context.create_module("bitcode");
        let fn_type = context.void_type().fn_type(&[], false);
        let function = module.add_function("f", fn_type, None);
        let block = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(block);
        builder.build_return(None).expect("failed to build return");

        let bitcode = module.write_bitcode_to_memory();
        let payload = bitcode.as_slice();
        assert_eq!(payload.last(), Some(&0));

        let parsed = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::Bitcode,
            payload,
        )
        .expect("failed to parse bitcode payload");

        assert_eq!(parsed.get_name().to_str(), Ok("stdin"));
    }

    #[test]
    fn lowers_variadic_function_types() {
        let context = Context::create();
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            b"declare i32 @printf(ptr, ...)\n",
        )
        .expect("failed to parse varargs declaration");

        let symbols = SymbolBuilder::default()
            .build(
                &module,
                crate::util::resolve_machine_model(&module, crate::MachineModel::Auto)
                    .expect("failed to resolve machine model"),
                &crate::contracts::ContractMap::default(),
                false,
            )
            .expect("failed to build symbol table");
        let printf = symbols.lookup("printf").expect("printf symbol");

        assert!(printf.typ.is_variadic_code());
    }

    #[test]
    fn rejects_module_inline_asm() {
        let context = Context::create();
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            br#"
                module asm "nop"

                define void @f() {
                entry:
                  ret void
                }
            "#,
        )
        .expect("failed to parse module asm");

        let error = reject_module_inline_asm(&module).expect_err("module asm should be rejected");
        let diagnostic = error
            .downcast_ref::<Diagnostic>()
            .expect("expected structured diagnostic");

        assert_eq!(
            diagnostic.kind,
            DiagnosticKind::UnsupportedFeature(UnsupportedFeature::InlineAsm)
        );
    }

    #[test]
    fn json_uses_stable_varargs_feature_name() {
        let error = anyhow::Error::new(Diagnostic::unsupported_feature(
            UnsupportedFeature::VarArgs,
            "unsupported variadic setup",
        ));
        let diagnostic = JsonDiagnostic::from_error(&error);

        assert_eq!(diagnostic.kind, "unsupported_feature");
        assert_eq!(diagnostic.unsupported_feature, Some("varargs"));
    }

    #[test]
    fn branch_operand_ordering() {
        // The LLVM C API returns conditional branch operands in a different order
        // than the textual IR syntax `br i1 %cond, label %true, label %false`:
        //
        //   operand 0 = condition
        //   operand 1 = false destination  (IfFalse)
        //   operand 2 = true destination   (IfTrue)
        //
        // This matches the raw Use array layout in CondBrInst [Cond, IfFalse, IfTrue]
        // but is the REVERSE of getSuccessor(0)=IfTrue / getSuccessor(1)=IfFalse.
        //
        // LLVM commit 750088fa8aba (March 2026) fixes this so that operand 1 = IfTrue
        // and operand 2 = IfFalse.  When we upgrade past that commit this test will
        // fail, reminding us to swap the operand indices in lower_branch.
        let context = Context::create();
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            br#"
                define i32 @main() {
                entry:
                  %cond = icmp eq i32 1, 1
                  br i1 %cond, label %yes, label %no
                yes:
                  ret i32 42
                no:
                  ret i32 99
                }
            "#,
        )
        .expect("failed to parse branch test module");

        let function = module.get_function("main").expect("missing main");
        let br = function
            .get_first_basic_block()
            .and_then(|block| block.get_terminator())
            .expect("missing br instruction");
        assert_eq!(br.get_opcode(), InstructionOpcode::Br);
        assert_eq!(br.get_num_operands(), 3);

        let op0 = br.get_operand(0).expect("missing operand 0");
        assert!(matches!(op0, inkwell::values::Operand::Value(_)));

        let op1 = br.get_operand(1).expect("missing operand 1");
        let op2 = br.get_operand(2).expect("missing operand 2");
        let op1_name = match op1 {
            inkwell::values::Operand::Block(bb) => bb.get_name().to_str().unwrap().to_owned(),
            _ => panic!("operand 1 should be a block"),
        };
        let op2_name = match op2 {
            inkwell::values::Operand::Block(bb) => bb.get_name().to_str().unwrap().to_owned(),
            _ => panic!("operand 2 should be a block"),
        };

        assert_eq!(op1_name, "no", "C API operand 1 is the false destination");
        assert_eq!(op2_name, "yes", "C API operand 2 is the true destination");
    }
}
