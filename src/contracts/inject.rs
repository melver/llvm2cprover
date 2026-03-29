// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026, Marco Elver <me@marcoelver.com>

//! Native contract attachment after LLVM lowering.
//!
//! Helper-driven `requires`, `ensures`, and `model` behavior is handled during
//! instruction lowering because it changes call-site behavior. This module only
//! handles the remaining symbol-level contract hook: `assigns`.
//!
//! For external functions, `assigns` is represented twice on purpose:
//!
//! - native CBMC contract metadata is attached to the function symbol
//! - a stub body is synthesized so plain `cbmc` still sees the intended writes
//!   without extra contract-mode flags

use anyhow::{Result, bail};
use cprover_bindings::InternedString;
use cprover_bindings::goto_program::{
    Expr, FunctionContract as GotoFunctionContract, Lambda, Location, Stmt, Symbol, SymbolTable,
    SymbolValues, Type,
};

use super::{ContractMap, FunctionContract};

enum HelperReturn {
    Void,
    Value,
    Exact(Type),
}

/// Attach symbol-level contracts after lowering.
///
/// `requires`, `ensures`, and `model` are handled during instruction lowering.
/// The remaining native contract hook is `assigns`, which is represented both as
/// CBMC contract metadata and, for externs, as a synthesized stub body so plain
/// `cbmc` sees the intended write effects without extra contract-mode flags.
pub fn inject_contracts(symbols: &mut SymbolTable, contracts: &ContractMap) -> Result<()> {
    validate_contracts(symbols, contracts)?;
    for (name, contract) in contracts.iter() {
        if contract.assigns.is_empty() {
            continue;
        }
        attach_assigns_contract(symbols, name, contract)?;
        synthesize_assigns_stub(symbols, name, contract)?;
    }
    Ok(())
}

fn validate_contracts(symbols: &SymbolTable, contracts: &ContractMap) -> Result<()> {
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
        if let Some(helper) = contract.ensures.as_deref() {
            let ensures_arity = param_count + usize::from(return_type != Type::empty());
            validate_helper(
                symbols,
                name,
                helper,
                ensures_arity,
                "ensures",
                HelperReturn::Value,
            )?;
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
        for helper in &contract.assigns {
            validate_helper(
                symbols,
                name,
                helper,
                param_count,
                "assigns",
                HelperReturn::Void,
            )?;
        }
    }
    Ok(())
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
    let mut clauses = Vec::with_capacity(contract.assigns.len());
    let args = parameter_exprs(&target.typ, target.location);
    for helper in &contract.assigns {
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
        clauses.push(Lambda::as_contract_for(
            &target.typ,
            None::<InternedString>,
            call,
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

fn validate_helper(
    symbols: &SymbolTable,
    target_name: &str,
    helper_name: &str,
    expected_arity: usize,
    kind: &str,
    expected_return: HelperReturn,
) -> Result<()> {
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
        HelperReturn::Void => {
            if actual_return != Type::empty() {
                bail!("{kind} helper `{helper_name}` for `{target_name}` must return void");
            }
        }
        HelperReturn::Value => {
            if actual_return == Type::empty() {
                bail!("{kind} helper `{helper_name}` for `{target_name}` must return a value");
            }
        }
        HelperReturn::Exact(expected_return) => {
            if actual_return == Type::empty() {
                bail!("{kind} helper `{helper_name}` for `{target_name}` must return a value");
            }
            if actual_return != expected_return {
                bail!(
                    "{kind} helper `{helper_name}` for `{target_name}` returns {:?}, expected {:?}",
                    actual_return,
                    expected_return
                );
            }
        }
    }
    Ok(())
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
    if !target.is_extern || !matches!(target.value, SymbolValues::None) {
        bail!("assigns contract for `{name}` currently requires an external declaration");
    }

    let loc = target.location;
    ensure_function_parameters(symbols, name, &target.typ, &loc);
    let params = parameter_exprs(&target.typ, loc);
    let mut body = Vec::new();
    for helper in &contract.assigns {
        let helper_symbol = symbols
            .lookup(helper.as_str())
            .ok_or_else(|| anyhow::anyhow!("assigns helper `{helper}` is missing"))?;
        let helper_expr = helper_symbol.to_expr().with_location(loc);
        let args = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &helper_expr,
            params.clone(),
            symbols.machine_model(),
        );
        body.push(Stmt::function_call(None, helper_expr, args, loc));
    }
    let return_type = target
        .typ
        .return_type()
        .cloned()
        .unwrap_or_else(Type::empty);
    if return_type != Type::empty() {
        body.push(Stmt::ret(Some(Expr::nondet(return_type)), loc));
    } else {
        body.push(Stmt::ret(None, loc));
    }

    symbols.update_fn_declaration_with_definition(name, Stmt::block(body, loc));
    if let Some(symbol) = symbols.lookup_mut(name) {
        symbol.is_extern = false;
    }
    Ok(())
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
