// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Deterministic translation assumption and stub inventory.
//!
//! Reports are emitted only after lowering and GOTO serialization succeed, so a
//! failed translation cannot leave behind a newly generated report.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};
use cprover_bindings::goto_program::{Location, SymbolTable};
use inkwell::llvm_sys::core::LLVMGetFirstUse;
use inkwell::module::Module;
use inkwell::values::{AsValueRef, FunctionValue};
use serde::Serialize;

use crate::contracts::{ContractMap, FunctionContract, InlineAsmDialect, InlineAsmSite};
use crate::{MachineModel, UnsoundFeature};

use super::JsonLocation;

/// Report bookkeeping accumulated while lowering function bodies.
///
/// Collection is disabled unless `--report` was requested, so the common case
/// pays neither the allocations nor the set inserts. New report inputs belong
/// here rather than in another parameter threaded through the lowering stack.
#[derive(Debug, Default)]
pub(crate) struct ReportCollector {
    enabled: bool,
    approximations: Vec<Approximation>,
    relied_contracts: BTreeMap<String, bool>,
    resolved_functions: BTreeSet<String>,
    inline_asm_models: BTreeSet<InlineAsmAssumption>,
}

impl ReportCollector {
    /// Create a collector that records only when a report will be written.
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// Record one approximation taken under an enabled [`UnsoundFeature`].
    pub(crate) fn approximation(
        &mut self,
        feature: UnsoundFeature,
        construct: &str,
        function: &str,
        location: &Location,
    ) {
        if self.enabled {
            self.approximations.push(Approximation {
                feature: feature.as_str(),
                construct: construct.to_owned(),
                function: function.to_owned(),
                location: (!location.is_none()).then(|| JsonLocation::from(location)),
            });
        }
    }

    /// Record that a direct call site was lowered against a manifest contract.
    pub(crate) fn relied_contract(&mut self, function: &str, replacement: bool) {
        if self.enabled {
            *self
                .relied_contracts
                .entry(function.to_owned())
                .or_default() |= replacement;
        }
    }

    /// Record that a declaration was resolved to a builtin or intrinsic lowering.
    pub(crate) fn resolved_function(&mut self, function: &str) {
        if self.enabled {
            self.resolved_functions.insert(function.to_owned());
        }
    }

    /// Record one compiled model used in place of target inline assembly.
    pub(crate) fn inline_asm_model(&mut self, site: &InlineAsmSite<'_>, model: &str) {
        if self.enabled {
            self.inline_asm_models.insert(InlineAsmAssumption {
                asm: site.asm.to_owned(),
                constraints: site.constraints.to_owned(),
                dialect: site.dialect,
                side_effects: site.side_effects,
                align_stack: site.align_stack,
                function: site.function.to_owned(),
                model: model.to_owned(),
                discharged: false,
            });
        }
    }
}

#[derive(Debug, Serialize)]
struct Approximation {
    feature: &'static str,
    construct: String,
    function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<JsonLocation>,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct InlineAsmAssumption {
    asm: String,
    constraints: String,
    dialect: InlineAsmDialect,
    side_effects: bool,
    align_stack: bool,
    function: String,
    model: String,
    discharged: bool,
}

#[derive(Debug, Serialize)]
struct TranslationReport {
    schema_version: u32,
    machine_model: &'static str,
    target_triple: String,
    data_layout: String,
    undefined_functions: Vec<String>,
    approximations: Vec<Approximation>,
    inline_asm_models: Vec<InlineAsmAssumption>,
    contracts: Vec<ContractAssumption>,
}

#[derive(Debug, Serialize)]
struct ContractAssumption {
    function: String,
    reliance: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assigns: Option<Vec<String>>,
    discharged: bool,
}

pub(crate) fn write_report(
    path: &Path,
    module: &Module<'_>,
    machine_model: MachineModel,
    contracts: &ContractMap,
    enforced_contracts: &BTreeSet<String>,
    symbols: &SymbolTable,
    collected: ReportCollector,
) -> Result<()> {
    let ReportCollector {
        mut approximations,
        relied_contracts,
        resolved_functions,
        inline_asm_models,
        ..
    } = collected;
    approximations.sort_by(|a, b| {
        (&a.function, &a.construct, a.feature).cmp(&(&b.function, &b.construct, b.feature))
    });
    let report = TranslationReport {
        schema_version: 4,
        machine_model: machine_model.as_str(),
        target_triple: module.get_triple().as_str().to_string_lossy().into_owned(),
        data_layout: module
            .get_data_layout()
            .as_str()
            .to_string_lossy()
            .into_owned(),
        undefined_functions: undefined_functions(
            module,
            contracts,
            &relied_contracts,
            &resolved_functions,
            symbols,
        ),
        approximations,
        inline_asm_models: inline_asm_models.into_iter().collect(),
        contracts: contracts
            .iter()
            .filter_map(|(name, contract)| {
                let replacement = relied_contracts.get(name).copied();
                if replacement.is_none() && !enforced_contracts.contains(name) {
                    return None;
                }
                Some(contract_assumption(
                    (name, contract),
                    replacement,
                    enforced_contracts.contains(name),
                ))
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&report).expect("translation report is serializable");
    fs::write(path, bytes)
        .with_context(|| format!("failed to write translation report {}", path.display()))
}

fn undefined_functions(
    module: &Module<'_>,
    contracts: &ContractMap,
    relied_contracts: &BTreeMap<String, bool>,
    resolved_functions: &BTreeSet<String>,
    symbols: &SymbolTable,
) -> Vec<String> {
    let mut undefined = module
        .get_functions()
        .filter(|function| function.count_basic_blocks() == 0 && is_referenced(*function))
        .map(|function| function.get_name().to_string_lossy().into_owned())
        .filter(|name| {
            !name.starts_with("llvm.")
                && !resolved_functions.contains(name)
                && is_unresolved_function(symbols, name)
        })
        .collect::<BTreeSet<_>>();

    for (target, contract, replaced) in contracts.iter().filter_map(|(target, contract)| {
        relied_contracts
            .get(target)
            .map(|replaced| (target, contract, *replaced))
    }) {
        // Models redirect the call, while assigns synthesizes a body for the
        // external target. Explicit replacements redirect even an empty-frame
        // contract. None leaves the original declaration as a relied-upon stub.
        if contract.model.is_some()
            || contract.assigns.as_ref().is_some_and(|a| !a.is_empty())
            || replaced
        {
            undefined.remove(target);
        }
        for helper in contract.helpers() {
            if is_unresolved_function(symbols, helper) {
                undefined.insert(helper.to_owned());
            }
        }
    }

    undefined.into_iter().collect()
}

fn is_unresolved_function(symbols: &SymbolTable, name: &str) -> bool {
    symbols
        .lookup(name)
        .is_some_and(|symbol| symbol.is_extern && symbol.value.is_none())
}

fn is_referenced(function: FunctionValue<'_>) -> bool {
    unsafe { !LLVMGetFirstUse(function.as_value_ref()).is_null() }
}

fn contract_assumption(
    (name, contract): (&str, &FunctionContract),
    replacement: Option<bool>,
    discharged: bool,
) -> ContractAssumption {
    ContractAssumption {
        function: name.to_owned(),
        reliance: match replacement {
            Some(true) => "replacement",
            Some(false) => "instrumentation",
            None => "enforcement",
        },
        requires: contract.requires.clone(),
        ensures: contract.ensures.clone(),
        snapshot: contract.snapshot.clone(),
        model: contract.model.clone(),
        assigns: contract.assigns.clone(),
        // This records that the artifact contains the selected enforcement obligation. A proof
        // result still comes from running CBMC after its dynamic-frame transformation.
        discharged,
    }
}
