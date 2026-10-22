// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! External contract and inline-assembly model manifests backed by compiled LLVM functions.
//!
//! The manifest format is intentionally small. It maps target functions to
//! contract helpers and exact inline-assembly metadata to model functions. The
//! executable logic remains ordinary LLVM code that can be compiled, linked,
//! tested, and reused independently of this crate.

pub mod inject;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

/// The merged contract and inline-assembly model configuration.
///
/// Manifests only map LLVM constructs to helper symbols. The executable logic
/// lives in ordinary compiled LLVM functions, which keeps `llvm2cprover` out of
/// the business of parsing a contract or target-semantics DSL.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractMap {
    functions: BTreeMap<String, FunctionContract>,
    inline_asm: Vec<InlineAsmModel>,
    helpers: BTreeSet<String>,
}

/// Exact-match rule that redirects one inline-assembly call to compiled LLVM code.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InlineAsmModel {
    /// LLVM's raw inline-assembly template.
    pub asm: String,
    /// LLVM's raw constraint string.
    pub constraints: String,
    /// Assembly dialect used to interpret the raw template.
    pub dialect: InlineAsmDialect,
    /// Whether LLVM marks the assembly as having side effects.
    pub side_effects: bool,
    /// Whether LLVM requires stack alignment before executing the assembly.
    pub align_stack: bool,
    /// Optional enclosing function name used to narrow the rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    /// Defined LLVM function called in place of the assembly.
    pub model: String,
}

/// One inline-assembly call site, as the exact-match key a model rule is looked up by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InlineAsmSite<'a> {
    /// LLVM's raw inline-assembly template.
    pub asm: &'a str,
    /// LLVM's raw constraint string.
    pub constraints: &'a str,
    /// Assembly dialect used to interpret the raw template.
    pub dialect: InlineAsmDialect,
    /// Whether LLVM marks the assembly as having side effects.
    pub side_effects: bool,
    /// Whether LLVM requires stack alignment before executing the assembly.
    pub align_stack: bool,
    /// Name of the function whose body contains the assembly.
    pub function: &'a str,
}

/// LLVM inline-assembly template dialect used as part of an exact model match.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InlineAsmDialect {
    Att,
    Intel,
}

impl InlineAsmModel {
    /// Whether this rule applies to one concrete call site.
    pub fn matches(&self, site: &InlineAsmSite<'_>) -> bool {
        self.same_assembly(
            site.asm,
            site.constraints,
            site.dialect,
            site.side_effects,
            site.align_stack,
        ) && self
            .function
            .as_deref()
            .is_none_or(|qualified| qualified == site.function)
    }

    /// Whether two rules could both apply to the same call site.
    ///
    /// Rules qualified by different enclosing functions never overlap; an unqualified rule
    /// overlaps every rule with the same assembly.
    fn overlaps(&self, other: &Self) -> bool {
        self.same_assembly(
            &other.asm,
            &other.constraints,
            other.dialect,
            other.side_effects,
            other.align_stack,
        ) && (self.function == other.function
            || self.function.is_none()
            || other.function.is_none())
    }

    /// Whether the assembly itself is identical, ignoring any `function` qualifier.
    pub(crate) fn same_assembly(
        &self,
        asm: &str,
        constraints: &str,
        dialect: InlineAsmDialect,
        side_effects: bool,
        align_stack: bool,
    ) -> bool {
        self.asm == asm
            && self.constraints == constraints
            && self.dialect == dialect
            && self.side_effects == side_effects
            && self.align_stack == align_stack
    }
}

/// Call-site and model directives for one target function.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FunctionContract {
    /// Helper function called before the target call and assumed to hold.
    pub requires: Option<String>,
    /// Helper function called after the target call and asserted to hold.
    pub ensures: Option<String>,
    /// Helper function called before the target to capture state for `ensures`.
    ///
    /// The snapshot helper receives the target parameters and returns one non-void value.
    /// `ensures` receives that value after the target parameters and before the target result.
    pub snapshot: Option<String>,
    /// Helper function used instead of the original callee.
    pub model: Option<String>,
    /// Compiled, observationally read-only helpers describing writable locations.
    ///
    /// Each helper returns a pointer to one concrete, nonlocal writable object. `None`
    /// distinguishes an omitted frame from an explicitly empty (pure) frame.
    pub assigns: Option<Vec<String>>,
}

impl FunctionContract {
    pub(crate) fn helpers(&self) -> impl Iterator<Item = &str> {
        [
            self.requires.as_deref(),
            self.ensures.as_deref(),
            self.snapshot.as_deref(),
            self.model.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(self.assigns.iter().flatten().map(String::as_str))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: Option<u64>,
    #[serde(default)]
    functions: BTreeMap<String, FunctionContract>,
    #[serde(default)]
    inline_asm: Vec<InlineAsmModel>,
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
    /// Return whether any contract or inline-assembly directives are configured.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty() && self.inline_asm.is_empty()
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

    /// Find the model for an exact inline-assembly call-site match.
    ///
    /// [`ContractMap::merge_manifest`] rejects overlapping rules, so at most one can match.
    pub fn inline_asm_model(&self, site: &InlineAsmSite<'_>) -> Option<&InlineAsmModel> {
        let mut matches = self.inline_asm.iter().filter(|entry| entry.matches(site));
        let first = matches.next();
        debug_assert!(
            first.is_none() || matches.next().is_none(),
            "overlapping inline asm rules must be rejected when the manifest is merged"
        );
        first
    }

    /// Iterate over configured inline-assembly model rules.
    pub fn inline_asm_models(&self) -> impl Iterator<Item = &InlineAsmModel> {
        self.inline_asm.iter()
    }

    /// Return whether any inline-assembly model rules are configured.
    pub fn has_inline_asm_models(&self) -> bool {
        !self.inline_asm.is_empty()
    }

    /// Restrict contracts and inline-assembly rules to the reachable cone of influence.
    ///
    /// Explicitly selected replacement and enforcement targets are retained so missing or
    /// unreachable selections still fail closed during contract validation.
    pub(crate) fn scoped_to(
        &self,
        reachable_functions: &BTreeSet<String>,
        replacements: &BTreeSet<String>,
        enforcements: &BTreeSet<String>,
    ) -> Self {
        let functions = self
            .functions
            .iter()
            .filter(|(name, _)| {
                reachable_functions.contains(name.as_str())
                    || replacements.contains(name.as_str())
                    || enforcements.contains(name.as_str())
            })
            .map(|(name, contract)| (name.clone(), contract.clone()))
            .collect::<BTreeMap<_, _>>();
        let inline_asm = self
            .inline_asm
            .iter()
            .filter(|mapping| reachable_functions.contains(&mapping.model))
            .cloned()
            .collect::<Vec<_>>();
        let mut helpers = BTreeSet::new();
        for contract in functions.values() {
            helpers.extend(contract.helpers().map(str::to_owned));
        }
        for model in &inline_asm {
            helpers.insert(model.model.clone());
        }
        Self {
            functions,
            inline_asm,
            helpers,
        }
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
            self.helpers.extend(contract.helpers().map(str::to_owned));
            if self.functions.insert(name.clone(), contract).is_some() {
                bail!(
                    "{}: duplicate contract definition for `{name}`",
                    path.display()
                );
            }
        }
        for model in manifest.inline_asm {
            // Empty ordinary calls lower as no-op barriers, but an empty `callbr` can still
            // select an indirect successor and therefore needs an exact model for its effects.
            // Keep the empty string available as a valid exact-match key for that case.
            if model.model.trim().is_empty() {
                bail!(
                    "{}: inline asm model must define a nonempty `model`",
                    path.display()
                );
            }
            if model.function.as_deref().is_some_and(str::is_empty) {
                bail!(
                    "{}: inline asm model `function` qualifier must be nonempty",
                    path.display()
                );
            }
            if self
                .inline_asm
                .iter()
                .any(|existing| existing.overlaps(&model))
            {
                bail!(
                    "{}: ambiguous inline asm model for template {:?} and constraints {:?}",
                    path.display(),
                    model.asm,
                    model.constraints
                );
            }
            self.helpers.insert(model.model.clone());
            self.inline_asm.push(model);
        }
        Ok(())
    }
}

fn validate_function_contract(path: &Path, name: &str, contract: &FunctionContract) -> Result<()> {
    if contract.requires.is_none()
        && contract.ensures.is_none()
        && contract.snapshot.is_none()
        && contract.model.is_none()
        && contract.assigns.is_none()
    {
        bail!(
            "{}: contract for `{name}` must define at least one of `requires`, `ensures`, `snapshot`, `model`, or `assigns`",
            path.display()
        );
    }
    if contract.snapshot.is_some() && contract.ensures.is_none() {
        bail!(
            "{}: contract for `{name}` defines `snapshot` without `ensures`",
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
            snapshot = "__contract_snapshot_abs"
            model = "__contract_model_abs"
            assigns = ["__contract_assigns_abs"]
            "#,
        )
        .expect("failed to parse contracts");

        let abs = contracts.function("abs").expect("missing abs contract");
        assert_eq!(abs.requires.as_deref(), Some("__contract_requires_abs"));
        assert_eq!(abs.ensures.as_deref(), Some("__contract_ensures_abs"));
        assert_eq!(abs.snapshot.as_deref(), Some("__contract_snapshot_abs"));
        assert_eq!(abs.model.as_deref(), Some("__contract_model_abs"));
        assert_eq!(abs.assigns, Some(vec!["__contract_assigns_abs".to_owned()]));
        assert!(contracts.is_helper("__contract_requires_abs"));
        assert!(contracts.is_helper("__contract_snapshot_abs"));
        assert!(contracts.is_helper("__contract_assigns_abs"));
    }

    #[test]
    fn rejects_snapshot_without_ensures() {
        let error = super::parse_contracts(
            r#"
            [functions.f]
            snapshot = "snapshot_f"
            "#,
        )
        .expect_err("an unused snapshot must fail");

        assert!(
            error
                .to_string()
                .contains("defines `snapshot` without `ensures`")
        );
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

    #[test]
    fn distinguishes_omitted_and_explicitly_empty_assigns() {
        let omitted = super::parse_contracts(
            r#"
            [functions.f]
            requires = "requires_f"
            "#,
        )
        .expect("failed to parse omitted assigns");
        let explicit = super::parse_contracts(
            r#"
            [functions.f]
            assigns = []
            "#,
        )
        .expect("failed to parse empty assigns");

        assert_eq!(omitted.function("f").unwrap().assigns, None);
        assert_eq!(explicit.function("f").unwrap().assigns, Some(Vec::new()));
    }

    #[test]
    fn matches_inline_asm_by_template_constraints_and_function() {
        let contracts = super::parse_contracts(
            r#"
            [[inline_asm]]
            asm = "opaque $0"
            constraints = "=r,r"
            dialect = "att"
            side_effects = true
            align_stack = false
            function = "target"
            model = "model_opaque"
            "#,
        )
        .expect("failed to parse inline asm model");

        assert!(
            contracts
                .inline_asm_model(&super::InlineAsmSite {
                    asm: "opaque $0",
                    constraints: "=r,r",
                    dialect: super::InlineAsmDialect::Att,
                    side_effects: true,
                    align_stack: false,
                    function: "other",
                })
                .is_none()
        );
        assert_eq!(
            contracts
                .inline_asm_model(&super::InlineAsmSite {
                    asm: "opaque $0",
                    constraints: "=r,r",
                    dialect: super::InlineAsmDialect::Att,
                    side_effects: true,
                    align_stack: false,
                    function: "target",
                })
                .unwrap()
                .model,
            "model_opaque"
        );
        assert!(contracts.is_helper("model_opaque"));
        assert!(
            contracts
                .inline_asm_model(&super::InlineAsmSite {
                    asm: "opaque $0",
                    constraints: "=r,r",
                    dialect: super::InlineAsmDialect::Intel,
                    side_effects: true,
                    align_stack: false,
                    function: "target",
                })
                .is_none()
        );
        assert!(
            contracts
                .inline_asm_model(&super::InlineAsmSite {
                    asm: "opaque $0",
                    constraints: "=r,r",
                    dialect: super::InlineAsmDialect::Att,
                    side_effects: false,
                    align_stack: false,
                    function: "target",
                })
                .is_none()
        );
        assert!(
            contracts
                .inline_asm_model(&super::InlineAsmSite {
                    asm: "opaque $0",
                    constraints: "=r,r",
                    dialect: super::InlineAsmDialect::Att,
                    side_effects: true,
                    align_stack: true,
                    function: "target",
                })
                .is_none()
        );
    }

    #[test]
    fn allows_empty_inline_asm_key_for_asm_goto() {
        let contracts = super::parse_contracts(
            r#"
            [[inline_asm]]
            asm = ""
            constraints = "!i"
            dialect = "att"
            side_effects = true
            align_stack = false
            model = "model_goto"
            "#,
        )
        .expect("empty asm-goto templates should be representable");

        assert!(
            contracts
                .inline_asm_model(&super::InlineAsmSite {
                    asm: "",
                    constraints: "!i",
                    dialect: super::InlineAsmDialect::Att,
                    side_effects: true,
                    align_stack: false,
                    function: "main",
                })
                .is_some()
        );
    }

    #[test]
    fn rejects_ambiguous_inline_asm_matches() {
        let error = super::parse_contracts(
            r#"
            [[inline_asm]]
            asm = "opaque"
            constraints = ""
            dialect = "att"
            side_effects = true
            align_stack = false
            model = "generic"

            [[inline_asm]]
            asm = "opaque"
            constraints = ""
            dialect = "att"
            side_effects = true
            align_stack = false
            function = "target"
            model = "specific"
            "#,
        )
        .expect_err("overlapping rules must fail");
        assert!(error.to_string().contains("ambiguous inline asm model"));
    }
}
