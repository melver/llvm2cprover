// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Candidate sets for indirect calls (`--fp-groups`).
//!
//! CBMC's own function-pointer removal offers every address-taken function with a compatible
//! signature at each indirect call. This can result in formula blow-up whenever symex cannot fold
//! the pointer; e.g. when multiple equivalent implementations are linked for the purpose of
//! equivalence checking. With `--fp-groups`, the translator emits the dispatch itself: candidates
//! are the address-taken functions in the cone whose LLVM function type is exactly the call's,
//! restricted to the caller's group plus ungrouped functions. Any other target value fails an
//! assertion and ends the path (fail closed).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use inkwell::module::Module;
use inkwell::types::AsTypeRef;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupsFile {
    groups: BTreeMap<String, Vec<String>>,
}

/// Per-type candidate lists plus the grouping, built once per translation.
#[derive(Debug)]
pub(crate) struct FpDispatch {
    /// Function name -> group name.
    group_of: HashMap<String, String>,
    /// LLVM function type (uniqued `LLVMTypeRef`) -> address-taken functions of that type.
    by_type: HashMap<usize, Vec<String>>,
}

impl FpDispatch {
    pub(crate) fn load(
        path: &Path,
        module: &Module<'_>,
        address_taken: &BTreeSet<String>,
    ) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading --fp-groups file {}", path.display()))?;
        let file: GroupsFile = toml::from_str(&text)
            .with_context(|| format!("parsing --fp-groups file {}", path.display()))?;
        let mut group_of = HashMap::new();
        for (group, functions) in file.groups {
            for function in functions {
                // A misspelled (or un-prefixed, under `--rename-prefix`) name would silently leave
                // the function ungrouped, which offers it to every caller: exactly what the groups
                // exclude.
                if function.contains('\0') || module.get_function(&function).is_none() {
                    bail!("--fp-groups: `{function}` is not a function in the module");
                }
                if let Some(previous) = group_of.insert(function.clone(), group.clone()) {
                    bail!("--fp-groups: `{function}` is in both `{previous}` and `{group}`");
                }
            }
        }
        let mut by_type: HashMap<usize, Vec<String>> = HashMap::new();
        for function in module.get_functions() {
            let name = function.get_name().to_string_lossy();
            if address_taken.contains(name.as_ref()) {
                by_type
                    .entry(function.get_type().as_type_ref() as usize)
                    .or_default()
                    .push(name.into_owned());
            }
        }
        for candidates in by_type.values_mut() {
            candidates.sort();
        }
        Ok(Self { group_of, by_type })
    }

    /// Candidates for an indirect call of LLVM function type `typ` inside `caller`.
    pub(crate) fn candidates(&self, caller: &str, typ: usize) -> Vec<&str> {
        let caller_group = self.group_of.get(caller);
        self.by_type
            .get(&typ)
            .into_iter()
            .flatten()
            .filter(
                |candidate| match (caller_group, self.group_of.get(*candidate)) {
                    (Some(caller_group), Some(group)) => caller_group == group,
                    _ => true,
                },
            )
            .map(String::as_str)
            .collect()
    }
}
