// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! External contract manifests backed by already-compiled LLVM helper functions.
//!
//! The manifest format is intentionally small. It only maps target functions to
//! helper symbols; the contract logic itself remains ordinary LLVM code that
//! can be compiled, linked, tested, and reused independently of this crate.

pub mod inject;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

/// The merged contract configuration from one or more TOML manifests.
///
/// The manifest only maps target functions to helper symbols. The contract
/// logic itself lives in ordinary compiled LLVM functions, which keeps
/// `llvm2cprover` out of the business of parsing a contract DSL.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractMap {
    functions: BTreeMap<String, FunctionContract>,
    helpers: BTreeSet<String>,
}

/// Call-site and model directives for one target function.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FunctionContract {
    /// Helper function called before the target call and assumed to hold.
    pub requires: Option<String>,
    /// Helper function called after the target call and asserted to hold.
    pub ensures: Option<String>,
    /// Helper function used instead of the original callee.
    pub model: Option<String>,
    /// Void helper functions that describe the writable locations of an extern.
    pub assigns: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: Option<u64>,
    #[serde(default)]
    functions: BTreeMap<String, FunctionContract>,
}

/// Load and merge external contract manifests.
pub fn load_contracts(paths: &[PathBuf]) -> Result<ContractMap> {
    let mut contracts = ContractMap::default();
    for path in paths {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read contract file {}", path.display()))?;
        contracts.merge_manifest(path, &text)?;
    }
    Ok(contracts)
}

/// Parse and merge one in-memory contract manifest.
pub fn parse_contracts(text: &str) -> Result<ContractMap> {
    let mut contracts = ContractMap::default();
    contracts.merge_manifest(Path::new("<contracts>"), text)?;
    Ok(contracts)
}

impl ContractMap {
    /// Return whether any contract directives are configured.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    /// Look up the contract directives for one target function.
    pub fn function(&self, name: &str) -> Option<&FunctionContract> {
        self.functions.get(name)
    }

    /// Return whether `name` is reserved as a helper or model symbol.
    pub fn is_helper(&self, name: &str) -> bool {
        self.helpers.contains(name)
    }

    /// Iterate over all configured target-function contracts.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &FunctionContract)> {
        self.functions
            .iter()
            .map(|(name, contract)| (name.as_str(), contract))
    }

    fn merge_manifest(&mut self, path: &Path, text: &str) -> Result<()> {
        let manifest = toml::from_str::<Manifest>(text)
            .with_context(|| format!("failed to parse contract file {}", path.display()))?;
        if let Some(version) = manifest.version
            && version != 1
        {
            bail!(
                "{}: unsupported contract manifest version {}",
                path.display(),
                version
            );
        }
        for (name, contract) in manifest.functions {
            validate_function_contract(path, &name, &contract)?;
            if self
                .functions
                .insert(name.clone(), contract.clone())
                .is_some()
            {
                bail!(
                    "{}: duplicate contract definition for `{name}`",
                    path.display()
                );
            }
            self.helpers.extend(
                [contract.requires, contract.ensures, contract.model]
                    .into_iter()
                    .flatten(),
            );
            self.helpers.extend(contract.assigns.iter().cloned());
        }
        Ok(())
    }
}

fn validate_function_contract(path: &Path, name: &str, contract: &FunctionContract) -> Result<()> {
    if contract.requires.is_none()
        && contract.ensures.is_none()
        && contract.model.is_none()
        && contract.assigns.is_empty()
    {
        bail!(
            "{}: contract for `{name}` must define at least one of `requires`, `ensures`, `model`, or `assigns`",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_function_contract_manifest() {
        let contracts = super::parse_contracts(
            r#"
            [functions.abs]
            requires = "__contract_requires_abs"
            ensures = "__contract_ensures_abs"
            model = "__contract_model_abs"
            assigns = ["__contract_assigns_abs"]
            "#,
        )
        .expect("failed to parse contracts");

        let abs = contracts.function("abs").expect("missing abs contract");
        assert_eq!(abs.requires.as_deref(), Some("__contract_requires_abs"));
        assert_eq!(abs.ensures.as_deref(), Some("__contract_ensures_abs"));
        assert_eq!(abs.model.as_deref(), Some("__contract_model_abs"));
        assert_eq!(abs.assigns, vec!["__contract_assigns_abs"]);
        assert!(contracts.is_helper("__contract_requires_abs"));
        assert!(contracts.is_helper("__contract_assigns_abs"));
    }

    #[test]
    fn omits_manifest_version_by_default() {
        let contracts = super::parse_contracts(
            r#"
            [functions.f]
            model = "__contract_model_f"
            "#,
        )
        .expect("failed to parse contracts without version");

        assert_eq!(
            contracts
                .function("f")
                .expect("missing contract")
                .model
                .as_deref(),
            Some("__contract_model_f")
        );
    }

    #[test]
    fn rejects_unknown_contract_keys() {
        super::parse_contracts(
            r#"
            [functions.abs]
            helper = "x"
            "#,
        )
        .expect_err("contracts should fail");
    }
}
