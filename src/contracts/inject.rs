// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Native contract attachment and explicit replacement synthesis.
//!
//! At ordinary call sites, helper-driven `requires`, `snapshot`, `ensures`, and `model` behavior
//! is handled during instruction lowering. A snapshot captures pre-state after `requires` and
//! before the target or model runs. Explicit replacements are synthesized here before instruction
//! lowering with the same ordering, placing snapshot capture before `assigns`; symbol-level
//! `assigns` metadata is attached after lowering. Observationally read-only assigns helpers return
//! CBMC lvalues before entering the preserved implementation; their purity is validated against
//! the LLVM input before lowering.
//!
//! For external functions, `assigns` is represented twice on purpose:
//!
//! - native CBMC contract metadata is attached to the function symbol
//! - a stub body is synthesized so plain `cbmc` still sees the intended writes
//!   without extra contract-mode flags

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use cprover_bindings::InternedString;
use cprover_bindings::goto_program::{
    Expr, FunctionContract as GotoFunctionContract, Lambda, Location, Parameter, Stmt, Symbol,
    SymbolTable, Type,
};

use super::{ContractMap, FunctionContract};
use crate::util;

/// Contract target name to the internal replacement symbol synthesized for it.
pub(crate) type ReplacementMap = BTreeMap<String, String>;

enum HelperReturn {
    Value,
    Exact(Type),
    Assigns,
}

/// Attach symbol-level contracts after lowering.
///
/// Ordinary-call `requires`, pre-call `snapshot`, `ensures`, and `model` behavior is handled
/// during instruction lowering, while explicit replacements are synthesized earlier. The
/// remaining native contract hook is `assigns`, which is represented both as CBMC contract
/// metadata and, for externs, as a synthesized stub body so plain `cbmc` sees the intended write
/// effects without extra contract-mode flags.
pub fn inject_contracts(symbols: &mut SymbolTable, contracts: &ContractMap) -> Result<()> {
    validate_contracts(symbols, contracts)?;
    inject_contracts_with_replacements(symbols, contracts, &BTreeSet::new(), &BTreeSet::new())
}

/// Attach `assigns` metadata, skipping targets bypassed by an explicit replacement.
///
/// Callers must have validated `contracts` against `symbols` already; the in-tree path does
/// that in [`prepare_replacements`], before lowering, so a bad manifest fails early.
pub(crate) fn inject_contracts_with_replacements(
    symbols: &mut SymbolTable,
    contracts: &ContractMap,
    replacements: &BTreeSet<String>,
    enforcements: &BTreeSet<String>,
) -> Result<()> {
    for (name, contract) in contracts.iter() {
        let Some(assigns) = contract.assigns.as_ref() else {
            continue;
        };
        let replaced = replacements.contains(name);
        let Some(target) = symbols.lookup(name) else {
            continue;
        };
        let target_is_variadic = target.typ.is_variadic_code();
        let needs_extern_stub = is_undefined_extern(target);
        if target_is_variadic {
            if replaced {
                // cprover_bindings cannot attach a native FunctionContract to VariadicCode.
                // The selected replacement still executes the assigns helpers directly, so its
                // Replacement semantics do not depend on metadata on the bypassed original symbol.
                continue;
            }
            bail!("assigns metadata for variadic function `{name}` is unsupported");
        }
        attach_assigns_contract(symbols, name, contract)?;
        if enforcements.contains(name) {
            attach_enforcement_contract(symbols, name, contract)?;
        }
        if !replaced && !assigns.is_empty() && needs_extern_stub {
            synthesize_assigns_stub(symbols, name, contract)?;
        }
    }
    Ok(())
}

/// Stable property symbol consumed by `goto-instrument --enforce-contract`.
pub fn enforcement_contract_symbol(name: &str) -> String {
    format!(
        "__llvm2cprover_contract_spec_{}",
        crate::util::sanitize_name(name)
    )
}

fn enforcement_contract_identifier(name: &str) -> String {
    format!("contract::{}", enforcement_contract_symbol(name))
}

fn enforcement_assigns_slot(name: &str, index: usize) -> String {
    format!(
        "__llvm2cprover_contract_assigns_{}_{}",
        crate::util::sanitize_name(name),
        index
    )
}

/// Stable harness entry point for checking one preserved implementation.
pub fn enforcement_harness_symbol(name: &str) -> String {
    format!(
        "__llvm2cprover_contract_enforcement_{}",
        crate::util::sanitize_name(name)
    )
}

fn attach_enforcement_contract(
    symbols: &mut SymbolTable,
    name: &str,
    contract: &FunctionContract,
) -> Result<()> {
    let target = symbols
        .lookup(name)
        .ok_or_else(|| anyhow::anyhow!("contract target `{name}` is missing"))?
        .clone();
    let contract_name = enforcement_contract_identifier(name);
    reject_symbol_collision(symbols, &contract_name, "contract enforcement symbol")?;
    let contract_type = Type::code(
        indexed_parameters(&target.typ, &contract_name),
        target
            .typ
            .return_type()
            .cloned()
            .expect("validated function type"),
    );
    ensure_function_parameters(symbols, &contract_name, &contract_type, &target.location);
    let symbol = Symbol::function(
        contract_name.clone(),
        contract_type,
        None,
        contract_name.clone(),
        target.location,
    )
    .with_is_property(true);
    let contract_type = symbol.typ.clone();
    symbols.insert(symbol);
    attach_enforcement_assigns_contract(symbols, &contract_name, &contract_type, name, contract)
}

fn attach_enforcement_assigns_contract(
    symbols: &mut SymbolTable,
    contract_name: &str,
    contract_type: &Type,
    target_name: &str,
    contract: &FunctionContract,
) -> Result<()> {
    let clauses = contract
        .assigns
        .iter()
        .flatten()
        .enumerate()
        .map(|(index, _)| {
            let assigned = symbols
                .lookup(enforcement_assigns_slot(target_name, index))
                .expect("prepared enforcement assigns slot")
                .to_expr()
                .dereference();
            Lambda::as_contract_for(contract_type, None::<InternedString>, assigned)
        })
        .collect();
    symbols.attach_contract(contract_name, GotoFunctionContract::new(clauses));
    Ok(())
}

/// Whether an assigns helper's return type names a concrete writable object.
fn is_assigns_frame_pointer(typ: &Type) -> bool {
    typ.is_pointer()
        && !typ
            .base_type()
            .is_some_and(|pointee| pointee == &Type::empty())
}

/// Require the explicit `assigns` clause both replacement and enforcement depend on.
fn require_explicit_assigns(contract: &FunctionContract, name: &str, mode: &str) -> Result<()> {
    if contract.assigns.is_none() {
        bail!(
            "contract {mode} for `{name}` requires an explicit `assigns` clause; use `assigns = []` for no writes"
        );
    }
    Ok(())
}

/// Build `argN` parameters mirroring a target's signature, scoped to one synthesized symbol.
fn indexed_parameters(target_type: &Type, prefix: &str) -> Vec<Parameter> {
    target_type
        .parameters()
        .expect("validated function type")
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let base = format!("arg{index}");
            let identifier = format!("{prefix}::{base}");
            Parameter::new(Some(base), Some(identifier), parameter.typ().clone())
        })
        .collect()
}

/// Reject a synthesized symbol name that the input module already uses.
fn reject_symbol_collision(symbols: &SymbolTable, name: &str, kind: &str) -> Result<()> {
    if symbols.lookup(name).is_some() {
        bail!("{kind} `{name}` collides with an input symbol");
    }
    Ok(())
}

/// Whether a symbol is an external declaration with no body to translate.
fn is_undefined_extern(symbol: &Symbol) -> bool {
    symbol.is_extern && symbol.value.is_none()
}

/// Validate replacement selections and synthesize internal contract-backed functions.
///
/// These symbols are created before LLVM bodies are lowered so direct calls can be redirected to
/// them. The original target symbol and body are left untouched.
pub(crate) fn prepare_replacements(
    symbols: &mut SymbolTable,
    contracts: &ContractMap,
    replacements: &BTreeSet<String>,
) -> Result<ReplacementMap> {
    let mut synthesized = ReplacementMap::new();
    for name in replacements {
        let contract = contracts.function(name).ok_or_else(|| {
            anyhow::anyhow!("--replace-with-contract target `{name}` has no contract")
        })?;
        require_explicit_assigns(contract, name, "replacement")?;
        if contract.model.is_some() {
            bail!("contract replacement for `{name}` cannot also define `model`");
        }
        synthesized.insert(
            name.clone(),
            synthesize_replacement(symbols, name, contract)?,
        );
    }
    Ok(synthesized)
}

/// Validate enforcement selections and synthesize harnesses around preserved real bodies.
pub(crate) fn prepare_enforcements(
    symbols: &mut SymbolTable,
    contracts: &ContractMap,
    enforcements: &BTreeSet<String>,
    replacements: &BTreeSet<String>,
) -> Result<()> {
    for name in enforcements {
        if replacements.contains(name) {
            bail!("contract enforcement for `{name}` cannot be combined with replacement");
        }
        let contract = contracts
            .function(name)
            .ok_or_else(|| anyhow::anyhow!("--enforce-contract target `{name}` has no contract"))?;
        require_explicit_assigns(contract, name, "enforcement")?;
        if contract.model.is_some() {
            bail!("contract enforcement for `{name}` cannot discharge a trusted `model`");
        }
        for (kind, helper) in [
            ("requires", contract.requires.as_deref()),
            ("ensures", contract.ensures.as_deref()),
            ("snapshot", contract.snapshot.as_deref()),
        ] {
            if let Some(helper) = helper {
                let symbol = symbols.lookup(helper).expect("validated contract helper");
                if !has_helper_body(symbol) {
                    bail!(
                        "contract enforcement for `{name}` requires defined {kind} helper `{helper}`"
                    );
                }
            }
        }
        for (index, helper) in contract.assigns.iter().flatten().enumerate() {
            let helper_symbol = symbols
                .lookup(helper.as_str())
                .expect("validated assigns helper");
            let typ = helper_symbol
                .typ
                .return_type()
                .expect("validated assigns helper")
                .clone();
            debug_assert!(is_assigns_frame_pointer(&typ));
            let slot = enforcement_assigns_slot(name, index);
            reject_symbol_collision(symbols, &slot, "contract enforcement assigns slot")?;
            let mut symbol = Symbol::static_variable(&slot, &slot, typ.clone(), Location::none());
            symbol.update(cprover_bindings::goto_program::SymbolValues::Expr(
                typ.null(),
            ));
            symbols.insert(symbol);
        }
        let target = symbols
            .lookup(name)
            .ok_or_else(|| anyhow::anyhow!("contract enforcement target `{name}` is missing"))?;
        if target.is_extern {
            bail!("contract enforcement target `{name}` must be defined");
        }
        if target.typ.is_variadic_code() {
            bail!("contract enforcement for variadic function `{name}` is unsupported");
        }
        if !contract
            .assigns
            .as_ref()
            .expect("explicit assigns")
            .is_empty()
            && target
                .typ
                .return_type()
                .is_some_and(|typ| typ == &Type::empty())
        {
            bail!(
                "contract enforcement for void function `{name}` with a nonempty assigns frame is unsupported"
            );
        }
        synthesize_enforcement_harness(symbols, name, contract)?;
    }
    Ok(())
}

/// Stable private symbol used for one explicit contract replacement.
pub(crate) fn replacement_symbol(name: &str) -> String {
    format!(
        "__llvm2cprover_contract_replacement_{}",
        crate::util::sanitize_name(name)
    )
}

/// Check every manifest helper against the lowered symbol table.
///
/// Run once, before [`prepare_replacements`] and [`prepare_enforcements`], so a bad manifest
/// fails before any synthesis happens.
pub(crate) fn validate_contracts(symbols: &SymbolTable, contracts: &ContractMap) -> Result<()> {
    for (name, contract) in contracts.iter() {
        let target = symbols
            .lookup(name)
            .ok_or_else(|| anyhow::anyhow!("contract target `{name}` is missing"))?;
        validate_target_is_function(name, &target.typ)?;
        let param_count = target.typ.parameters().map_or(0, Vec::len);
        let return_type = target
            .typ
            .return_type()
            .cloned()
            .unwrap_or_else(Type::empty);
        if let Some(helper) = contract.requires.as_deref() {
            validate_helper(
                symbols,
                name,
                helper,
                param_count,
                "requires",
                HelperReturn::Value,
            )?;
        }
        let snapshot_type = if let Some(helper) = contract.snapshot.as_deref() {
            let snapshot_symbol = validate_helper(
                symbols,
                name,
                helper,
                param_count,
                "snapshot",
                HelperReturn::Value,
            )?;
            // A snapshot must observe the caller's pre-state, which an opaque declaration
            // cannot do. This is stricter than `is_undefined_extern`, which only asks whether
            // a symbol still needs a synthesized stub.
            if !has_helper_body(snapshot_symbol) {
                bail!("snapshot helper `{helper}` for `{name}` must be defined");
            }
            let target_parameters = target
                .typ
                .parameters()
                .expect("validated function type")
                .iter()
                .map(|parameter| parameter.typ().clone())
                .collect::<Vec<_>>();
            validate_helper_parameters(
                symbols,
                name,
                helper,
                snapshot_symbol,
                &target_parameters,
                "snapshot",
            )?;
            Some(
                snapshot_symbol
                    .typ
                    .return_type()
                    .cloned()
                    .expect("validated snapshot return type"),
            )
        } else {
            None
        };
        if let Some(helper) = contract.ensures.as_deref() {
            let ensures_arity = param_count
                + usize::from(snapshot_type.is_some())
                + usize::from(return_type != Type::empty());
            let ensures_symbol = validate_helper(
                symbols,
                name,
                helper,
                ensures_arity,
                "ensures",
                HelperReturn::Value,
            )?;
            if let Some(snapshot_type) = snapshot_type.as_ref() {
                let snapshot_parameter = ensures_symbol
                    .typ
                    .parameters()
                    .expect("validated ensures function type")
                    .get(param_count)
                    .expect("validated ensures arity")
                    .typ();
                if !types_are_call_compatible(
                    snapshot_type,
                    snapshot_parameter,
                    symbols.machine_model(),
                ) {
                    bail!(
                        "ensures helper `{helper}` for `{name}` has snapshot parameter type {:?}, expected {:?}",
                        snapshot_parameter,
                        snapshot_type
                    );
                }
            }
        }
        if let Some(helper) = contract.model.as_deref() {
            validate_helper(
                symbols,
                name,
                helper,
                param_count,
                "model",
                HelperReturn::Exact(return_type.clone()),
            )?;
        }
        for helper in contract.assigns.iter().flatten() {
            validate_helper(
                symbols,
                name,
                helper,
                param_count,
                "assigns",
                HelperReturn::Assigns,
            )?;
        }
    }
    Ok(())
}

/// Build the `ensures` argument list: target parameters, the snapshot, then the target result.
///
/// The instrumentation and synthesized-replacement paths share this ordering.
pub(crate) fn ensures_arguments(
    mut parameters: Vec<Expr>,
    snapshot: Option<Expr>,
    result: Option<Expr>,
) -> Vec<Expr> {
    parameters.extend(snapshot);
    parameters.extend(result);
    parameters
}

/// Whether a helper symbol carries a body that can be reasoned about.
///
/// Distinct from [`is_undefined_extern`], which asks whether a target still needs a stub.
fn has_helper_body(symbol: &Symbol) -> bool {
    !symbol.is_extern && !symbol.location.is_builtin()
}

/// Check a helper's parameter types against what call sites will pass.
///
/// Only `snapshot` uses this today; `requires`, `ensures`, and `model` rely on the
/// target-equivalent casts applied when their calls are emitted.
fn validate_helper_parameters(
    symbols: &SymbolTable,
    target_name: &str,
    helper_name: &str,
    helper_symbol: &Symbol,
    expected: &[Type],
    kind: &str,
) -> Result<()> {
    let parameters = helper_symbol
        .typ
        .parameters()
        .expect("helper existence and arity were validated");
    for (index, (parameter, expected)) in parameters.iter().zip(expected).enumerate() {
        if !types_are_call_compatible(expected, parameter.typ(), symbols.machine_model()) {
            bail!(
                "{kind} helper `{helper_name}` for `{target_name}` argument {index} has type {:?}, expected {:?}",
                parameter.typ(),
                expected
            );
        }
    }
    Ok(())
}

/// Match the target-equivalent casts used when helper calls are emitted, without treating two
/// unrelated aggregate types as equivalent merely because neither has a scalar machine width.
fn types_are_call_compatible(
    source: &Type,
    target: &Type,
    machine_model: &cprover_bindings::MachineModel,
) -> bool {
    source == target
        || (source.is_equal_on_machine(target, machine_model)
            && Expr::nondet(source.clone()).can_cast_to(target))
}

fn attach_assigns_contract(
    symbols: &mut SymbolTable,
    name: &str,
    contract: &FunctionContract,
) -> Result<()> {
    let target = symbols
        .lookup(name)
        .ok_or_else(|| anyhow::anyhow!("contract target `{name}` is missing"))?
        .clone();
    let assigns = contract.assigns.as_deref().unwrap_or_default();
    let mut clauses = Vec::with_capacity(assigns.len());
    let args = parameter_exprs(&target.typ, target.location);
    for helper in assigns {
        let helper_symbol = symbols
            .lookup(helper.as_str())
            .ok_or_else(|| anyhow::anyhow!("assigns helper `{helper}` is missing"))?;
        let helper_expr = helper_symbol.to_expr().with_location(target.location);
        let call = helper_expr.call(
            Expr::cast_arguments_to_target_equivalent_function_parameter_types(
                &helper_symbol.to_expr(),
                args.clone(),
                symbols.machine_model(),
            ),
        );
        let assigned = call.dereference();
        clauses.push(Lambda::as_contract_for(
            &target.typ,
            None::<InternedString>,
            assigned,
        ));
    }
    symbols.attach_contract(name, GotoFunctionContract::new(clauses));
    Ok(())
}

fn validate_target_is_function(name: &str, target_type: &Type) -> Result<()> {
    if target_type.parameters().is_none() || target_type.return_type().is_none() {
        bail!("contract target `{name}` is not a function");
    }
    Ok(())
}

/// Validate one helper's arity and return type, yielding the resolved symbol for further checks.
fn validate_helper<'a>(
    symbols: &'a SymbolTable,
    target_name: &str,
    helper_name: &str,
    expected_arity: usize,
    kind: &str,
    expected_return: HelperReturn,
) -> Result<&'a Symbol> {
    let helper_symbol = symbols
        .lookup(helper_name)
        .ok_or_else(|| anyhow::anyhow!("{kind} helper `{helper_name}` is missing"))?;
    let helper_type = &helper_symbol.typ;
    let actual_arity = helper_type.parameters().map_or(0, Vec::len);
    if actual_arity != expected_arity {
        bail!(
            "{kind} helper `{helper_name}` for `{target_name}` expects {actual_arity} args, got {expected_arity}"
        );
    }
    let actual_return = helper_type
        .return_type()
        .cloned()
        .unwrap_or_else(Type::empty);
    match expected_return {
        HelperReturn::Value => {
            if actual_return == Type::empty() {
                bail!("{kind} helper `{helper_name}` for `{target_name}` must return a value");
            }
        }
        HelperReturn::Exact(expected_return) => {
            if actual_return != expected_return {
                bail!(
                    "{kind} helper `{helper_name}` for `{target_name}` returns {:?}, expected {:?}",
                    actual_return,
                    expected_return
                );
            }
        }
        HelperReturn::Assigns => {
            if !has_helper_body(helper_symbol) {
                bail!(
                    "assigns helper `{helper_name}` for `{target_name}` must be a compiled definition"
                );
            }
            if !actual_return.is_pointer() {
                bail!("assigns helper `{helper_name}` for `{target_name}` must return a pointer");
            }
            if !is_assigns_frame_pointer(&actual_return) {
                bail!(
                    "assigns helper `{helper_name}` for `{target_name}` must return a pointer with a concrete pointee type"
                );
            }
        }
    }
    Ok(helper_symbol)
}

fn synthesize_assigns_stub(
    symbols: &mut SymbolTable,
    name: &str,
    contract: &FunctionContract,
) -> Result<()> {
    let target = symbols
        .lookup(name)
        .ok_or_else(|| anyhow::anyhow!("contract target `{name}` is missing"))?
        .clone();
    if !is_undefined_extern(&target) {
        bail!("assigns contract for `{name}` currently requires an external declaration");
    }

    let loc = target.location;
    ensure_function_parameters(symbols, name, &target.typ, &loc);
    let params = parameter_exprs(&target.typ, loc);
    let mut body = assigns_helper_calls(symbols, name, contract, &params, loc)?;
    let return_type = target
        .typ
        .return_type()
        .cloned()
        .unwrap_or_else(Type::empty);
    if return_type != Type::empty() {
        body.push(Stmt::ret(Some(util::nondet_expr(return_type)), loc));
    } else {
        body.push(Stmt::ret(None, loc));
    }

    symbols.update_fn_declaration_with_definition(name, Stmt::block(body, loc));
    if let Some(symbol) = symbols.lookup_mut(name) {
        symbol.is_extern = false;
    }
    Ok(())
}

fn synthesize_replacement(
    symbols: &mut SymbolTable,
    name: &str,
    contract: &FunctionContract,
) -> Result<String> {
    let target = symbols
        .lookup(name)
        .ok_or_else(|| anyhow::anyhow!("contract target `{name}` is missing"))?
        .clone();
    let replacement = replacement_symbol(name);
    reject_symbol_collision(symbols, &replacement, "contract replacement symbol")?;
    let loc = target.location;
    let params = indexed_parameters(&target.typ, &replacement);
    let return_type = target
        .typ
        .return_type()
        .cloned()
        .expect("validated function type");
    let replacement_type = if target.typ.is_variadic_code() {
        Type::variadic_code(params, return_type.clone())
    } else {
        Type::code(params, return_type.clone())
    };

    ensure_function_parameters(symbols, &replacement, &replacement_type, &loc);
    let args = parameter_exprs(&replacement_type, loc);
    let mut body = Vec::new();
    if let Some(helper) = contract.requires.as_deref() {
        let condition = emit_value_helper(symbols, &replacement, helper, &args, "requires", loc)?;
        body.extend(condition.0);
        body.push(Stmt::assert(
            condition.1.cast_to(Type::bool()),
            "contract",
            &format!("requires for {name}"),
            loc,
        ));
    }
    let snapshot = if let Some(helper) = contract.snapshot.as_deref() {
        let snapshot = emit_value_helper(symbols, &replacement, helper, &args, "snapshot", loc)?;
        body.extend(snapshot.0);
        Some(snapshot.1)
    } else {
        None
    };
    body.extend(assigns_helper_calls(
        symbols,
        &replacement,
        contract,
        &args,
        loc,
    )?);

    let result = (return_type != Type::empty())
        .then(|| insert_local(symbols, &replacement, "result", return_type.clone(), loc));
    if let Some(result) = &result {
        body.push(Stmt::assign(
            result.clone(),
            util::nondet_expr(return_type),
            loc,
        ));
    }
    if let Some(helper) = contract.ensures.as_deref() {
        let helper_args = ensures_arguments(args, snapshot, result.clone());
        let condition =
            emit_value_helper(symbols, &replacement, helper, &helper_args, "ensures", loc)?;
        body.extend(condition.0);
        body.push(Stmt::assume(condition.1.cast_to(Type::bool()), loc));
    }
    body.push(Stmt::ret(result, loc));

    let symbol = Symbol::function(
        replacement.clone(),
        replacement_type,
        Some(Stmt::block(body, loc)),
        replacement.clone(),
        loc,
    )
    .with_is_file_local(true);
    symbols.insert(symbol);
    Ok(replacement)
}

fn synthesize_enforcement_harness(
    symbols: &mut SymbolTable,
    name: &str,
    contract: &FunctionContract,
) -> Result<String> {
    let target = symbols
        .lookup(name)
        .ok_or_else(|| anyhow::anyhow!("contract target `{name}` is missing"))?
        .clone();
    let harness = enforcement_harness_symbol(name);
    reject_symbol_collision(symbols, &harness, "contract enforcement harness")?;
    let loc = target.location;
    let harness_type = Type::code(indexed_parameters(&target.typ, &harness), Type::empty());
    ensure_function_parameters(symbols, &harness, &harness_type, &loc);
    let args = parameter_exprs(&harness_type, loc);
    let mut body = Vec::new();

    if let Some(helper) = contract.requires.as_deref() {
        let condition = emit_value_helper(symbols, &harness, helper, &args, "requires", loc)?;
        body.extend(condition.0);
        body.push(Stmt::assume(condition.1.cast_to(Type::bool()), loc));
    }
    let snapshot = if let Some(helper) = contract.snapshot.as_deref() {
        let snapshot = emit_value_helper(symbols, &harness, helper, &args, "snapshot", loc)?;
        body.extend(snapshot.0);
        Some(snapshot.1)
    } else {
        None
    };
    for (index, helper) in contract.assigns.iter().flatten().enumerate() {
        let helper_symbol = symbols
            .lookup(helper.as_str())
            .expect("validated assigns helper")
            .clone();
        let helper_expr = helper_symbol.to_expr().with_location(loc);
        let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &helper_expr,
            args.clone(),
            symbols.machine_model(),
        );
        let slot = symbols
            .lookup(enforcement_assigns_slot(name, index))
            .expect("prepared enforcement assigns slot")
            .to_expr()
            .with_location(loc);
        body.push(Stmt::function_call(Some(slot), helper_expr, arguments, loc));
    }

    let return_type = target
        .typ
        .return_type()
        .cloned()
        .expect("validated function type");
    let result = (return_type != Type::empty())
        .then(|| insert_local(symbols, &harness, "result", return_type, loc));
    let target_expr = target.to_expr().with_location(loc);
    body.push(Stmt::function_call(
        result.clone(),
        target_expr.clone(),
        Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &target_expr,
            args.clone(),
            symbols.machine_model(),
        ),
        loc,
    ));

    if let Some(helper) = contract.ensures.as_deref() {
        let helper_args = ensures_arguments(args, snapshot, result);
        let condition = emit_value_helper(symbols, &harness, helper, &helper_args, "ensures", loc)?;
        body.extend(condition.0);
        body.push(Stmt::assert(
            condition.1.cast_to(Type::bool()),
            "contract",
            &format!("ensures for {name}"),
            loc,
        ));
    }
    body.push(Stmt::ret(None, loc));

    symbols.insert(
        Symbol::function(
            harness.clone(),
            harness_type,
            Some(Stmt::block(body, loc)),
            harness.clone(),
            loc,
        )
        .with_is_file_local(true),
    );
    Ok(harness)
}

/// Apply the frame described by each assigns helper.
///
/// Call each assigns helper to compute one writable lvalue, then assign it a nondeterministic value.
fn assigns_helper_calls(
    symbols: &mut SymbolTable,
    function: &str,
    contract: &FunctionContract,
    args: &[Expr],
    loc: Location,
) -> Result<Vec<Stmt>> {
    let mut statements = Vec::new();
    for (index, helper) in contract.assigns.iter().flatten().enumerate() {
        let helper_symbol = symbols
            .lookup(helper.as_str())
            .ok_or_else(|| anyhow::anyhow!("assigns helper `{helper}` is missing"))?
            .clone();
        let helper_expr = helper_symbol.to_expr().with_location(loc);
        let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &helper_expr,
            args.to_vec(),
            symbols.machine_model(),
        );
        let return_type = helper_symbol
            .typ
            .return_type()
            .cloned()
            .expect("validated helper function");
        // The helper names an lvalue; havoc it so the frame is modeled.
        let result = insert_local(
            symbols,
            function,
            &format!("assigns{index}"),
            return_type,
            loc,
        );
        let lvalue = result.clone().dereference();
        statements.push(Stmt::function_call(
            Some(result),
            helper_expr,
            arguments,
            loc,
        ));
        statements.push(Stmt::assign(
            lvalue.clone(),
            util::nondet_expr(lvalue.typ().clone()),
            loc,
        ));
    }
    Ok(statements)
}

fn emit_value_helper(
    symbols: &mut SymbolTable,
    function: &str,
    helper: &str,
    args: &[Expr],
    kind: &str,
    loc: Location,
) -> Result<(Vec<Stmt>, Expr)> {
    let helper_symbol = symbols
        .lookup(helper)
        .ok_or_else(|| anyhow::anyhow!("{kind} helper `{helper}` is missing"))?
        .clone();
    let return_type = helper_symbol
        .typ
        .return_type()
        .cloned()
        .unwrap_or_else(Type::empty);
    let result = insert_local(symbols, function, kind, return_type, loc);
    let helper_expr = helper_symbol.to_expr().with_location(loc);
    let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
        &helper_expr,
        args.to_vec(),
        symbols.machine_model(),
    );
    Ok((
        vec![Stmt::function_call(
            Some(result.clone()),
            helper_expr,
            arguments,
            loc,
        )],
        result,
    ))
}

fn insert_local(
    symbols: &mut SymbolTable,
    function: &str,
    base: &str,
    typ: Type,
    loc: Location,
) -> Expr {
    let identifier = format!("{function}::{base}");
    let symbol = Symbol::variable(identifier.clone(), base, typ.clone(), loc)
        .with_is_thread_local(true)
        .with_is_state_var(false)
        .with_is_static_lifetime(false);
    symbols.insert(symbol);
    Expr::symbol_expression(identifier, typ).with_location(loc)
}

fn ensure_function_parameters(
    symbols: &mut SymbolTable,
    function: &str,
    typ: &Type,
    location: &Location,
) {
    typ.parameters()
        .into_iter()
        .flatten()
        .for_each(|parameter| {
            let identifier = parameter
                .identifier()
                .expect("function parameters are expected to have identifiers");
            let base_name = parameter.base_name().unwrap_or(identifier);
            if symbols.lookup(identifier).is_none() {
                let symbol = Symbol::variable(
                    identifier.to_string(),
                    base_name.to_string(),
                    parameter.typ().clone(),
                    *location,
                )
                .with_is_parameter(true)
                .with_is_thread_local(false)
                .with_is_state_var(false)
                .with_is_static_lifetime(false);
                symbols.insert(symbol);
                symbols.insert_parameter(function.to_owned(), identifier.to_string());
            }
        });
}

fn parameter_exprs(typ: &Type, location: Location) -> Vec<Expr> {
    typ.parameters()
        .into_iter()
        .flatten()
        .map(|parameter| {
            let identifier = parameter
                .identifier()
                .expect("function parameters are expected to have identifiers");
            Expr::symbol_expression(identifier, parameter.typ().clone()).with_location(location)
        })
        .collect()
}
