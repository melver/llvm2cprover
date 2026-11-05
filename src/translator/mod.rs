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
//! 6. optionally goto-convert the bodies, and serialize the final GOTO program
//!
//! Unsupported-feature detection is intentionally distributed across the
//! lowering code instead of maintained as a separate validator. That keeps the
//! implementation fail-closed without duplicating feature knowledge in two
//! places.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::{fs, io::Read};

use anyhow::{Context as _, Result, anyhow, bail};
use cprover_bindings::goto_program::Location;
use cprover_bindings::irep::goto_binary_serde::write_goto_binary_file_with_functions;
use inkwell::context::Context;
use inkwell::llvm_sys::LLVMAtomicOrdering;
use inkwell::llvm_sys::core::{
    LLVMAliasGetAliasee, LLVMGetBasicBlockParent, LLVMGetCalledValue, LLVMGetFirstUse,
    LLVMGetInstructionParent, LLVMGetNextUse, LLVMGetNumOperands, LLVMGetOperand, LLVMGetOrdering,
    LLVMGetUser, LLVMIsACallInst, LLVMIsAGlobalAlias, LLVMIsAInstruction, LLVMIsAInvokeInst,
};
use inkwell::llvm_sys::prelude::LLVMValueRef;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::Module;
use inkwell::values::{
    AsValueRef, BasicValue, BasicValueEnum, FunctionValue, InstructionOpcode, InstructionValue,
    Operand,
};
use serde::Serialize;

use crate::contracts;
use crate::util;
use crate::{Config, InputFormat};

mod asm_manifest;
mod fp_dispatch;
pub mod globals;
mod goto_convert;
pub mod instructions;
mod rename;
mod report;
pub mod symbols;
pub mod types;

pub(crate) const EH_ACTIVE_SYMBOL: &str = "__llvm2cprover_eh_active";
pub(crate) const EH_PTR_SYMBOL: &str = "__llvm2cprover_eh_ptr";
pub(crate) const EH_SELECTOR_SYMBOL: &str = "__llvm2cprover_eh_selector";

pub(crate) fn is_metadata_noop_intrinsic(name: &str) -> bool {
    name.starts_with("llvm.dbg.") || name.starts_with("llvm.experimental.noalias.scope.decl")
}

/// Peel `GlobalAlias` wrappers (`@a = alias ..., ptr @target`) to their aliasee.
pub(super) fn unwrap_global_alias(mut raw: LLVMValueRef) -> LLVMValueRef {
    while !raw.is_null() && unsafe { !LLVMIsAGlobalAlias(raw).is_null() } {
        raw = unsafe { LLVMAliasGetAliasee(raw) };
    }
    raw
}

/// Direct `Function` callee of a call site, following `GlobalAlias` wrappers.
pub(super) fn direct_callee(instruction: InstructionValue<'_>) -> Option<FunctionValue<'_>> {
    let raw = unwrap_global_alias(unsafe { LLVMGetCalledValue(instruction.as_value_ref()) });
    unsafe { FunctionValue::new(raw) }
}

pub use self::asm_manifest::AsmManifest;
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

/// LLVM feature classes that currently fail closed.
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

    /// Parse, lower, and emit a GOTO binary for one input module (or, with `emit_llvm`, the
    /// transformed LLVM module as bitcode).
    pub fn translate(&self, input: TranslationInput) -> Result<()> {
        let bytes = read_input(&input)?;
        let context = Context::create();
        let module = parse_module(&context, &input, self.config.input_format, &bytes)?;
        let mut contracts = contracts::load_contracts(&self.config.contracts)?;
        if self.config.enforce_contract.len() > 1 {
            bail!("exactly one --enforce-contract target may be selected per translation");
        }
        let resolved_machine_model =
            util::resolve_machine_model_kind(&module, self.config.machine_model)?;
        let machine_model = util::machine_model_for(&module, resolved_machine_model);
        if let Some(prefix) = self.config.rename_prefix.as_deref()
            && !prefix.is_empty()
        {
            let renames = rename::rename_module_symbols(&module, &machine_model, prefix)?;
            contracts = contracts.renamed(&renames);
        }
        if self.config.emit_llvm {
            let output = self
                .config
                .output
                .as_ref()
                .context("--emit-llvm requires an explicit output path")?;
            if !module.write_bitcode_to_path(output) {
                bail!("failed to write LLVM bitcode to {}", output.display());
            }
            return Ok(());
        }
        reject_module_inline_asm(&module)?;
        let cone = (!self.config.entry.is_empty())
            .then(|| {
                symbols::ConeOfInfluence::compute(
                    &module,
                    &contracts,
                    &self.config.entry,
                    &self.config.replace_with_contract,
                    &self.config.enforce_contract,
                )
            })
            .transpose()?;
        let contracts = if let Some(cone) = &cone {
            contracts.scoped_to(
                &cone.reachable_functions,
                &self.config.replace_with_contract,
                &self.config.enforce_contract,
            )
        } else {
            contracts
        };
        validate_inline_asm_model_definitions(&module, &contracts)?;
        validate_assigns_helpers(&module, &contracts)?;
        validate_contract_replacement_uses(
            &module,
            &self.config.replace_with_contract,
            cone.as_ref(),
        )?;
        validate_contract_enforcement_uses(&module, &self.config.enforce_contract, cone.as_ref())?;
        validate_contract_enforcement_runtime(
            &module,
            &self.config.enforce_contract,
            cone.as_ref(),
        )?;
        let output = util::output_path(&input, self.config.output.as_deref());
        if self.config.report.as_deref() == Some(output.as_path()) {
            bail!("--report must not overwrite the GOTO output");
        }
        let fp_dispatch = match self.config.fp_groups.as_deref() {
            Some(path) => {
                let cone = cone.as_ref().context("--fp-groups requires --entry")?;
                Some(fp_dispatch::FpDispatch::load(
                    path,
                    &module,
                    &cone.address_taken_functions,
                )?)
            }
            None => None,
        };
        let mut report = report::ReportCollector::new(self.config.report.is_some());
        let mut symbols = SymbolBuilder::default().build_reporting(
            &module,
            machine_model,
            &contracts,
            cone.as_ref(),
            &self.config.replace_with_contract,
            &self.config.enforce_contract,
            &self.config.unsound,
            fp_dispatch.as_ref(),
            &mut report,
        )?;
        contracts::inject::inject_contracts_with_replacements(
            &mut symbols,
            &contracts,
            &self.config.replace_with_contract,
            &self.config.enforce_contract,
        )?;
        if !self.config.enforce_contract.is_empty() {
            // DFCC must introduce `free` together with its contracts runtime. The bindings seed an
            // unused declaration eagerly, which prevents CBMC's library linker from supplying the
            // real body, so omit that declaration after rejecting input uses above.
            const UNUSED_FREE: &str = "__llvm2cprover_unused_builtin_free";
            if symbols.lookup(UNUSED_FREE).is_some() {
                bail!(
                    "DFCC runtime compatibility symbol `{UNUSED_FREE}` collides with an input symbol"
                );
            }
            symbols.lookup_mut("free").expect("builtin free").name = UNUSED_FREE.into();
        }
        symbols::add_missing_parameter_symbols(&mut symbols);
        let functions = if self.config.goto_convert {
            goto_convert::convert(&symbols)?
        } else {
            Vec::new()
        };
        write_goto_binary_file_with_functions(output.as_path(), &symbols, &functions);
        if let Some(path) = self.config.report.as_deref() {
            report::write_report(
                path,
                &module,
                resolved_machine_model,
                &contracts,
                &self.config.enforce_contract,
                &symbols,
                report,
            )?;
        }
        Ok(())
    }
}

/// Parse an LLVM module and generate inline-assembly model stubs without lowering it.
pub fn emit_asm_manifest(
    input: TranslationInput,
    input_format: InputFormat,
    contract_paths: &[PathBuf],
) -> Result<AsmManifest> {
    let bytes = read_input(&input)?;
    let context = Context::create();
    let module = parse_module(&context, &input, input_format, &bytes)?;
    let contracts = contracts::load_contracts(contract_paths)?;
    asm_manifest::generate(&module, &contracts)
}

/// Check that every assigns helper's body is observationally read-only.
///
/// The helper's *form* — compiled, pointer-returning, concrete pointee — is validated once
/// against the lowered symbol table in [`contracts::inject::validate_helper`]. Helpers that do
/// not have that form are skipped here so the single canonical error comes from there.
fn validate_assigns_helpers(module: &Module<'_>, contracts: &contracts::ContractMap) -> Result<()> {
    for (target, contract) in contracts.iter() {
        for helper in contract.assigns.iter().flatten() {
            let Some(function) = module.get_function(helper) else {
                continue;
            };
            let returns_pointer = function
                .get_type()
                .get_return_type()
                .is_some_and(|typ| typ.is_pointer_type());
            if !returns_pointer || function.count_basic_blocks() == 0 {
                continue;
            }
            for block in function.get_basic_blocks() {
                let mut instruction = block.get_first_instruction();
                while let Some(current) = instruction {
                    validate_assigns_instruction(target, helper, current)?;
                    instruction = current.get_next_instruction();
                }
            }
        }
    }
    Ok(())
}

fn validate_assigns_instruction(
    target: &str,
    helper: &str,
    instruction: InstructionValue<'_>,
) -> Result<()> {
    match instruction.get_opcode() {
        InstructionOpcode::Store => {
            if instruction.get_volatile().unwrap_or(false) {
                bail!("assigns helper `{helper}` for `{target}` contains a volatile store");
            }
            if unsafe { LLVMGetOrdering(instruction.as_value_ref()) }
                != LLVMAtomicOrdering::LLVMAtomicOrderingNotAtomic
            {
                bail!("assigns helper `{helper}` for `{target}` contains an atomic store");
            }
            let destination = match instruction.get_operand(1) {
                Some(Operand::Value(value)) => value,
                _ => bail!("store in assigns helper `{helper}` has no pointer destination"),
            };
            if !is_local_alloca_derived(destination) {
                bail!("assigns helper `{helper}` for `{target}` writes nonlocal state");
            }
        }
        InstructionOpcode::Load => {
            if instruction.get_volatile().unwrap_or(false)
                || unsafe { LLVMGetOrdering(instruction.as_value_ref()) }
                    != LLVMAtomicOrdering::LLVMAtomicOrderingNotAtomic
            {
                bail!(
                    "assigns helper `{helper}` for `{target}` contains a volatile or atomic load"
                );
            }
        }
        InstructionOpcode::AtomicRMW | InstructionOpcode::AtomicCmpXchg => {
            bail!("assigns helper `{helper}` for `{target}` contains an atomic write");
        }
        InstructionOpcode::Fence => {
            bail!("assigns helper `{helper}` for `{target}` contains an atomic fence");
        }
        InstructionOpcode::Call | InstructionOpcode::Invoke | InstructionOpcode::CallBr => {
            let called_name = direct_callee(instruction)
                .map(|function| function.get_name().to_string_lossy().into_owned());
            let harmless = called_name.as_deref().is_some_and(|name| {
                is_metadata_noop_intrinsic(name) || name.starts_with("llvm.lifetime.")
            });
            if !harmless {
                bail!(
                    "assigns helper `{helper}` for `{target}` calls a potentially side-effecting function"
                );
            }
        }
        InstructionOpcode::Return => {
            if let Some(Operand::Value(value)) = instruction.get_operand(0)
                && value.get_type().is_pointer_type()
                && is_local_alloca_derived(value)
            {
                bail!(
                    "assigns helper `{helper}` for `{target}` returns a pointer to its own local allocation"
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_local_alloca_derived(value: BasicValueEnum<'_>) -> bool {
    let Some(instruction) = value.as_instruction_value() else {
        return false;
    };
    match instruction.get_opcode() {
        InstructionOpcode::Alloca => true,
        InstructionOpcode::BitCast
        | InstructionOpcode::AddrSpaceCast
        | InstructionOpcode::GetElementPtr => match instruction.get_operand(0) {
            Some(Operand::Value(base)) => is_local_alloca_derived(base),
            _ => false,
        },
        _ => false,
    }
}

fn validate_contract_enforcement_runtime(
    module: &Module<'_>,
    enforcements: &BTreeSet<String>,
    cone: Option<&symbols::ConeOfInfluence>,
) -> Result<()> {
    if enforcements.is_empty() {
        return Ok(());
    }
    let free_used = match cone {
        Some(cone) => cone.reachable_functions.contains("free"),
        None => module.get_function("free").is_some_and(|free| {
            free.count_basic_blocks() != 0
                || unsafe { !LLVMGetFirstUse(free.as_value_ref()).is_null() }
        }),
    };
    if free_used {
        bail!(
            "contract enforcement currently cannot be combined with input uses or definitions of `free`"
        );
    }
    Ok(())
}

fn validate_contract_enforcement_uses(
    module: &Module<'_>,
    enforcements: &BTreeSet<String>,
    cone: Option<&symbols::ConeOfInfluence>,
) -> Result<()> {
    for name in enforcements {
        let Some(function) = module.get_function(name) else {
            continue;
        };
        // Direct self-recursion is the degenerate call cycle, so the cycle walk below reports it.
        validate_direct_call_uses(
            function.as_value_ref(),
            name,
            cone,
            || format!("contract enforcement for `{name}` does not support invoke uses"),
            || {
                format!(
                    "contract enforcement for `{name}` requires direct calls only; its address is used indirectly"
                )
            },
        )?;
        validate_contract_enforcement_call_cycle(function.as_value_ref(), name, cone)?;
    }
    Ok(())
}

fn validate_contract_enforcement_call_cycle(
    target: LLVMValueRef,
    name: &str,
    cone: Option<&symbols::ConeOfInfluence>,
) -> Result<()> {
    let mut visited = BTreeSet::from([target as usize]);
    let mut pending = vec![target];
    while let Some(callee) = pending.pop() {
        for_each_non_alias_use(callee, |user, callee_or_alias| {
            if is_direct_call_to(user, callee_or_alias) {
                let caller = unsafe { LLVMGetBasicBlockParent(LLVMGetInstructionParent(user)) };
                if cone.is_some_and(|cone| {
                    unsafe { FunctionValue::new(caller) }.is_none_or(|function| {
                        !cone
                            .lowered_functions
                            .contains(function.get_name().to_string_lossy().as_ref())
                    })
                }) {
                    return Ok(());
                }
                if caller == target {
                    bail!(
                        "contract enforcement for `{name}` does not support recursive call cycles"
                    );
                }
                if visited.insert(caller as usize) {
                    pending.push(caller);
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn validate_inline_asm_model_definitions(
    module: &Module<'_>,
    contracts: &contracts::ContractMap,
) -> Result<()> {
    for mapping in contracts.inline_asm_models() {
        let model = module
            .get_function(&mapping.model)
            .ok_or_else(|| anyhow!("inline asm model function `{}` is missing", mapping.model))?;
        if model.count_basic_blocks() == 0 {
            bail!(
                "inline asm model function `{}` must be defined in the input module",
                mapping.model
            );
        }
    }
    Ok(())
}

fn validate_contract_replacement_uses(
    module: &Module<'_>,
    replacements: &BTreeSet<String>,
    cone: Option<&symbols::ConeOfInfluence>,
) -> Result<()> {
    for name in replacements {
        let Some(function) = module.get_function(name) else {
            continue;
        };
        validate_direct_call_uses(
            function.as_value_ref(),
            name,
            cone,
            || invoke_replacement_message(name),
            || {
                format!(
                    "contract replacement for `{name}` requires direct calls only; its address is used indirectly"
                )
            },
        )?;
    }
    Ok(())
}

/// Whether `user` is a plain direct call to `callee` that does not also leak its address.
///
/// A direct call names the callee exactly once; a second occurrence means the address is also
/// passed as an argument.
fn is_direct_call_to(user: LLVMValueRef, callee: LLVMValueRef) -> bool {
    unsafe {
        (!LLVMIsACallInst(user).is_null() || !LLVMIsAInvokeInst(user).is_null())
            && LLVMGetCalledValue(user) == callee
            && self_operand_count(user, callee) == 1
    }
}

/// Check that every use of `function` (including through `GlobalAlias` wrappers) is a direct call.
///
/// Shared by the replacement and enforcement pre-passes, which differ only in wording.
fn validate_direct_call_uses(
    function: LLVMValueRef,
    name: &str,
    cone: Option<&symbols::ConeOfInfluence>,
    invoke_message: impl Fn() -> String,
    indirect_message: impl Fn() -> String,
) -> Result<()> {
    if cone.is_some_and(|cone| cone.address_taken_functions.contains(name)) {
        bail!("{}", indirect_message());
    }
    for_each_non_alias_use(function, |user, callee_or_alias| {
        if let Some(cone) = cone {
            let Some(caller) = enclosing_function_name(user) else {
                return Ok(());
            };
            if !cone.lowered_functions.contains(&caller) {
                return Ok(());
            }
        }
        if unsafe { !LLVMIsAInvokeInst(user).is_null() } {
            bail!("{}", invoke_message());
        }
        if !is_direct_call_to(user, callee_or_alias) {
            bail!("{}", indirect_message());
        }
        Ok(())
    })
}

/// Visit every non-alias user of `root`, peeling `GlobalAlias` wrappers that refer to it.
fn for_each_non_alias_use(
    root: LLVMValueRef,
    mut visit: impl FnMut(LLVMValueRef, LLVMValueRef) -> Result<()>,
) -> Result<()> {
    let mut seen = BTreeSet::from([root as usize]);
    let mut worklist = vec![root];
    while let Some(value) = worklist.pop() {
        let mut usage = unsafe { LLVMGetFirstUse(value) };
        while !usage.is_null() {
            let user = unsafe { LLVMGetUser(usage) };
            if unsafe { !LLVMIsAGlobalAlias(user).is_null() } {
                if seen.insert(user as usize) {
                    worklist.push(user);
                }
            } else {
                visit(user, value)?;
            }
            usage = unsafe { LLVMGetNextUse(usage) };
        }
    }
    Ok(())
}

fn enclosing_function_name(user: LLVMValueRef) -> Option<String> {
    if unsafe { LLVMIsAInstruction(user) }.is_null() {
        return None;
    }
    let block = unsafe { LLVMGetInstructionParent(user) };
    if block.is_null() {
        return None;
    }
    let function = unsafe { FunctionValue::new(LLVMGetBasicBlockParent(block)) }?;
    Some(function.get_name().to_string_lossy().into_owned())
}

/// How many operands of `user` refer to `value`.
fn self_operand_count(user: LLVMValueRef, value: LLVMValueRef) -> usize {
    let operand_count = unsafe { LLVMGetNumOperands(user) };
    debug_assert!(operand_count >= 0);
    (0..operand_count as u32)
        .filter(|index| unsafe { LLVMGetOperand(user, *index) == value })
        .count()
}

/// Shared wording for the one unsupported replacement call form.
pub(crate) fn invoke_replacement_message(name: &str) -> String {
    format!("contract replacement for `{name}` does not support invoke")
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
    asm_manifest::has_module_inline_asm(module)
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
                &crate::UnsoundOptions::default(),
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
        // Prior to LLVM 23 (commit 750088fa8aba), the LLVM C API returned
        // conditional branch operands in reverse order (operand 1 = IfFalse,
        // operand 2 = IfTrue). Verify that CondBrInst operands match the
        // textual IR syntax `br i1 %cond, label %true, label %false`:
        //
        //   operand 0 = condition
        //   operand 1 = true destination   (IfTrue)
        //   operand 2 = false destination  (IfFalse)
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
        assert_eq!(br.get_opcode(), InstructionOpcode::CondBr);
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

        assert_eq!(op1_name, "yes", "C API operand 1 is the true destination");
        assert_eq!(op2_name, "no", "C API operand 2 is the false destination");
    }

    #[test]
    fn contract_use_validation_peels_global_aliases() {
        let context = Context::create();
        let direct_ir = br#"
            @unused_alias = alias i32 (i32), ptr @target
            @call_alias = alias i32 (i32), ptr @target

            define i32 @target(i32 %x) {
            entry:
              ret i32 %x
            }

            define i32 @main() {
            entry:
              %r = call i32 @call_alias(i32 1)
              ret i32 %r
            }
        "#;
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            direct_ir,
        )
        .expect("parse direct alias IR");
        let target = BTreeSet::from([String::from("target")]);
        validate_contract_replacement_uses(&module, &target, None)
            .expect("direct alias calls should allow replacement");
        validate_contract_enforcement_uses(&module, &target, None)
            .expect("direct alias calls should allow enforcement");

        let invoke_ir = br#"
            @alias_target = alias void (), ptr @target
            declare i32 @__gxx_personality_v0(...)

            define void @target() {
            entry:
              ret void
            }

            define void @main() personality ptr @__gxx_personality_v0 {
            entry:
              invoke void @alias_target() to label %ok unwind label %lpad
            ok:
              ret void
            lpad:
              %lp = landingpad { ptr, i32 } cleanup
              ret void
            }
        "#;
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            invoke_ir,
        )
        .expect("parse invoke alias IR");
        let entry = BTreeSet::from([String::from("main")]);
        let cone = symbols::ConeOfInfluence::compute(
            &module,
            &contracts::ContractMap::default(),
            &entry,
            &BTreeSet::new(),
            &target,
        )
        .expect("compute cone");
        let err = validate_contract_enforcement_uses(&module, &target, Some(&cone))
            .expect_err("invoke through alias must fail closed");
        assert!(err.to_string().contains("invoke"));

        let rec_ir = br#"
            @rec_alias = alias i32 (i32), ptr @target

            define i32 @target(i32 %x) {
            entry:
              %r = call i32 @rec_alias(i32 %x)
              ret i32 %r
            }
        "#;
        let module = parse_module(
            &context,
            &TranslationInput::Stdin,
            InputFormat::LlvmIr,
            rec_ir,
        )
        .expect("parse recursive alias IR");
        let cone = symbols::ConeOfInfluence::compute(
            &module,
            &contracts::ContractMap::default(),
            &target,
            &BTreeSet::new(),
            &target,
        )
        .expect("compute cone");
        let err = validate_contract_enforcement_uses(&module, &target, Some(&cone))
            .expect_err("recursion through alias must fail closed");
        assert!(err.to_string().contains("recursive call cycles"));
    }
}
