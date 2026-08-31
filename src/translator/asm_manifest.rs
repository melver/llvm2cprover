// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Inventory and manifest generation for LLVM inline assembly.
//!
//! Scanning uses the same metadata decoder and empty-barrier classification as
//! instruction lowering. This keeps generated exact-match rules aligned with
//! the rules that consume them, without performing any GOTO lowering.

use std::collections::{BTreeMap, BTreeSet};
use std::slice;

use anyhow::{Context as _, Result, bail};
use inkwell::llvm_sys::core::{
    LLVMGetCalledValue, LLVMGetInlineAsmAsmString, LLVMGetInlineAsmCanUnwind,
    LLVMGetInlineAsmConstraintString, LLVMGetInlineAsmDialect, LLVMGetInlineAsmHasSideEffects,
    LLVMGetInlineAsmNeedsAlignedStack, LLVMGetModuleInlineAsm, LLVMGetNumArgOperands,
    LLVMIsAInlineAsm,
};
use inkwell::llvm_sys::prelude::LLVMValueRef;
use inkwell::module::Module;
use inkwell::values::{AsValueRef, InstructionOpcode, InstructionValue};

use serde::Serialize;

use crate::contracts::{ContractMap, InlineAsmDialect, InlineAsmModel, InlineAsmSite};

/// Wrapper matching the manifest's `[[inline_asm]]` array-of-tables shape.
#[derive(Serialize)]
struct ManifestSkeleton {
    inline_asm: Vec<InlineAsmModel>,
}

/// One of LLVM's pointer/length inline-assembly string accessors.
type InlineAsmStringGetter =
    unsafe extern "C" fn(LLVMValueRef, *mut usize) -> *const std::os::raw::c_char;

/// Exact LLVM metadata shared by manifest generation and instruction lowering.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct InlineAsmMetadata {
    pub(crate) asm: String,
    pub(crate) constraints: String,
    pub(crate) dialect: InlineAsmDialect,
    pub(crate) side_effects: bool,
    pub(crate) align_stack: bool,
    pub(crate) can_unwind: bool,
}

/// A generated, redirectable inline-assembly manifest and its inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsmManifest {
    /// TOML skeleton suitable for writing directly to a manifest file.
    pub toml: String,
    /// Number of distinct exact-match templates emitted.
    pub distinct_templates: usize,
    /// Number of unmatched call sites represented by the emitted templates.
    pub sites: usize,
    /// Number of unwind-capable sites omitted because models cannot replace them.
    pub unwind_sites: usize,
    /// Whether unsupported module-level assembly was present and omitted.
    pub has_module_asm: bool,
}

/// The owned form of [`InlineAsmSite`]'s exact-match key, used to group sites.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TemplateKey {
    asm: String,
    constraints: String,
    dialect: InlineAsmDialect,
    side_effects: bool,
    align_stack: bool,
}

impl TemplateKey {
    /// Build the manifest entry this template needs, with a placeholder model to fill in.
    ///
    /// Going through [`InlineAsmModel`] means the emitted schema is the one the loader parses;
    /// a new manifest field breaks this constructor instead of silently going unemitted.
    fn into_model(self, function: Option<String>) -> InlineAsmModel {
        InlineAsmModel {
            asm: self.asm,
            constraints: self.constraints,
            dialect: self.dialect,
            side_effects: self.side_effects,
            align_stack: self.align_stack,
            function,
            model: "...".to_owned(),
        }
    }
}

#[derive(Debug, Default)]
struct TemplateSites {
    /// Enclosing function name to the model signatures its sites require.
    functions: BTreeMap<String, BTreeSet<String>>,
    sites: usize,
    qualified_conflict: bool,
}

impl TemplateSites {
    /// Every model signature this template needs, across all enclosing functions.
    fn signatures(&self) -> BTreeSet<&String> {
        self.functions.values().flatten().collect()
    }
}

/// Scan a parsed module and render models for every unmatched, modellable assembly site.
pub(crate) fn generate(module: &Module<'_>, contracts: &ContractMap) -> Result<AsmManifest> {
    let has_module_asm = has_module_inline_asm(module);
    let mut templates = BTreeMap::<TemplateKey, TemplateSites>::new();
    let mut unwind_sites = 0;

    for function in module.get_functions() {
        let function_name = function.get_name().to_string_lossy().into_owned();
        for block in function.get_basic_blocks() {
            for instruction in block.get_instructions() {
                let opcode = instruction.get_opcode();
                if !matches!(
                    opcode,
                    InstructionOpcode::Call | InstructionOpcode::Invoke | InstructionOpcode::CallBr
                ) {
                    continue;
                }
                let raw_instruction = instruction.as_value_ref();
                let raw_asm = unsafe { LLVMGetCalledValue(raw_instruction) };
                if unsafe { LLVMIsAInlineAsm(raw_asm) }.is_null() {
                    continue;
                }
                let metadata = decode_inline_asm(raw_asm)?;
                let argument_count = unsafe { LLVMGetNumArgOperands(raw_instruction) };
                if is_elided_barrier(
                    opcode,
                    &metadata,
                    instruction.get_type().is_void_type(),
                    argument_count,
                ) {
                    continue;
                }
                if metadata.can_unwind {
                    unwind_sites += 1;
                    continue;
                }

                let site = InlineAsmSite {
                    asm: &metadata.asm,
                    constraints: &metadata.constraints,
                    dialect: metadata.dialect,
                    side_effects: metadata.side_effects,
                    align_stack: metadata.align_stack,
                    function: &function_name,
                };
                if contracts.inline_asm_model(&site).is_some() {
                    continue;
                }

                let signature = llvm_model_signature(instruction, argument_count)?;
                let key = TemplateKey {
                    asm: metadata.asm,
                    constraints: metadata.constraints,
                    dialect: metadata.dialect,
                    side_effects: metadata.side_effects,
                    align_stack: metadata.align_stack,
                };
                // A rule already matching this template for some other function must be
                // narrowed too, or the emitted stub would overlap it and be rejected.
                let qualified_conflict = contracts.inline_asm_models().any(|mapping| {
                    mapping.same_assembly(
                        &key.asm,
                        &key.constraints,
                        key.dialect,
                        key.side_effects,
                        key.align_stack,
                    )
                });
                let sites = templates.entry(key).or_default();
                sites.qualified_conflict |= qualified_conflict;
                sites
                    .functions
                    .entry(function_name.clone())
                    .or_default()
                    .insert(signature);
                sites.sites += 1;
            }
        }
    }

    let site_count = templates.values().map(|entry| entry.sites).sum();
    let mut toml = String::new();
    for (index, (template, sites)) in templates.iter().enumerate() {
        if index != 0 {
            toml.push('\n');
        }
        let signatures = sites.signatures();
        let signatures_vary_by_function = signatures.len() > 1
            && (sites.functions.values())
                .any(|only| only.iter().collect::<BTreeSet<_>>() != signatures);
        if sites.qualified_conflict || signatures_vary_by_function {
            for (function_index, (function, function_signatures)) in
                sites.functions.iter().enumerate()
            {
                if function_index != 0 {
                    toml.push('\n');
                }
                render_entry(
                    &mut toml,
                    template.clone(),
                    &function_signatures.iter().collect(),
                    slice::from_ref(function),
                    Some(function.clone()),
                )?;
            }
        } else {
            let functions = sites.functions.keys().cloned().collect::<Vec<_>>();
            render_entry(&mut toml, template.clone(), &signatures, &functions, None)?;
        }
    }

    Ok(AsmManifest {
        distinct_templates: templates.len(),
        sites: site_count,
        unwind_sites,
        has_module_asm,
        toml,
    })
}

fn render_entry(
    output: &mut String,
    template: TemplateKey,
    signatures: &BTreeSet<&String>,
    functions: &[String],
    qualifier: Option<String>,
) -> Result<()> {
    output.push_str("# Enclosing functions: ");
    output.push_str(
        &functions
            .iter()
            .map(|name| format!("{name:?}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    output.push('\n');
    for signature in signatures {
        output.push_str("# Required model signature: ");
        output.push_str(signature);
        output.push('\n');
    }
    if signatures.len() > 1 {
        output.push_str(
            "# NOTE: an enclosing function uses this template with incompatible signatures; function qualification cannot distinguish those sites.\n",
        );
    }
    let skeleton = ManifestSkeleton {
        inline_asm: vec![template.into_model(qualifier)],
    };
    output.push_str(
        &toml::to_string(&skeleton).context("failed to render inline-assembly manifest")?,
    );
    Ok(())
}

/// Decode exactly the metadata used by [`InlineAsmSite`] matching.
pub(crate) fn decode_inline_asm(asm: LLVMValueRef) -> Result<InlineAsmMetadata> {
    Ok(InlineAsmMetadata {
        asm: inline_asm_string(asm, LLVMGetInlineAsmAsmString, "template")?,
        constraints: inline_asm_string(asm, LLVMGetInlineAsmConstraintString, "constraints")?,
        dialect: match unsafe { LLVMGetInlineAsmDialect(asm) } {
            inkwell::llvm_sys::LLVMInlineAsmDialect::LLVMInlineAsmDialectATT => {
                InlineAsmDialect::Att
            }
            inkwell::llvm_sys::LLVMInlineAsmDialect::LLVMInlineAsmDialectIntel => {
                InlineAsmDialect::Intel
            }
        },
        side_effects: unsafe { LLVMGetInlineAsmHasSideEffects(asm) != 0 },
        align_stack: unsafe { LLVMGetInlineAsmNeedsAlignedStack(asm) != 0 },
        can_unwind: unsafe { LLVMGetInlineAsmCanUnwind(asm) != 0 },
    })
}

/// Whether lowering elides this site as a no-op barrier instead of requiring a model.
///
/// Manifest generation and `try_lower_inline_asm_call` must agree: a site the generator omits
/// but lowering still demands a model for would leave a gap the skeleton never mentions.
pub(crate) fn is_elided_barrier(
    opcode: InstructionOpcode,
    metadata: &InlineAsmMetadata,
    returns_void: bool,
    argument_count: u32,
) -> bool {
    // A callbr can select an indirect successor even with an empty template, so it always
    // needs a model for its control-flow effect.
    opcode != InstructionOpcode::CallBr
        && !metadata.can_unwind
        && inline_asm_is_empty(&metadata.asm)
        && (returns_void || argument_count == 1)
}

/// Empty and comment-only templates have no machine-level state effect.
pub(crate) fn inline_asm_is_empty(asm: &str) -> bool {
    asm.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#')
    })
}

pub(crate) fn has_module_inline_asm(module: &Module<'_>) -> bool {
    let mut len = 0;
    let ptr = unsafe { LLVMGetModuleInlineAsm(module.as_mut_ptr(), &mut len) };
    !ptr.is_null() && len != 0
}

fn inline_asm_string(
    asm: LLVMValueRef,
    getter: InlineAsmStringGetter,
    field: &str,
) -> Result<String> {
    let mut len = 0;
    let ptr = unsafe { getter(asm, &mut len) };
    if ptr.is_null() || len == 0 {
        return Ok(String::new());
    }
    std::str::from_utf8(unsafe { slice::from_raw_parts(ptr.cast::<u8>(), len) })
        .map(str::to_owned)
        .with_context(|| format!("inline-assembly {field} is not valid UTF-8"))
}

fn llvm_model_signature(instruction: InstructionValue<'_>, argument_count: u32) -> Result<String> {
    let return_type = instruction.get_type().print_to_string().to_string();
    let mut parameters = Vec::with_capacity(argument_count as usize);
    for index in 0..argument_count {
        let Some(operand) = instruction.get_operand(index) else {
            bail!("inline asm call is missing argument {index}");
        };
        let Some(value) = operand.value() else {
            bail!("inline asm argument {index} is not an LLVM value");
        };
        parameters.push(value.get_type().print_to_string().to_string());
    }
    Ok(format!("{return_type} @model({})", parameters.join(", ")))
}
