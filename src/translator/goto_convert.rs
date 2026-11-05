// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Goto-conversion: lower structured `Stmt` function bodies to flat GOTO instruction lists.
//!
//! CBMC runs this conversion (`goto_convert`) itself when it loads a binary whose function map is
//! empty. Other consumers, notably ESBMC, do not, and would see every function as bodiless. The
//! lowering mirrors CBMC's for the statements the translator emits and fails closed on the rest:
//!
//! - `return` sets the return value, kills every live local, and jumps to `END_FUNCTION`.
//! - Leaving a block kills its locals (unless it ends in an unconditional jump), and a `goto`
//!   kills the locals of every block it leaves. Labels may only appear outside nested blocks,
//!   and no top-level declaration may follow a label or `goto`, so CBMC's scope tree reduces to a
//!   stack and a `goto` always leaves every nested block and never enters a scope.
//! - `nondet` is the only side effect expressions may contain; CBMC would hoist any other.
//! - Bodiless callees that CBMC's conversion rewrites by name are lowered the same way or rejected
//!   ([`is_cbmc_special_call`]), and SV-COMP's `__VERIFIER_atomic_*` function bodies are atomic.

use std::collections::HashMap;

use anyhow::{Context as _, Result, anyhow, bail};
use cprover_bindings::InternedString;
use cprover_bindings::goto_program::{
    Expr, ExprValue, Location, Stmt, StmtBody, SymbolTable, SymbolValues,
};
use cprover_bindings::irep::goto_binary_serde::GotoInstruction;
use cprover_bindings::irep::{Irep, IrepId, ToIrep};

use crate::util;

// CBMC's `goto_program_instruction_typet`.
const GOTO: usize = 1;
const ASSUME: usize = 2;
const ASSERT: usize = 3;
const OTHER: usize = 4;
const SKIP: usize = 5;
const END_FUNCTION: usize = 9;
const ATOMIC_BEGIN: usize = 10;
const ATOMIC_END: usize = 11;
const SET_RETURN_VALUE: usize = 12;
const ASSIGN: usize = 13;
const DECL: usize = 14;
const DEAD: usize = 15;
const FUNCTION_CALL: usize = 16;

/// Converted function bodies, ready for `write_goto_binary_file_with_functions`.
pub type GotoFunctions = Vec<(InternedString, Vec<GotoInstruction>)>;

/// Goto-convert every function body in `symbols`.
pub fn convert(symbols: &SymbolTable) -> Result<GotoFunctions> {
    let true_irep = Expr::bool_true().to_irep(symbols.machine_model());
    symbols
        .iter()
        .filter_map(|(name, symbol)| match &symbol.value {
            SymbolValues::Stmt(body) => Some((*name, body)),
            _ => None,
        })
        .map(|(name, body)| {
            let mut function = Function {
                symbols,
                true_irep: true_irep.clone(),
                instructions: Vec::new(),
                pending: Vec::new(),
                labels: HashMap::new(),
                scopes: Vec::new(),
                scope_closed: false,
                atomic: false,
            };
            let instructions = function
                .body(name, body)
                .and_then(|()| function.finish())
                .with_context(|| format!("goto-converting `{name}`"))?;
            Ok((name, instructions))
        })
        .collect()
}

enum Dest {
    Index(usize),
    Label(InternedString),
    End,
}

struct Function<'a> {
    symbols: &'a SymbolTable,
    true_irep: Irep,
    instructions: Vec<GotoInstruction>,
    /// Jumps to resolve once every label and the `END_FUNCTION` are known.
    pending: Vec<(usize, Dest)>,
    labels: HashMap<InternedString, usize>,
    /// Locals declared in each enclosing block, outermost first.
    scopes: Vec<Vec<Expr>>,
    /// A label or `goto` was seen, so top-level declarations would change scope structure.
    scope_closed: bool,
    /// The whole body is one atomic section, which returns leave through its `ATOMIC_END`.
    atomic: bool,
}

impl Function<'_> {
    fn body(&mut self, name: InternedString, body: &Stmt) -> Result<()> {
        let StmtBody::Block(stmts) = body.body() else {
            bail!("function body is not a block");
        };
        // As CBMC's `convert_function`.
        self.atomic = name.to_string().starts_with("__VERIFIER_atomic_");
        if self.atomic {
            self.push(ATOMIC_BEGIN, Irep::nil(), *body.location(), None);
        }
        self.block(stmts, *body.location(), false)
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<()> {
        let loc = *stmt.location();
        match stmt.body() {
            StmtBody::Assign { .. } => self.code(ASSIGN, stmt),
            StmtBody::Dead(_) => self.code(DEAD, stmt),
            StmtBody::Expression(_) => self.code(OTHER, stmt),
            StmtBody::Skip => self.push(SKIP, Irep::nil(), loc, None),
            StmtBody::Assert { cond, .. } => {
                self.push(ASSERT, Irep::nil(), loc, Some(cond));
                // As CBMC's `convert_assert`; `--no-assertions` and friends key on it.
                let location = &mut self.instructions.last_mut().expect("pushed").location;
                *location = std::mem::replace(location, Irep::nil())
                    .with_named_sub(IrepId::from_string("user-provided"), Irep::one());
            }
            StmtBody::Assume { cond } => self.push(ASSUME, Irep::nil(), loc, Some(cond)),
            StmtBody::Decl { lhs, value } => {
                if self.scope_closed && self.scopes.len() == 1 {
                    bail!("declaration after a label or goto in the top-level block");
                }
                self.code(DECL, &Stmt::decl(lhs.clone(), None, loc));
                if let Some(value) = value {
                    self.code(ASSIGN, &Stmt::assign(lhs.clone(), value.clone(), loc));
                }
                let scope = self.scopes.last_mut().expect("bodies are blocks");
                scope.push(lhs.clone());
            }
            StmtBody::Block(stmts) => self.block(stmts, loc, false)?,
            StmtBody::AtomicBlock(stmts) => self.block(stmts, loc, true)?,
            StmtBody::Return(value) => {
                if value.is_some() {
                    self.code(SET_RETURN_VALUE, stmt);
                }
                let live: Vec<Expr> = self.scopes.iter().flatten().cloned().collect();
                self.kill(&live, loc);
                self.goto(Dest::End, loc);
            }
            StmtBody::Goto {
                dest,
                loop_invariants,
                loop_modifies,
            } => {
                if loop_invariants.is_some() || loop_modifies.is_some() {
                    bail!("loop contracts on `goto {dest}` are not supported");
                }
                self.scope_closed = true;
                let leaving: Vec<Expr> = self.scopes[1..].iter().flatten().cloned().collect();
                self.kill(&leaving, loc);
                self.goto(Dest::Label(*dest), loc);
            }
            StmtBody::Label { label, body } => {
                if self.scopes.len() != 1 {
                    bail!("label `{label}` is inside a nested block");
                }
                if label.to_string().starts_with("__CPROVER_ASYNC_") {
                    bail!("thread-creation label `{label}` is not supported");
                }
                self.scope_closed = true;
                let start = self.instructions.len();
                self.stmt(body)?;
                if self.instructions.len() == start {
                    self.push(SKIP, Irep::nil(), loc, None);
                }
                if self.labels.insert(*label, start).is_some() {
                    bail!("duplicate label `{label}`");
                }
                self.instructions[start].labels.push(*label);
            }
            StmtBody::Ifthenelse { i, t, e } => {
                // CBMC's `generate_ifthenelse`: `if(!c) goto y/z; t; goto z; y: e; z: SKIP`, where
                // a then-branch ending in a jump needs neither `goto z` nor, with an else, `z`.
                let skip_then = self.jump(Some(&i.clone().not()), loc);
                self.stmt(t)?;
                let then_jumps = self.ends_in_jump();
                let skip_else = match e {
                    Some(e) => {
                        let skip_else = (!then_jumps).then(|| self.jump(None, loc));
                        self.land(skip_then);
                        self.stmt(e)?;
                        skip_else
                    }
                    None => Some(skip_then),
                };
                if let Some(skip_else) = skip_else {
                    self.land(skip_else);
                    self.push(SKIP, Irep::nil(), loc, None);
                }
            }
            StmtBody::Switch {
                control,
                cases,
                default,
            } => {
                // C semantics: dispatch on the case values, then lay out the bodies in order so
                // that each falls through to the next.
                let dispatch: Vec<usize> = cases
                    .iter()
                    .map(|case| self.jump(Some(&control.clone().eq(case.case().clone())), loc))
                    .collect();
                let to_default = self.jump(None, loc);
                for (jump, case) in dispatch.into_iter().zip(cases) {
                    self.land(jump);
                    self.stmt(case.body())?;
                }
                self.land(to_default);
                if let Some(default) = default {
                    self.stmt(default)?;
                }
                self.push(SKIP, Irep::nil(), loc, None);
            }
            StmtBody::FunctionCall {
                lhs,
                function,
                arguments,
            } => match self.bodiless_callee(function).as_deref() {
                Some(name)
                    if name.starts_with("nondet_") || name.starts_with("__VERIFIER_nondet_") =>
                {
                    if let Some(lhs) = lhs {
                        let nondet = util::nondet_expr(lhs.typ().clone());
                        self.code(ASSIGN, &Stmt::assign(lhs.clone(), nondet, loc));
                    }
                }
                Some("__CPROVER_havoc_object") => {
                    let ([object], None) = (arguments.as_slice(), lhs) else {
                        bail!("`__CPROVER_havoc_object` takes one argument and returns nothing");
                    };
                    let mut code = Irep::just_sub(vec![object.to_irep(self.mm())])
                        .with_named_sub(IrepId::Statement, Irep::just_id(IrepId::HavocObject));
                    code.id = IrepId::Code;
                    self.push(OTHER, code, loc, None);
                }
                Some(
                    name @ ("__CPROVER_atomic_begin"
                    | "__VERIFIER_atomic_begin"
                    | "__CPROVER_atomic_end"
                    | "__VERIFIER_atomic_end"),
                ) => {
                    let ([], None) = (arguments.as_slice(), lhs) else {
                        bail!("`{name}` takes no arguments and returns nothing");
                    };
                    let kind = if name.ends_with("begin") {
                        ATOMIC_BEGIN
                    } else {
                        ATOMIC_END
                    };
                    self.push(kind, Irep::nil(), loc, None);
                }
                Some(name) if is_cbmc_special_call(name) => {
                    bail!(
                        "call to bodiless `{name}`, which CBMC's goto-convert rewrites, is not supported"
                    )
                }
                _ => self.code(FUNCTION_CALL, stmt),
            },
            StmtBody::Break
            | StmtBody::Continue
            | StmtBody::For { .. }
            | StmtBody::While { .. } => {
                bail!("unsupported statement {:?}", stmt.body())
            }
        }
        Ok(())
    }

    fn block(&mut self, stmts: &[Stmt], loc: Location, atomic: bool) -> Result<()> {
        self.scopes.push(Vec::new());
        if atomic {
            self.push(ATOMIC_BEGIN, Irep::nil(), loc, None);
        }
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        if atomic {
            self.push(ATOMIC_END, Irep::nil(), loc, None);
        }
        let scope = self.scopes.pop().expect("pushed above");
        if !self.ends_in_jump() {
            self.kill(&scope, loc);
        }
        Ok(())
    }

    fn ends_in_jump(&self) -> bool {
        self.instructions
            .last()
            .is_some_and(|last| last.kind == GOTO && last.guard == self.true_irep)
    }

    /// Mark the end of life of `locals`, innermost first.
    fn kill(&mut self, locals: &[Expr], loc: Location) {
        for local in locals.iter().rev() {
            self.code(DEAD, &Stmt::dead(local.clone(), loc));
        }
    }

    fn goto(&mut self, dest: Dest, loc: Location) {
        let jump = self.jump(None, loc);
        self.pending.push((jump, dest));
    }

    /// Push a forward `GOTO` whose destination is set by [`Self::land`].
    fn jump(&mut self, guard: Option<&Expr>, loc: Location) -> usize {
        self.push(GOTO, Irep::nil(), loc, guard);
        self.instructions.len() - 1
    }

    fn land(&mut self, jump: usize) {
        self.pending
            .push((jump, Dest::Index(self.instructions.len())));
    }

    fn code(&mut self, kind: usize, stmt: &Stmt) {
        let code = stmt.to_irep(self.mm());
        self.push(kind, code, *stmt.location(), None);
    }

    fn push(&mut self, kind: usize, code: Irep, location: Location, guard: Option<&Expr>) {
        let mm = self.symbols.machine_model();
        self.instructions.push(GotoInstruction {
            code,
            location: location.to_irep(mm),
            kind,
            guard: guard.map_or_else(|| self.true_irep.clone(), |guard| guard.to_irep(mm)),
            target_number: None,
            targets: Vec::new(),
            labels: Vec::new(),
        });
    }

    fn mm(&self) -> &cprover_bindings::MachineModel {
        self.symbols.machine_model()
    }

    fn bodiless_callee(&self, function: &Expr) -> Option<String> {
        let ExprValue::Symbol { identifier } = function.value() else {
            return None;
        };
        let symbol = self.symbols.lookup(*identifier)?;
        matches!(symbol.value, SymbolValues::None).then(|| identifier.to_string())
    }

    fn finish(&mut self) -> Result<Vec<GotoInstruction>> {
        if self
            .instructions
            .iter()
            .any(|i| needs_cleaning(&i.code) || needs_cleaning(&i.guard))
        {
            bail!("expression with side effects other than nondet");
        }
        let end = self.instructions.len();
        if self.atomic {
            self.push(ATOMIC_END, Irep::nil(), Location::None, None);
        }
        self.push(END_FUNCTION, Irep::nil(), Location::None, None);
        for (jump, dest) in std::mem::take(&mut self.pending) {
            let target = match dest {
                Dest::Index(index) => index,
                Dest::End => end,
                Dest::Label(label) => *self
                    .labels
                    .get(&label)
                    .ok_or_else(|| anyhow!("goto to undefined label `{label}`"))?,
            };
            self.instructions[jump].targets.push(target);
            self.instructions[target].target_number = Some(target);
        }
        Ok(std::mem::take(&mut self.instructions))
    }
}

/// Whether CBMC's `clean_expr` would rewrite `irep` (`needs_cleaning`), leaving `nondet` alone.
fn needs_cleaning(irep: &Irep) -> bool {
    let rewritten_side_effect = irep.id == IrepId::SideEffect
        && irep.lookup(IrepId::Statement).map(|s| &s.id) != Some(&IrepId::Nondet);
    rewritten_side_effect
        || matches!(irep.id, IrepId::Comma | IrepId::CompoundLiteral)
        || irep.sub.iter().any(needs_cleaning)
        || irep.named_sub.iter().any(|(_, sub)| needs_cleaning(sub))
}

/// Bodiless callees that CBMC's goto-convert rewrites by name (`do_function_call_symbol` in
/// `builtin_functions.cpp`) instead of calling. Emitting them as plain calls would silently change
/// their meaning, so those without a lowering in [`Function::stmt`] are rejected.
#[rustfmt::skip]
fn is_cbmc_special_call(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "__CPROVER_assume", "__CPROVER_assert", "__CPROVER_precondition",
        "__CPROVER_postcondition", "__CPROVER_havoc_slice", "__CPROVER_printf", "__CPROVER_scanf",
        "__CPROVER_input", "__CPROVER_output", "__CPROVER::input", "__CPROVER::output",
        "__CPROVER::atomic_begin", "__CPROVER::atomic_end", "__CPROVER_prob_biased_coin", "__CPROVER_array_equal",
        "__CPROVER_array_set", "__CPROVER_array_copy", "__CPROVER_array_replace",
        "__CPROVER_fence", "__VERIFIER_assume", "__VERIFIER_error", "assert", "__assert_fail", "_assert", "__assert_c99", "_wassert",
        "__assert_rtn", "__assert", "__assert_func", "__assert2", "__assert13",
        "__builtin_prefetch", "__builtin_unreachable", "__builtin_va_arg", "__builtin_va_copy",
        "__builtin_va_start", "__va_start", "__builtin_va_end", "__builtin_isgreater",
        "__builtin_isgreaterequal", "__builtin_isless", "__builtin_islessequal",
        "__builtin_islessgreater", "__builtin_isunordered", "alloca", "__builtin_alloca",
    ];
    const PREFIXES: &[&str] = &["__CPROVER_prob_uniform_", "__CPROVER_uninterpreted_"];
    NAMES.contains(&name) || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cprover_bindings::goto_program::{Symbol, Type};

    fn convert_body(stmts: Vec<Stmt>, extra: Vec<Symbol>) -> Result<GotoFunctions> {
        let loc = Location::None;
        let machine_model = util::machine_model(crate::MachineModel::Lp64, "x86_64".into(), false);
        let mut symbols = SymbolTable::new(machine_model);
        let typ = Type::code(vec![], Type::empty());
        symbols.insert(Symbol::function(
            "f",
            typ,
            Some(Stmt::block(stmts, loc)),
            "f",
            loc,
        ));
        extra.into_iter().for_each(|symbol| symbols.insert(symbol));
        convert(&symbols)
    }

    fn shape(functions: &GotoFunctions) -> Vec<(usize, Option<usize>, Vec<usize>)> {
        let instructions = &functions[0].1;
        instructions
            .iter()
            .map(|i| (i.kind, i.target_number, i.targets.clone()))
            .collect()
    }

    #[test]
    fn lowers_control_flow_and_lifetimes() {
        let loc = Location::None;
        let x = Expr::symbol_expression("f::x", Type::signed_int(32));
        let is_zero = x.clone().eq(Type::signed_int(32).zero());
        let then_return = Stmt::if_then_else(is_zero, Stmt::ret(None, loc), None, loc);
        let body = Stmt::block(vec![then_return, Stmt::goto("L", loc)], loc).with_label("L");
        let functions = convert_body(vec![Stmt::decl(x, None, loc), body], vec![]).unwrap();
        // DECL x; L: GOTO [!(x == 0)] 4; DEAD x; GOTO end; 4: SKIP; GOTO L; end: END_FUNCTION.
        // The trailing unconditional jump suppresses the block-exit DEADs.
        assert_eq!(
            shape(&functions),
            vec![
                (DECL, None, vec![]),
                (GOTO, Some(1), vec![4]),
                (DEAD, None, vec![]),
                (GOTO, None, vec![6]),
                (SKIP, Some(4), vec![]),
                (GOTO, None, vec![1]),
                (END_FUNCTION, Some(6), vec![]),
            ]
        );
    }

    #[test]
    fn kills_locals_on_fall_through_past_a_jump() {
        let loc = Location::None;
        let t = Expr::symbol_expression("f::t", Type::signed_int(32));
        let is_zero = t.clone().eq(Type::signed_int(32).zero());
        let then_goto = Stmt::if_then_else(is_zero, Stmt::goto("L", loc), None, loc);
        let body = Stmt::block(vec![Stmt::decl(t, None, loc), then_goto], loc).with_label("L");
        let functions = convert_body(vec![body], vec![]).unwrap();
        // L: DECL t; GOTO [!(t == 0)] 4; DEAD t; GOTO L; 4: SKIP; DEAD t; END_FUNCTION.
        assert_eq!(
            shape(&functions),
            vec![
                (DECL, Some(0), vec![]),
                (GOTO, None, vec![4]),
                (DEAD, None, vec![]),
                (GOTO, None, vec![0]),
                (SKIP, Some(4), vec![]),
                (DEAD, None, vec![]),
                (END_FUNCTION, None, vec![]),
            ]
        );
    }

    #[test]
    fn rejects_calls_cbmc_rewrites_by_name() {
        let loc = Location::None;
        let error = Symbol::function(
            "__VERIFIER_error",
            Type::code(vec![], Type::empty()),
            None,
            "__VERIFIER_error",
            loc,
        );
        let call = Stmt::function_call(None, error.to_expr(), vec![], loc);
        let result = convert_body(vec![call], vec![error]);
        assert!(
            format!("{:#}", result.err().expect("special call must be rejected"))
                .contains("__VERIFIER_error")
        );
    }
}
