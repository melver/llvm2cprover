// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Deterministic translation assumption and stub inventory.
//!
//! Reports are emitted only after lowering and GOTO serialization succeed, so a
//! failed translation cannot leave behind a newly generated report.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};
use cprover_bindings::goto_program::{Location, SymbolTable};
use inkwell::llvm_sys::core::LLVMGetFirstUse;
use inkwell::module::Module;
use inkwell::values::{AsValueRef, FunctionValue};
use serde::Serialize;

use crate::contracts::{ContractMap, FunctionContract};
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
    relied_contracts: BTreeSet<String>,
    resolved_functions: BTreeSet<String>,
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
    pub(crate) fn relied_contract(&mut self, function: &str) {
        if self.enabled {
            self.relied_contracts.insert(function.to_owned());
        }
    }

    /// Record that a declaration was resolved to a builtin or intrinsic lowering.
    pub(crate) fn resolved_function(&mut self, function: &str) {
        if self.enabled {
            self.resolved_functions.insert(function.to_owned());
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

#[derive(Debug, Serialize)]
struct TranslationReport {
    schema_version: u32,
    machine_model: &'static str,
    target_triple: String,
    data_layout: String,
    undefined_functions: Vec<String>,
    approximations: Vec<Approximation>,
    contracts: Vec<ContractAssumption>,
}

#[derive(Debug, Serialize)]
struct ContractAssumption {
    function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ensures: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    assigns: Vec<String>,
    discharged: bool,
}

pub(crate) fn write_report(
    path: &Path,
    module: &Module<'_>,
    machine_model: MachineModel,
    contracts: &ContractMap,
    symbols: &SymbolTable,
    collected: ReportCollector,
) -> Result<()> {
    let ReportCollector {
        mut approximations,
        relied_contracts,
        resolved_functions,
        ..
    } = collected;
    approximations.sort_by(|a, b| {
        (&a.function, &a.construct, a.feature).cmp(&(&b.function, &b.construct, b.feature))
    });
    let report = TranslationReport {
        schema_version: 1,
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
        contracts: contracts
            .iter()
            .filter(|(name, _)| relied_contracts.contains(*name))
            .map(contract_assumption)
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&report).expect("translation report is serializable");
    fs::write(path, bytes)
        .with_context(|| format!("failed to write translation report {}", path.display()))
}

fn undefined_functions(
    module: &Module<'_>,
    contracts: &ContractMap,
    relied_contracts: &BTreeSet<String>,
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

    for (target, contract) in contracts
        .iter()
        .filter(|(target, _)| relied_contracts.contains(*target))
    {
        // Models redirect the call, while assigns synthesizes a body for the
        // external target. Neither leaves the original declaration as a stub.
        if contract.model.is_some() || !contract.assigns.is_empty() {
            undefined.remove(target);
        }
        for helper in [
            contract.requires.as_deref(),
            contract.ensures.as_deref(),
            contract.model.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(contract.assigns.iter().map(String::as_str))
        {
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

fn contract_assumption((name, contract): (&str, &FunctionContract)) -> ContractAssumption {
    ContractAssumption {
        function: name.to_owned(),
        requires: contract.requires.clone(),
        ensures: contract.ensures.clone(),
        model: contract.model.clone(),
        assigns: contract.assigns.clone(),
        // E3 contract discharge is not implemented. Make trust explicit instead
        // of implying that call-site instrumentation proves the helper contract.
        discharged: false,
    }
}
