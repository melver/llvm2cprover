// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! LLVM instruction lowering into GOTO function bodies.
//!
//! This module owns actual executable lowering:
//!
//! - SSA values become declared temporaries
//! - PHI nodes are lowered on predecessor edges
//! - basic blocks become labeled GOTO blocks
//! - ordinary contracts are injected at call sites, while explicitly selected calls redirect
//!   to internal replacement functions synthesized from the compiled helpers
//! - exactly matched inline assembly redirects to defined compiled model functions;
//!   modeled `callbr` executes the model once and nondeterministically follows one
//!   of its declared successor edges
//!
//! Unsupported instructions fail where they are encountered, which keeps
//! diagnostics specific and avoids a second feature-classification pass.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use cprover_bindings::InternedString;
use cprover_bindings::goto_program::{
    BuiltinFn, Expr, ExprValue, Location, Stmt, SwitchCase, Symbol, SymbolTable, Type,
    arithmetic_overflow_result_type,
};
use inkwell::AtomicRMWBinOp;
use inkwell::FloatPredicate;
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::llvm_sys::core::{
    LLVMGetBasicBlockTerminator, LLVMGetCalledFunctionType, LLVMGetCalledValue, LLVMGetClause,
    LLVMGetMaskValue, LLVMGetNormalDest, LLVMGetNumArgOperands, LLVMGetNumClauses,
    LLVMGetNumMaskElements, LLVMGetNumOperandBundles, LLVMGetNumSuccessors, LLVMGetOperand,
    LLVMGetSuccessor, LLVMGetSwitchCaseValue, LLVMGetUnwindDest, LLVMIsAConstantExpr,
    LLVMIsAGlobalVariable, LLVMIsAInlineAsm, LLVMIsCleanup, LLVMIsUndef,
};
use inkwell::llvm_sys::prelude::{LLVMBasicBlockRef, LLVMValueRef};
use inkwell::types::{AnyType, AnyTypeEnum, BasicTypeEnum, FunctionType};
use inkwell::values::{
    AnyValue, AsValueRef, BasicValue, BasicValueEnum, CallSiteValue, FunctionValue, GlobalValue,
    InstructionOpcode, InstructionValue, PhiValue,
};
use num::bigint::BigInt;

use super::globals::GlobalLowerer;
use super::report::ReportCollector;
use super::types::TypeLowerer;
use super::{Diagnostic, EH_ACTIVE_SYMBOL, EH_PTR_SYMBOL, EH_SELECTOR_SYMBOL, UnsupportedFeature};
use crate::contracts::inject::ReplacementMap;
use crate::contracts::{ContractMap, FunctionContract, InlineAsmSite};
use crate::util;
use crate::{UnsoundFeature, UnsoundOptions};

/// Lowers supported LLVM function bodies into GOTO statements.
///
/// The lowerer is shared across functions but all per-function state lives in
/// the internal [`FunctionLowerer`].
#[derive(Debug)]
pub struct InstructionLowerer<'a> {
    contracts: &'a ContractMap,
    replacements: &'a ReplacementMap,
    unsound: &'a UnsoundOptions,
    report: &'a mut ReportCollector,
}

impl<'a> InstructionLowerer<'a> {
    /// Create an instruction lowerer with the active contract map.
    pub(crate) fn new(
        contracts: &'a ContractMap,
        replacements: &'a ReplacementMap,
        unsound: &'a UnsoundOptions,
        report: &'a mut ReportCollector,
    ) -> Self {
        Self {
            contracts,
            replacements,
            unsound,
            report,
        }
    }

    /// Lower one defined LLVM function and attach its body to the symbol table.
    pub fn lower_function<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
        symbols: &mut SymbolTable,
        types: &mut TypeLowerer,
        globals: &mut GlobalLowerer,
    ) -> Result<()> {
        FunctionLowerer::new(
            function,
            symbols,
            types,
            globals,
            self.contracts,
            self.replacements,
            self.unsound,
            self.report,
        )
        .lower()
    }
}

#[derive(Debug)]
struct FunctionLowerer<'a, 'ctx, 'c> {
    function: FunctionValue<'ctx>,
    function_name: String,
    symbols: &'a mut SymbolTable,
    types: &'a mut TypeLowerer,
    globals: &'a mut GlobalLowerer,
    contracts: &'c ContractMap,
    replacements: &'c ReplacementMap,
    unsound: &'c UnsoundOptions,
    report: &'c mut ReportCollector,
    suppress_contracts: bool,
    block_labels: HashMap<BasicBlock<'ctx>, String>,
    current_block: Option<BasicBlock<'ctx>>,
    values: HashMap<usize, Expr>,
    aggregates: HashMap<usize, Vec<Expr>>,
    prologue: Vec<Stmt>,
    next_tmp: usize,
    // Stack of (stacksave value-ref key, VLA slot exprs allocated since that save).
    // Innermost active save is last; stackrestore pops up to its matching entry and
    // emits Stmt::dead for each tracked VLA slot.
    stack_saves: Vec<(usize, Vec<Expr>)>,
}

impl<'a, 'ctx, 'c> FunctionLowerer<'a, 'ctx, 'c> {
    fn new(
        function: FunctionValue<'ctx>,
        symbols: &'a mut SymbolTable,
        types: &'a mut TypeLowerer,
        globals: &'a mut GlobalLowerer,
        contracts: &'c ContractMap,
        replacements: &'c ReplacementMap,
        unsound: &'c UnsoundOptions,
        report: &'c mut ReportCollector,
    ) -> Self {
        let function_name = function.get_name().to_string_lossy().into_owned();
        Self {
            function,
            // Contract helpers are lowered as ordinary functions, but their bodies must not be
            // wrapped in further contract checks or model substitution.
            suppress_contracts: contracts.is_helper(function_name.as_str()),
            function_name,
            symbols,
            types,
            globals,
            contracts,
            replacements,
            unsound,
            report,
            block_labels: HashMap::new(),
            current_block: None,
            values: HashMap::new(),
            aggregates: HashMap::new(),
            prologue: Vec::new(),
            next_tmp: 0,
            stack_saves: Vec::new(),
        }
    }

    /// The internal symbol a direct call to `callee_name` is redirected to, if any.
    ///
    /// Contract helpers are compiled as ordinary code, so a helper that calls its own target
    /// must not recursively re-enter the replacement it implements.
    fn replacement_symbol(&self, callee_name: &str) -> Option<&'c str> {
        if self.suppress_contracts {
            return None;
        }
        self.replacements.get(callee_name).map(String::as_str)
    }

    /// Returns the function's basic blocks in reverse post-order (RPO).
    ///
    /// RPO guarantees every dominator is visited before the blocks it dominates.
    /// That is required so that when `phi_edge_assignments` resolves an incoming
    /// SSA value `[%v, %pred]`, the definition of `%v` has already been lowered
    /// and is present in `self.values`.  The IR textual order produced by the
    /// optimizer may place loop-exit blocks before the loop body, breaking the
    /// assumption that definitions precede their uses in block order.
    fn blocks_in_rpo(&self) -> Vec<BasicBlock<'ctx>> {
        let entry = match self.function.get_first_basic_block() {
            Some(b) => b,
            None => return Vec::new(),
        };

        let mut post_order: Vec<LLVMBasicBlockRef> = Vec::new();
        let mut visited: HashSet<LLVMBasicBlockRef> = HashSet::new();

        // Stack entries: (block, terminator, next-successor-index).
        // Storing the terminator avoids re-fetching it on every iteration.
        // Marking blocks visited at push-time prevents them from being pushed twice.
        let entry_ptr = entry.as_mut_ptr();
        let entry_term = unsafe { LLVMGetBasicBlockTerminator(entry_ptr) };
        visited.insert(entry_ptr);
        let mut stack: Vec<(LLVMBasicBlockRef, LLVMValueRef, u32)> =
            vec![(entry_ptr, entry_term, 0)];

        while let Some((bb, term, next_succ)) = stack.last_mut() {
            let (bb, term) = (*bb, *term);
            let n_succs = if term.is_null() {
                0
            } else {
                unsafe { LLVMGetNumSuccessors(term) }
            };
            let idx = *next_succ;
            if idx < n_succs {
                *next_succ += 1;
                let succ = unsafe { LLVMGetSuccessor(term, idx) };
                if visited.insert(succ) {
                    let succ_term = unsafe { LLVMGetBasicBlockTerminator(succ) };
                    stack.push((succ, succ_term, 0));
                }
            } else {
                post_order.push(bb);
                stack.pop();
            }
        }

        // RPO = reverse of post-order.  Map raw refs back to inkwell wrappers.
        post_order
            .into_iter()
            .rev()
            .map(|bb| unsafe { BasicBlock::new(bb).expect("valid basic block") })
            .collect()
    }

    fn lower(mut self) -> Result<()> {
        self.declare_parameters()?;
        let all_blocks = self.function.get_basic_blocks();
        if all_blocks.is_empty() {
            return Ok(());
        }
        // `init_block_labels` and `declare_phi_results` are order-independent;
        // use the plain IR list so every block gets a label even if unreachable.
        self.init_block_labels(&all_blocks);
        self.declare_phi_results()?;

        // Lower blocks in RPO so that every SSA definition is available before
        // phi_edge_assignments tries to reference it on a successor edge.
        let rpo_blocks = self.blocks_in_rpo();
        let mut body = Vec::new();
        for block in rpo_blocks {
            body.push(self.lower_basic_block(block)?);
        }
        // Many SSA temps and hidden locals are only discovered while lowering instructions, so
        // collect the prologue after visiting every block. Otherwise recursive calls reuse one
        // symbol object instead of getting a fresh DECL per activation record.
        let mut prologue = std::mem::take(&mut self.prologue);
        prologue.extend(body);

        self.symbols.update_fn_declaration_with_definition(
            self.function_name.clone(),
            Stmt::block(prologue, self.function_location()),
        );
        Ok(())
    }

    fn init_block_labels(&mut self, blocks: &[BasicBlock<'ctx>]) {
        for (index, block) in blocks.iter().copied().enumerate() {
            let name = block.get_name().to_string_lossy();
            let suffix = if name.is_empty() {
                format!("bb{index}")
            } else {
                format!("bb{index}_{}", util::sanitize_name(name.as_ref()))
            };
            self.block_labels.insert(
                block,
                format!("{}::{}", util::sanitize_name(&self.function_name), suffix),
            );
        }
    }

    fn lower_basic_block(&mut self, block: BasicBlock<'ctx>) -> Result<Stmt> {
        self.current_block = Some(block);
        let mut body = Vec::new();
        for instruction in block.get_instructions() {
            if instruction.get_opcode() == InstructionOpcode::Phi {
                continue;
            }
            self.lower_instruction(instruction, &mut body)?;
        }
        self.current_block = None;
        Ok(Stmt::block(body, self.block_location(block)).with_label(self.block_label(block)?))
    }

    fn declare_phi_results(&mut self) -> Result<()> {
        for block in self.function.get_basic_blocks() {
            for instruction in block.get_instructions() {
                if instruction.get_opcode() != InstructionOpcode::Phi {
                    break;
                }
                self.declare_instruction_result(instruction)?;
            }
        }
        Ok(())
    }

    fn declare_parameters(&mut self) -> Result<()> {
        let loc = self.function_location();
        for (index, param) in self.function.get_params().into_iter().enumerate() {
            let byval_type = self
                .types
                .lower_param_byval(self.function, index as u32)
                .map_err(|diag| diag.with_symbol(self.function_name.clone()))?;
            let typ = match &byval_type {
                Some(pointee) => pointee.clone().to_pointer(),
                None => self.types.lower_parameter(param)?,
            };
            self.insert_pending_aggregate_symbols();
            let base =
                self.value_base_name(index, "arg", param.get_name().to_string_lossy().as_ref());
            let expr = self.insert_local_symbol(&base, typ, true, false, loc);
            if let Some(byval_type) = byval_type {
                // LLVM `byval(T)` passes a callee-owned copy of `*param` while keeping the formal
                // parameter pointer in the function signature.
                let copy = self.declare_hidden_local("byval", byval_type, loc);
                self.prologue
                    .push(Stmt::assign(copy.clone(), expr.dereference(), loc));
                self.values.insert(
                    param.as_value_ref() as usize,
                    copy.address_of().with_location(loc),
                );
            } else {
                self.values.insert(param.as_value_ref() as usize, expr);
            }
        }
        Ok(())
    }

    fn lower_instruction(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        match instruction.get_opcode() {
            InstructionOpcode::Phi => Ok(()),
            InstructionOpcode::Add
            | InstructionOpcode::Sub
            | InstructionOpcode::Mul
            | InstructionOpcode::UDiv
            | InstructionOpcode::SDiv
            | InstructionOpcode::URem
            | InstructionOpcode::SRem
            | InstructionOpcode::FAdd
            | InstructionOpcode::FSub
            | InstructionOpcode::FMul
            | InstructionOpcode::FDiv
            | InstructionOpcode::FRem
            | InstructionOpcode::And
            | InstructionOpcode::Or
            | InstructionOpcode::Xor
            | InstructionOpcode::Shl
            | InstructionOpcode::LShr
            | InstructionOpcode::AShr => self.lower_binary_op(instruction, body),
            InstructionOpcode::ICmp => self.lower_icmp(instruction, body),
            InstructionOpcode::FCmp => self.lower_fcmp(instruction, body),
            InstructionOpcode::FNeg => self.lower_fneg(instruction, body),
            InstructionOpcode::Alloca => self.lower_alloca(instruction, body),
            InstructionOpcode::Load => self.lower_load(instruction, body),
            InstructionOpcode::Store => self.lower_store(instruction, body),
            InstructionOpcode::GetElementPtr => self.lower_gep(instruction, body),
            InstructionOpcode::UncondBr | InstructionOpcode::CondBr => {
                self.lower_branch(instruction, body)
            }
            InstructionOpcode::Switch => self.lower_switch(instruction, body),
            InstructionOpcode::ZExt
            | InstructionOpcode::SExt
            | InstructionOpcode::Trunc
            | InstructionOpcode::BitCast
            | InstructionOpcode::PtrToInt
            | InstructionOpcode::IntToPtr
            | InstructionOpcode::AddrSpaceCast
            | InstructionOpcode::FPExt
            | InstructionOpcode::FPTrunc
            | InstructionOpcode::FPToSI
            | InstructionOpcode::FPToUI
            | InstructionOpcode::SIToFP
            | InstructionOpcode::UIToFP => self.lower_cast(instruction, body),
            InstructionOpcode::Freeze => self.lower_freeze(instruction, body),
            InstructionOpcode::Select => self.lower_select(instruction, body),
            InstructionOpcode::Return => self.lower_return(instruction, body),
            InstructionOpcode::Call => self.lower_call(instruction, body),
            InstructionOpcode::CallBr => self.lower_callbr(instruction, body),
            InstructionOpcode::Invoke => self.lower_invoke(instruction, body),
            InstructionOpcode::LandingPad => self.lower_landingpad(instruction, body),
            InstructionOpcode::Resume => self.lower_resume(instruction, body),
            InstructionOpcode::InsertElement => self.lower_insertelement(instruction, body),
            InstructionOpcode::ExtractElement => self.lower_extractelement(instruction, body),
            InstructionOpcode::ShuffleVector => self.lower_shufflevector(instruction, body),
            InstructionOpcode::AtomicRMW => self.lower_atomic_rmw(instruction, body),
            InstructionOpcode::AtomicCmpXchg => self.lower_atomic_cmpxchg(instruction, body),
            InstructionOpcode::ExtractValue => self.lower_extractvalue(instruction, body),
            InstructionOpcode::InsertValue => self.lower_insertvalue(instruction, body),
            InstructionOpcode::Unreachable => {
                body.push(Stmt::assert_false(
                    "unreachable",
                    "llvm.unreachable",
                    self.instruction_location(instruction),
                ));
                Ok(())
            }
            opcode => Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "unsupported LLVM instruction {:?} in function {}",
                    opcode, self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
            .into()),
        }
    }

    fn lower_binary_op(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let lhs = self.lower_operand(instruction, 0)?;
        let rhs = self.lower_operand(instruction, 1)?;
        let result = self.declare_instruction_result(instruction)?;
        let target = result.typ().clone();
        let value = match instruction.get_opcode() {
            InstructionOpcode::Add => lhs
                .cast_to(target.clone())
                .plus(rhs.cast_to(target.clone())),
            InstructionOpcode::Sub => lhs.cast_to(target.clone()).sub(rhs.cast_to(target.clone())),
            InstructionOpcode::Mul => lhs.cast_to(target.clone()).mul(rhs.cast_to(target.clone())),
            InstructionOpcode::UDiv => lhs.cast_to(target.clone()).div(rhs.cast_to(target.clone())),
            InstructionOpcode::SDiv => self
                .signed_view(lhs)?
                .div(self.signed_view(rhs)?)
                .cast_to(target.clone()),
            InstructionOpcode::URem => lhs.cast_to(target.clone()).rem(rhs.cast_to(target.clone())),
            InstructionOpcode::SRem => self
                .signed_view(lhs)?
                .rem(self.signed_view(rhs)?)
                .cast_to(target.clone()),
            InstructionOpcode::FAdd => lhs
                .cast_to(target.clone())
                .plus(rhs.cast_to(target.clone())),
            InstructionOpcode::FSub => lhs.cast_to(target.clone()).sub(rhs.cast_to(target.clone())),
            InstructionOpcode::FMul => lhs.cast_to(target.clone()).mul(rhs.cast_to(target.clone())),
            InstructionOpcode::FDiv => lhs.cast_to(target.clone()).div(rhs.cast_to(target.clone())),
            InstructionOpcode::FRem => lhs.cast_to(target.clone()).rem(rhs.cast_to(target.clone())),
            InstructionOpcode::And => lhs
                .cast_to(target.clone())
                .bitand(rhs.cast_to(target.clone())),
            InstructionOpcode::Or => lhs
                .cast_to(target.clone())
                .bitor(rhs.cast_to(target.clone())),
            InstructionOpcode::Xor => lhs
                .cast_to(target.clone())
                .bitxor(rhs.cast_to(target.clone())),
            InstructionOpcode::Shl | InstructionOpcode::LShr | InstructionOpcode::AShr => {
                self.lower_shift_value(instruction.get_opcode(), lhs, rhs, target)?
            }
            _ => unreachable!("binary opcode dispatched incorrectly"),
        };
        body.push(Stmt::assign(result, value, loc));
        Ok(())
    }

    /// Lower an integer or vector shift (`shl`, `lshr`, `ashr`).
    ///
    /// In LLVM IR, a shift count `>=` the operand bit width produces `poison`, not immediate UB,
    /// so optimizer passes routinely hoist shifts out of their `< width` guards and select over
    /// the result. Guard the shift with `(rhs < width) ? (lhs op clamped_rhs) : nondet` (clamping
    /// `clamped_rhs` inside the taken arm so CBMC's eager `--undefined-shift-check` does not
    /// fault on the untaken expression) to match how literal `poison` is lowered.
    fn lower_shift_value(
        &self,
        opcode: InstructionOpcode,
        lhs: Expr,
        rhs: Expr,
        target: Type,
    ) -> Result<Expr> {
        if target.is_vector() {
            let (len, lane_type) =
                target
                    .len()
                    .zip(target.base_type().cloned())
                    .ok_or_else(|| {
                        Diagnostic::unsupported_feature(
                            UnsupportedFeature::Instruction,
                            format!(
                                "vector shift expects a fixed vector in {}",
                                self.function_name
                            ),
                        )
                        .with_symbol(self.function_name.clone())
                    })?;
            let elems = (0..len)
                .map(|lane| {
                    let lhs_lane = self.aggregate_component(lhs.clone(), lane as usize)?;
                    let rhs_lane = self.aggregate_component(rhs.clone(), lane as usize)?;
                    self.lower_scalar_shift_value(opcode, lhs_lane, rhs_lane, lane_type.clone())
                })
                .collect::<Result<Vec<_>>>()?;
            return Ok(Expr::vector_expr(target, elems));
        }
        self.lower_scalar_shift_value(opcode, lhs, rhs, target)
    }

    fn lower_scalar_shift_value(
        &self,
        opcode: InstructionOpcode,
        lhs: Expr,
        rhs: Expr,
        target: Type,
    ) -> Result<Expr> {
        let width = self.integer_width(&target)?;
        let rhs = rhs.cast_to(target.clone());
        let in_range = rhs.clone().lt(Expr::int_constant(width, target.clone()));
        let clamped_rhs = in_range.clone().ternary(rhs, target.zero());
        let shifted = match opcode {
            InstructionOpcode::Shl => lhs.cast_to(target.clone()).shl(clamped_rhs),
            InstructionOpcode::LShr => lhs.cast_to(target.clone()).lshr(clamped_rhs),
            InstructionOpcode::AShr => self
                .signed_view(lhs)?
                .ashr(clamped_rhs)
                .cast_to(target.clone()),
            _ => unreachable!("shift opcode dispatched incorrectly"),
        };
        Ok(in_range.ternary(shifted, util::nondet_expr(target)))
    }

    fn lower_branch(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let predecessor = self.current_block.ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!(
                    "branch lowered without current block in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
        })?;
        match instruction.get_num_operands() {
            1 => {
                let dest = self.operand_block(
                    instruction,
                    0,
                    UnsupportedFeature::ControlFlow,
                    "malformed branch target",
                )?;
                body.push(self.branch_to(predecessor, dest)?);
                Ok(())
            }
            3 => {
                let cond = self.lower_operand(instruction, 0)?.cast_to(Type::bool());
                let true_dest = self.operand_block(
                    instruction,
                    1,
                    UnsupportedFeature::ControlFlow,
                    "malformed true branch target",
                )?;
                let false_dest = self.operand_block(
                    instruction,
                    2,
                    UnsupportedFeature::ControlFlow,
                    "malformed false branch target",
                )?;
                body.push(Stmt::if_then_else(
                    cond,
                    self.branch_to(predecessor, true_dest)?,
                    Some(self.branch_to(predecessor, false_dest)?),
                    loc,
                ));
                Ok(())
            }
            _ => Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!("unsupported branch shape in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
            .into()),
        }
    }

    fn lower_switch(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let predecessor = self.current_block.ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!(
                    "switch lowered without current block in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let control = self.lower_operand(instruction, 0)?;
        let default_dest = self.operand_block(
            instruction,
            1,
            UnsupportedFeature::ControlFlow,
            "malformed switch default",
        )?;

        let mut cases = Vec::new();
        for successor_index in
            1..unsafe { inkwell::llvm_sys::core::LLVMGetNumSuccessors(instruction.as_value_ref()) }
        {
            let case = self.lower_switch_case_value(instruction, successor_index)?;
            let dest = unsafe {
                BasicBlock::new(inkwell::llvm_sys::core::LLVMGetSuccessor(
                    instruction.as_value_ref(),
                    successor_index,
                ))
            }
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::ControlFlow,
                    format!("malformed switch destination in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
            })?;
            cases.push(SwitchCase::new(
                case.cast_to(control.typ().clone()),
                self.branch_to(predecessor, dest)?,
            ));
        }

        body.push(Stmt::switch(
            control,
            cases,
            Some(self.branch_to(predecessor, default_dest)?),
            loc,
        ));
        Ok(())
    }

    fn lower_switch_case_value(
        &mut self,
        instruction: InstructionValue<'ctx>,
        successor_index: u32,
    ) -> Result<Expr> {
        // Inkwell exposes switch successors as blocks only; case constants still come
        // from the raw C API indexed by successor, with slot 0 reserved for default.
        let raw = unsafe { LLVMGetSwitchCaseValue(instruction.as_value_ref(), successor_index) };
        if raw.is_null() {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!("malformed switch case in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }
        // CBMC now requires switch case operands to carry a non-nil source location, but LLVM
        // case constants arrive as raw values detached from the parent switch instruction.
        self.lower_value(unsafe { BasicValueEnum::new(raw) })
            .map(|expr| expr.with_location(self.instruction_location(instruction)))
    }

    fn branch_to(
        &mut self,
        predecessor: BasicBlock<'ctx>,
        successor: BasicBlock<'ctx>,
    ) -> Result<Stmt> {
        self.branch_to_with_stmts(predecessor, successor, Vec::new())
    }

    fn branch_to_with_stmts(
        &mut self,
        predecessor: BasicBlock<'ctx>,
        successor: BasicBlock<'ctx>,
        mut stmts: Vec<Stmt>,
    ) -> Result<Stmt> {
        let loc = self.block_location(successor);
        stmts.extend(self.phi_edge_assignments(predecessor, successor)?);
        stmts.push(Stmt::goto(self.block_label(successor)?, loc));
        Ok(if stmts.len() == 1 {
            stmts.pop().unwrap()
        } else {
            Stmt::block(stmts, loc)
        })
    }

    fn lower_icmp(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let pred = instruction.get_icmp_predicate().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("malformed icmp in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let lhs = self.lower_operand(instruction, 0)?;
        let rhs = self.lower_operand(instruction, 1)?;
        let result = self.declare_instruction_result(instruction)?;
        let value = if lhs.typ().is_vector() || rhs.typ().is_vector() {
            self.lower_vector_icmp_value(pred, lhs, rhs, result.typ().clone())?
        } else {
            self.lower_scalar_icmp_value(pred, lhs, rhs)?
        };
        body.push(Stmt::assign(
            result.clone(),
            value.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn normalize_icmp_operands(&self, lhs: Expr, rhs: Expr) -> (Expr, Expr) {
        if lhs.typ().is_pointer() && rhs.typ().is_pointer() && lhs.typ() != rhs.typ() {
            // LLVM opaque pointers allow address comparisons across differently recovered pointees.
            // CBMC requires one concrete pointer type, so compare the raw addresses as `void *`.
            return (
                lhs.cast_to(Type::void_pointer()),
                rhs.cast_to(Type::void_pointer()),
            );
        }
        (lhs, rhs)
    }

    fn lower_scalar_icmp_value(&self, pred: IntPredicate, lhs: Expr, rhs: Expr) -> Result<Expr> {
        let (lhs, rhs) = self.normalize_icmp_operands(lhs, rhs);
        let value = match pred {
            IntPredicate::EQ => lhs.eq(rhs),
            IntPredicate::NE => lhs.neq(rhs),
            IntPredicate::UGT => lhs.gt(rhs),
            IntPredicate::UGE => lhs.ge(rhs),
            IntPredicate::ULT => lhs.lt(rhs),
            IntPredicate::ULE => lhs.le(rhs),
            IntPredicate::SGT => self.signed_view(lhs)?.gt(self.signed_view(rhs)?),
            IntPredicate::SGE => self.signed_view(lhs)?.ge(self.signed_view(rhs)?),
            IntPredicate::SLT => self.signed_view(lhs)?.lt(self.signed_view(rhs)?),
            IntPredicate::SLE => self.signed_view(lhs)?.le(self.signed_view(rhs)?),
        };
        Ok(value)
    }

    fn lower_vector_icmp_value(
        &self,
        pred: IntPredicate,
        lhs: Expr,
        rhs: Expr,
        result_type: Type,
    ) -> Result<Expr> {
        let lhs_len = lhs
            .typ()
            .len()
            .ok_or_else(|| self.vector_icmp_err("vector icmp expects vector operands"))?;
        let rhs_len = rhs
            .typ()
            .len()
            .ok_or_else(|| self.vector_icmp_err("vector icmp expects vector operands"))?;
        let result_lane_type = result_type
            .base_type()
            .cloned()
            .ok_or_else(|| self.vector_icmp_err("vector icmp expects a vector result"))?;
        let result_len = result_type
            .len()
            .ok_or_else(|| self.vector_icmp_err("vector icmp expects a vector result"))?;
        debug_assert_eq!(lhs_len, rhs_len);
        debug_assert_eq!(lhs_len, result_len);

        let elems = (0..lhs_len)
            .map(|lane| {
                let lhs_lane = self.aggregate_component(lhs.clone(), lane as usize)?;
                let rhs_lane = self.aggregate_component(rhs.clone(), lane as usize)?;
                Ok(self
                    .lower_scalar_icmp_value(pred, lhs_lane, rhs_lane)?
                    .cast_to(result_lane_type.clone()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Expr::vector_expr(result_type, elems))
    }

    fn vector_icmp_err(&self, msg: &str) -> Diagnostic {
        Diagnostic::unsupported_feature(
            UnsupportedFeature::Instruction,
            format!("{msg} in {}", self.function_name),
        )
        .with_symbol(self.function_name.clone())
    }

    fn lower_vector_minmax_value(
        &self,
        callee_name: &str,
        lhs: Expr,
        rhs: Expr,
        result_type: Type,
    ) -> Result<Expr> {
        let lhs_len = lhs.typ().len().ok_or_else(|| {
            self.unsupported_intrinsic(callee_name, "vector min/max expects vector operands")
        })?;
        let rhs_len = rhs.typ().len().ok_or_else(|| {
            self.unsupported_intrinsic(callee_name, "vector min/max expects vector operands")
        })?;
        let result_lane_type = result_type.base_type().cloned().ok_or_else(|| {
            self.unsupported_intrinsic(callee_name, "vector min/max expects a vector result")
        })?;
        if !result_lane_type.is_integer() {
            return Err(self
                .unsupported_intrinsic(callee_name, "vector min/max expects integer lanes")
                .into());
        }
        debug_assert_eq!(lhs_len, rhs_len);
        debug_assert_eq!(Some(lhs_len), result_type.len());

        let is_signed = callee_name.starts_with("llvm.s");
        let is_min = callee_name.starts_with("llvm.umin") || callee_name.starts_with("llvm.smin");
        let elems = (0..lhs_len)
            .map(|lane| {
                let lhs_lane = self.aggregate_component(lhs.clone(), lane as usize)?;
                let rhs_lane = self.aggregate_component(rhs.clone(), lane as usize)?;
                self.minmax_value(is_signed, is_min, lhs_lane, rhs_lane)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Expr::vector_expr(result_type, elems))
    }

    /// Integer `min`/`max` of two scalars, as `llvm.{s,u}{min,max}` define it.
    fn minmax_value(&self, is_signed: bool, is_min: bool, lhs: Expr, rhs: Expr) -> Result<Expr> {
        let cmp = if is_signed {
            let lhs_cmp = self.signed_view(lhs.clone())?;
            let rhs_cmp = self.signed_view(rhs.clone())?;
            if is_min {
                lhs_cmp.lt(rhs_cmp)
            } else {
                lhs_cmp.gt(rhs_cmp)
            }
        } else if is_min {
            lhs.clone().lt(rhs.clone())
        } else {
            lhs.clone().gt(rhs.clone())
        };
        Ok(cmp.ternary(lhs, rhs))
    }

    fn lower_fcmp(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let pred = instruction.get_fcmp_predicate().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("malformed fcmp in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let lhs = self.lower_operand(instruction, 0)?;
        let rhs = self.lower_operand(instruction, 1)?;
        let result = self.declare_instruction_result(instruction)?;
        let unordered = lhs.clone().is_nan().or(rhs.clone().is_nan());
        // LLVM `fcmp` predicates are defined in terms of IEEE ordered/unordered behavior, so NaN
        // handling must stay explicit instead of relying on plain relational operators alone.
        let value = match pred {
            FloatPredicate::PredicateFalse => Expr::bool_false(),
            FloatPredicate::PredicateTrue => Expr::bool_true(),
            FloatPredicate::ORD => unordered.clone().not(),
            FloatPredicate::UNO => unordered.clone(),
            FloatPredicate::OEQ => unordered.clone().not().and(lhs.clone().feq(rhs.clone())),
            FloatPredicate::OGE => unordered.clone().not().and(lhs.clone().ge(rhs.clone())),
            FloatPredicate::OGT => unordered.clone().not().and(lhs.clone().gt(rhs.clone())),
            FloatPredicate::OLE => unordered.clone().not().and(lhs.clone().le(rhs.clone())),
            FloatPredicate::OLT => unordered.clone().not().and(lhs.clone().lt(rhs.clone())),
            FloatPredicate::ONE => unordered.clone().not().and(lhs.clone().fneq(rhs.clone())),
            FloatPredicate::UEQ => unordered.clone().or(lhs.clone().feq(rhs.clone())),
            FloatPredicate::UGE => unordered.clone().or(lhs.clone().ge(rhs.clone())),
            FloatPredicate::UGT => unordered.clone().or(lhs.clone().gt(rhs.clone())),
            FloatPredicate::ULE => unordered.clone().or(lhs.clone().le(rhs.clone())),
            FloatPredicate::ULT => unordered.clone().or(lhs.clone().lt(rhs.clone())),
            FloatPredicate::UNE => unordered.or(lhs.fneq(rhs)),
        };
        body.push(Stmt::assign(
            result.clone(),
            value.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn lower_fneg(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let operand = self.lower_operand(instruction, 0)?;
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            operand.cast_to(result.typ().clone()).neg(),
            loc,
        ));
        Ok(())
    }

    fn lower_alloca(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let count_value = self.operand_value(
            instruction,
            0,
            UnsupportedFeature::Instruction,
            "malformed alloca element count",
        )?;
        let mut allocated = self
            .types
            .lower_basic(instruction.get_allocated_type().map_err(|_| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!("malformed alloca in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
            })?)?;
        self.insert_pending_aggregate_symbols();
        let result_type = self.types.lower_instruction_result(instruction)?;
        let ptr = match count_value {
            BasicValueEnum::IntValue(count) => {
                if let Some(count) = count.get_zero_extended_constant() {
                    if count != 1 {
                        allocated = allocated.array_of(count as usize);
                    }
                    let slot = self.declare_hidden_local("alloca", allocated, loc);
                    if count != 1 {
                        slot.array_to_ptr()
                    } else {
                        slot.address_of()
                    }
                } else {
                    let count_expr = self.lower_operand(instruction, 0)?.cast_to(Type::size_t());
                    // Exact VLA types refer to a named bound symbol, so materialize the runtime
                    // count in a hidden local before constructing the array type.
                    let (size_local, size_name) = self.declare_hidden_local_at_point(
                        body,
                        "alloca_size",
                        Type::size_t(),
                        loc,
                    );
                    body.push(Stmt::assign(size_local.clone(), count_expr, loc));
                    let vla_type = allocated.variable_array_of(size_name);
                    let (slot, _) =
                        self.declare_hidden_local_at_point(body, "alloca", vla_type, loc);
                    if let Some((_, slots)) = self.stack_saves.last_mut() {
                        slots.push(slot.clone());
                    }
                    slot.array_to_ptr()
                }
            }
            _ => {
                return Err(Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!("expected integer operand in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
                .into());
            }
        };
        // Model `%p = alloca T` as a hidden local plus the SSA value for the resulting pointer.
        // `alloca T, 1` returns `&slot`, while counted allocas return a pointer to the first
        // element of the synthesized array object (`&slot[0]`) rather than a pointer to the
        // array itself cast to `T *`. That matches LLVM's result type and gives CBMC the same
        // object/bounds shape it uses for exact VLAs.
        self.values.insert(
            instruction.as_value_ref() as usize,
            ptr.cast_to(result_type).with_location(loc),
        );
        Ok(())
    }

    fn lower_load(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let result = self.declare_instruction_result(instruction)?;
        let ptr = self.lower_operand(instruction, 0)?;
        if let Some(slot) = self.direct_load_pointee(&ptr, result.typ()) {
            body.push(Stmt::assign(
                result.clone(),
                slot.cast_to(result.typ().clone()),
                loc,
            ));
            return Ok(());
        }
        body.push(Stmt::assign(
            result.clone(),
            ptr.cast_to(result.typ().clone().to_pointer()).dereference(),
            loc,
        ));
        Ok(())
    }

    fn lower_store(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let stored = self.operand_value(
            instruction,
            0,
            UnsupportedFeature::Instruction,
            "malformed store value",
        )?;
        let value = self.lower_value(stored)?;
        let ptr = self.lower_operand(instruction, 1)?;
        let stored_type = self.types.lower_basic(stored.get_type())?;
        self.insert_pending_aggregate_symbols();
        if let Some(slot) = self.direct_store_slot(&ptr, &stored_type) {
            body.push(Stmt::assign(slot, value.cast_to(stored_type), loc));
            return Ok(());
        }
        body.push(Stmt::assign(
            ptr.cast_to(stored_type.clone().to_pointer()).dereference(),
            value.cast_to(stored_type),
            loc,
        ));
        Ok(())
    }

    fn lower_gep(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let base = self.lower_operand(instruction, 0)?;
        let source_type =
            self.types
                .lower_basic(instruction.get_gep_source_element_type().map_err(|_| {
                    Diagnostic::unsupported_feature(
                        UnsupportedFeature::Instruction,
                        format!("malformed getelementptr in {}", self.function_name),
                    )
                    .with_symbol(self.function_name.clone())
                })?)?;
        self.insert_pending_aggregate_symbols();

        let ptr = self.compute_gep_address(instruction, base, source_type)?;
        let result_type = self.types.lower_instruction_result(instruction)?;
        if self.is_direct_address_expr(&ptr) {
            // Preserve direct address expressions as SSA values so later loads and stores can
            // keep array/member structure instead of falling back to generic pointer arithmetic.
            self.values.insert(
                instruction.as_value_ref() as usize,
                ptr.cast_to(result_type).with_location(loc),
            );
            return Ok(());
        }
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            ptr.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn lower_cast(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let value = self.lower_operand(instruction, 0)?;
        let result = self.declare_instruction_result(instruction)?;
        let cast = match instruction.get_opcode() {
            // LLVM signed extension is not a plain type-width change in our normalized
            // unsigned-bitvector world, so reinterpret the source as signed first.
            InstructionOpcode::SExt => self
                .signed_view(value)?
                .cast_to(
                    result
                        .typ()
                        .to_signed()
                        .unwrap_or_else(|| result.typ().clone()),
                )
                .cast_to(result.typ().clone()),
            // `i1` lowers to 8-bit `c_bool`, so truncate through a 1-bit bitvector first to
            // extract the least-significant bit instead of the low byte.
            InstructionOpcode::Trunc if result.typ().is_c_bool() => value
                .cast_to(Type::unsigned_int(1_u32))
                .cast_to(result.typ().clone()),
            InstructionOpcode::ZExt
            | InstructionOpcode::Trunc
            | InstructionOpcode::PtrToInt
            | InstructionOpcode::IntToPtr
            | InstructionOpcode::AddrSpaceCast
            | InstructionOpcode::FPExt
            | InstructionOpcode::FPTrunc
            | InstructionOpcode::FPToSI
            | InstructionOpcode::FPToUI
            | InstructionOpcode::SIToFP
            | InstructionOpcode::UIToFP => value.cast_to(result.typ().clone()),
            InstructionOpcode::BitCast => {
                self.lower_bitcast_value(instruction, value, result.typ().clone())?
            }
            _ => unreachable!("cast opcode dispatched incorrectly"),
        };
        body.push(Stmt::assign(result, cast, loc));
        Ok(())
    }

    fn lower_bitcast_value(
        &self,
        instruction: InstructionValue<'ctx>,
        value: Expr,
        result_type: Type,
    ) -> Result<Expr> {
        if value.typ().is_vector() && result_type.is_integer() {
            // Bitcast width is defined by the original LLVM types, not the lowered GOTO types.
            // In particular, LLVM `i1` currently lowers to `c_bool`, which would otherwise hide
            // the single-bit lane width needed for `<N x i1> -> iN` masks.
            let source_value = self.operand_value(
                instruction,
                0,
                UnsupportedFeature::Instruction,
                "malformed bitcast source",
            )?;
            let (lane_count, lane_bits) = self.llvm_fixed_vector_layout(source_value.get_type())?;
            let result_bits = self.llvm_scalar_bit_width(instruction.get_type())?;
            return self.lower_vector_to_scalar_bitcast(
                value,
                result_type,
                lane_count,
                lane_bits,
                result_bits,
            );
        }
        Ok(value.cast_to(result_type))
    }

    fn lower_vector_to_scalar_bitcast(
        &self,
        value: Expr,
        result_type: Type,
        lane_count: u64,
        lane_bits: u64,
        result_bits: u64,
    ) -> Result<Expr> {
        let lowered_lane_count = value.typ().len().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "vector bitcast expects a vector source in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let total_bits = lane_count.checked_mul(lane_bits).ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("vector bitcast width overflow in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        debug_assert_eq!(lowered_lane_count, lane_count);
        if total_bits != result_bits {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "vector bitcast requires equal source/result bit widths in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }

        // LLVM bitcast preserves lane bit patterns. For the little-endian machine models we
        // currently target, lane 0 occupies the least-significant bits of the scalar mask.
        let mut packed = Expr::int_constant(0, result_type.clone());
        for lane in 0..lane_count {
            let shift = lane * lane_bits;
            let lane_expr = self
                .aggregate_component(value.clone(), lane as usize)?
                .cast_to(result_type.clone());
            let shifted = if shift == 0 {
                lane_expr
            } else {
                lane_expr.shl(Expr::int_constant(shift, result_type.clone()))
            };
            packed = packed.bitor(shifted);
        }
        Ok(packed)
    }

    fn llvm_fixed_vector_layout(&self, typ: BasicTypeEnum<'ctx>) -> Result<(u64, u64)> {
        let AnyTypeEnum::VectorType(vector) = typ.as_any_type_enum() else {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("expected fixed vector type in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .into());
        };
        Ok((
            vector.get_size().into(),
            self.llvm_scalar_bit_width(vector.get_element_type().as_any_type_enum())?,
        ))
    }

    fn llvm_scalar_bit_width(&self, typ: AnyTypeEnum<'ctx>) -> Result<u64> {
        match typ {
            AnyTypeEnum::IntType(int) => Ok(int.get_bit_width().into()),
            _ => Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "unsupported LLVM bitcast width query in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .into()),
        }
    }

    fn lower_select(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let cond = self.lower_operand(instruction, 0)?.cast_to(Type::bool());
        let result = self.declare_instruction_result(instruction)?;
        let then_value = self
            .lower_operand(instruction, 1)?
            .cast_to(result.typ().clone());
        let else_value = self
            .lower_operand(instruction, 2)?
            .cast_to(result.typ().clone());
        body.push(Stmt::assign(
            result,
            cond.ternary(then_value, else_value),
            loc,
        ));
        Ok(())
    }

    fn lower_freeze(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let value = self.lower_operand(instruction, 0)?;
        // `freeze` only differs from identity on poison/undef inputs. The current translator does
        // not model LLVM poison explicitly, so preserve all defined executions by reusing the
        // operand as-is.
        self.assign_result(instruction, body, value)
    }

    fn lower_return(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        // Opaque-pointer values can pick up more specific pointee hints than the function
        // signature at individual return sites. Cast them back to the declared return
        // type so all exits use one CBMC pointer sort.
        let return_type = self.lowered_return_type();
        let value = (instruction.get_num_operands() != 0)
            .then(|| self.lower_operand(instruction, 0))
            .transpose()?
            .map(|value| value.cast_to(return_type));
        body.push(Stmt::ret(value, loc));
        Ok(())
    }

    fn lower_call(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let call = CallSiteValue::try_from(instruction).map_err(|_| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("malformed call instruction in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
        })?;

        let raw_callee = unsafe { LLVMGetCalledValue(call.as_value_ref()) };
        if self.try_lower_inline_asm_call(instruction, raw_callee, body)? {
            return Ok(());
        }
        if let Some(callee) = unsafe { FunctionValue::new(raw_callee) } {
            let callee_name = callee.get_name().to_string_lossy().into_owned();
            if super::is_metadata_noop_intrinsic(&callee_name) {
                // LLVM debug and alias-scope intrinsics carry metadata-only bookkeeping. Drop
                // them before lowering operands so metadata or `poison` arguments never enter
                // the semantic translation path.
                return Ok(());
            }
        }
        if let Some(callee) = call.get_called_fn_value() {
            let callee_name = callee.get_name().to_string_lossy().into_owned();
            let args = (0..call.count_arguments())
                .map(|index| self.lower_operand(instruction, index))
                .collect::<Result<Vec<_>>>()?;
            let replacement = self.replacement_symbol(&callee_name);
            if replacement.is_none()
                && self.try_lower_special_call(instruction, &callee_name, &args, body)?
            {
                self.report.resolved_function(&callee_name);
                return Ok(());
            }

            let contract = (!self.suppress_contracts)
                .then(|| self.contracts.function(callee_name.as_str()))
                .flatten()
                .cloned();
            if contract.is_some() {
                self.report
                    .relied_contract(&callee_name, replacement.is_some());
            }
            if let Some(contract) = &contract
                && replacement.is_none()
            {
                self.lower_requires_helper(contract, &callee_name, &args, body, loc)?;
            }
            let snapshot = if let Some(contract) = &contract
                && replacement.is_none()
            {
                self.lower_snapshot_helper(contract, &callee_name, &args, body, loc)?
            } else {
                None
            };

            let effective_callee = replacement.unwrap_or_else(|| {
                contract
                    .as_ref()
                    .and_then(|contract| contract.model.as_deref())
                    .unwrap_or(callee_name.as_str())
            });
            let function_expr = self.function_expr(effective_callee, loc)?;
            let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
                &function_expr,
                args.clone(),
                self.symbols.machine_model(),
            );
            self.emit_call_result(instruction, function_expr, arguments, body, loc)?;
            if let Some(contract) = &contract
                && replacement.is_none()
            {
                self.lower_ensures_helper(
                    contract,
                    &callee_name,
                    &args,
                    snapshot,
                    instruction,
                    body,
                    loc,
                )?;
            }
            return Ok(());
        }

        let args = (0..call.count_arguments())
            .map(|index| self.lower_operand(instruction, index))
            .collect::<Result<Vec<_>>>()?;
        let function_expr = self.indirect_callee_expr(call, loc)?;
        let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &function_expr,
            args,
            self.symbols.machine_model(),
        );
        self.emit_call_result(instruction, function_expr, arguments, body, loc)?;
        Ok(())
    }

    fn lower_invoke(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let predecessor = self.current_block.ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!(
                    "invoke lowered without current block in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
        })?;
        let normal_dest = unsafe { BasicBlock::new(LLVMGetNormalDest(instruction.as_value_ref())) }
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::ControlFlow,
                    format!("malformed invoke normal edge in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
                .with_location(loc)
            })?;
        let unwind_dest = unsafe { BasicBlock::new(LLVMGetUnwindDest(instruction.as_value_ref())) }
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::ControlFlow,
                    format!("malformed invoke unwind edge in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
                .with_location(loc)
            })?;
        let args = self.invoke_args(instruction)?;
        let raw_callee = unsafe { LLVMGetCalledValue(instruction.as_value_ref()) };
        if self.try_lower_inline_asm_call(instruction, raw_callee, body)? {
            body.push(self.branch_to(predecessor, normal_dest)?);
            return Ok(());
        }

        if let Some(callee) = unsafe { FunctionValue::new(raw_callee) } {
            let callee_name = callee.get_name().to_string_lossy().into_owned();
            let replacement = self.replacement_symbol(&callee_name);
            if replacement.is_none()
                && self.try_lower_special_call(instruction, &callee_name, &args, body)?
            {
                self.report.resolved_function(&callee_name);
                body.push(self.branch_on_exception(predecessor, normal_dest, unwind_dest)?);
                return Ok(());
            }

            let contract = (!self.suppress_contracts)
                .then(|| self.contracts.function(callee_name.as_str()))
                .flatten()
                .cloned();
            if contract.is_some() {
                self.report.relied_contract(&callee_name, false);
            }
            // `validate_contract_replacement_uses` rejects these before lowering; stay
            // fail-closed here rather than silently ignoring the requested replacement.
            if replacement.is_some() {
                return Err(Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    super::invoke_replacement_message(&callee_name),
                )
                .with_symbol(callee_name)
                .with_location(loc)
                .into());
            }
            if let Some(contract) = &contract {
                self.lower_requires_helper(contract, &callee_name, &args, body, loc)?;
            }
            let snapshot = if let Some(contract) = &contract {
                self.lower_snapshot_helper(contract, &callee_name, &args, body, loc)?
            } else {
                None
            };

            let effective_callee = contract
                .as_ref()
                .and_then(|contract| contract.model.as_deref())
                .unwrap_or(callee_name.as_str());
            let function_expr = self.function_expr(effective_callee, loc)?;
            let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
                &function_expr,
                args.clone(),
                self.symbols.machine_model(),
            );
            self.emit_call_result(instruction, function_expr, arguments, body, loc)?;

            let mut normal_stmts = Vec::new();
            if let Some(contract) = &contract {
                self.lower_ensures_helper(
                    contract,
                    &callee_name,
                    &args,
                    snapshot,
                    instruction,
                    &mut normal_stmts,
                    loc,
                )?;
            }
            body.push(self.branch_on_exception_with_normal_stmts(
                predecessor,
                normal_dest,
                unwind_dest,
                normal_stmts,
            )?);
            return Ok(());
        }

        let function_expr =
            self.indirect_callee_expr_from_instruction(instruction, raw_callee, loc)?;
        let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &function_expr,
            args,
            self.symbols.machine_model(),
        );
        self.emit_call_result(instruction, function_expr, arguments, body, loc)?;
        body.push(self.branch_on_exception(predecessor, normal_dest, unwind_dest)?);
        Ok(())
    }

    fn lower_callbr(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let predecessor = self.current_block.ok_or_else(|| {
            self.unsupported_control_flow("callbr lowered without current block", loc)
        })?;
        let raw_instruction = instruction.as_value_ref();
        let raw_callee = unsafe { LLVMGetCalledValue(raw_instruction) };
        if unsafe { LLVMIsAInlineAsm(raw_callee) }.is_null() {
            return Err(self
                .unsupported_control_flow("unsupported non-assembly callbr", loc)
                .into());
        }

        // A callbr's successors are its fallthrough followed by its indirect destinations, so
        // the generic terminator API enumerates exactly the edges this lowering must cover.
        let successor_count = unsafe { LLVMGetNumSuccessors(raw_instruction) };
        if successor_count < 2 {
            return Err(self
                .unsupported_control_flow(
                    "callbr without an indirect destination is unsupported",
                    loc,
                )
                .into());
        }
        if unsafe { LLVMGetNumOperandBundles(raw_instruction) } != 0 {
            return Err(self
                .unsupported_control_flow("callbr operand bundles are unsupported", loc)
                .into());
        }
        // Arguments, one label per successor, and the callee plus fallthrough operands. Anything
        // else means an operand shape this lowering does not understand.
        let expected_operands = unsafe { LLVMGetNumArgOperands(raw_instruction) }
            .checked_add(successor_count)
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| self.unsupported_control_flow("callbr operand count overflow", loc))?;
        if instruction.get_num_operands() != expected_operands {
            return Err(self
                .unsupported_control_flow("unexpected callbr operand count", loc)
                .into());
        }

        let successors = (0..successor_count)
            .map(|index| {
                unsafe { BasicBlock::new(LLVMGetSuccessor(raw_instruction, index)) }
                    .ok_or_else(|| self.unsupported_control_flow("malformed callbr successor", loc))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Unlike an ordinary call, even empty asm in a callbr may select an indirect edge.
        // Require an exact compiled model for its state effects before conservatively allowing
        // every destination declared by the LLVM instruction.
        let metadata = super::asm_manifest::decode_inline_asm(raw_callee)?;
        self.lower_inline_asm_model(instruction, &metadata, body, loc)?;
        let branches = successors
            .into_iter()
            .map(|successor| self.branch_to(predecessor, successor))
            .collect::<Result<Vec<_>>>()?;
        body.push(self.nondeterministic_branches(branches, loc)?);
        Ok(())
    }

    fn lower_landingpad(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        self.validate_landingpad_shape(instruction)?;
        let result = self.declare_instruction_result(instruction)?;
        let fields = result
            .typ()
            .lookup_components(self.symbols)
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::ControlFlow,
                    format!(
                        "landingpad result is not a lowered aggregate in {}",
                        self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
                .with_location(loc)
            })?;
        if fields.len() < 2 {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!(
                    "landingpad result shape is unsupported in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
            .into());
        }
        // The MVP EH model tracks one in-flight exception record. `landingpad` simply snapshots
        // that record into LLVM's `{ptr, i32}` pair without modeling full personality dispatch.
        let exn = self.eh_ptr_expr(loc)?.cast_to(fields[0].typ().clone());
        let selector = self.eh_selector_expr(loc)?.cast_to(fields[1].typ().clone());
        let result_expr = Expr::struct_expr_from_values(
            result.typ().clone(),
            vec![exn.clone(), selector.clone()],
            self.symbols,
        );
        body.push(Stmt::assign(result.clone(), result_expr, loc));
        self.aggregates
            .insert(instruction.as_value_ref() as usize, vec![exn, selector]);
        Ok(())
    }

    fn lower_resume(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let exception = self.lower_operand(instruction, 0)?;
        let exn = self.project_aggregate_value(exception.clone(), &[0])?;
        let selector = self.project_aggregate_value(exception, &[1])?;
        self.raise_exception(exn, selector, body, loc)?;
        body.push(self.exceptional_return(loc)?);
        Ok(())
    }

    fn try_lower_inline_asm_call(
        &mut self,
        instruction: InstructionValue<'ctx>,
        raw_callee: LLVMValueRef,
        body: &mut Vec<Stmt>,
    ) -> Result<bool> {
        if unsafe { LLVMIsAInlineAsm(raw_callee) }.is_null() {
            return Ok(false);
        }

        let metadata = super::asm_manifest::decode_inline_asm(raw_callee)?;
        if super::asm_manifest::inline_asm_is_empty(&metadata.asm) && !metadata.can_unwind {
            // Empty inline asm, including comment-only compiler barriers, is not a stateful
            // machine instruction. Lowering it away keeps verifier state unchanged without
            // pretending to understand real target assembly.
            if instruction.get_type().is_void_type() {
                return Ok(true);
            }
            if unsafe { LLVMGetNumArgOperands(instruction.as_value_ref()) } == 1 {
                let value = self.lower_operand(instruction, 0)?;
                self.assign_result(instruction, body, value)?;
                return Ok(true);
            }
        }

        let loc = self.instruction_location(instruction);
        self.lower_inline_asm_model(instruction, &metadata, body, loc)?;
        Ok(true)
    }

    /// Redirect one inline-assembly site to its compiled model, or fail closed.
    fn lower_inline_asm_model(
        &mut self,
        instruction: InstructionValue<'ctx>,
        metadata: &super::asm_manifest::InlineAsmMetadata,
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<()> {
        // Skip the lookup and site construction when no rules are configured, which is the
        // common case and keeps unmodelled assembly on the direct fail-closed path. The metadata
        // itself is decoded once by the caller and shared with manifest generation.
        let function_name = self.function_name.clone();
        let site = self
            .contracts
            .has_inline_asm_models()
            .then(|| InlineAsmSite {
                asm: &metadata.asm,
                constraints: &metadata.constraints,
                dialect: metadata.dialect,
                side_effects: metadata.side_effects,
                align_stack: metadata.align_stack,
                function: &function_name,
            });
        if let Some(site) = &site
            && let Some(mapping) = self.contracts.inline_asm_model(site)
        {
            if mapping.model == self.function_name {
                anyhow::bail!(
                    "inline asm model `{}` recursively matches assembly in its own body",
                    mapping.model
                );
            }
            if metadata.can_unwind {
                return Err(Diagnostic::unsupported_feature(
                    UnsupportedFeature::InlineAsm,
                    format!(
                        "inline asm model `{}` cannot replace unwind-capable assembly in {}",
                        mapping.model, self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
                .with_location(loc)
                .into());
            }
            let arguments = (0..unsafe { LLVMGetNumArgOperands(instruction.as_value_ref()) })
                .map(|index| self.lower_operand(instruction, index))
                .collect::<Result<Vec<_>>>()?;
            let function = self.function_expr(&mapping.model, loc)?;
            self.validate_inline_asm_model_signature(
                instruction,
                &mapping.model,
                &function,
                &arguments,
            )?;
            let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
                &function,
                arguments,
                self.symbols.machine_model(),
            );
            self.emit_call_result(instruction, function, arguments, body, loc)?;
            self.report.inline_asm_model(site, &mapping.model);
            return Ok(());
        }

        // Real target assembly may read or mutate memory through constraints and clobbers that
        // are not represented by its LLVM return value. Dropping it is therefore unsound even
        // when the caller permits selected approximations with `--unsound`; always fail closed.
        Err(Diagnostic::unsupported_feature(
            UnsupportedFeature::InlineAsm,
            format!(
                "unsupported inline asm {} in {}",
                match instruction.get_opcode() {
                    InstructionOpcode::CallBr => "callbr",
                    _ => "call",
                },
                self.function_name
            ),
        )
        .with_symbol(self.function_name.clone())
        .with_location(loc)
        .into())
    }

    fn nondeterministic_branches(&self, mut branches: Vec<Stmt>, loc: Location) -> Result<Stmt> {
        let mut branch = branches
            .pop()
            .ok_or_else(|| self.unsupported_control_flow("callbr has no successors", loc))?;
        while let Some(candidate) = branches.pop() {
            branch = Stmt::if_then_else(Expr::nondet(Type::bool()), candidate, Some(branch), loc);
        }
        Ok(branch)
    }

    fn emit_call_result(
        &mut self,
        instruction: InstructionValue<'ctx>,
        function: Expr,
        arguments: Vec<Expr>,
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<()> {
        if instruction.get_type().is_void_type() {
            body.push(Stmt::function_call(None, function, arguments, loc));
            return Ok(());
        }

        let result = self.declare_instruction_result(instruction)?;
        let return_type = function.typ().return_type().cloned().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "callee in {} does not have a return type",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .with_location(loc)
        })?;
        if result.typ() == &return_type {
            body.push(Stmt::function_call(Some(result), function, arguments, loc));
            return Ok(());
        }

        // Preseeded CBMC/library symbols often use C types like `c_int`, while LLVM SSA results
        // are signless integers such as `i32`. Call through the callee-typed temp, then cast once.
        let tmp = self.declare_hidden_local("call_ret", return_type.clone(), loc);
        body.push(Stmt::function_call(
            Some(tmp.clone()),
            function,
            arguments,
            loc,
        ));
        body.push(Stmt::assign(
            result.clone(),
            tmp.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn invoke_args(&mut self, instruction: InstructionValue<'ctx>) -> Result<Vec<Expr>> {
        (0..unsafe { LLVMGetNumArgOperands(instruction.as_value_ref()) })
            .map(|index| self.lower_operand(instruction, index))
            .collect()
    }

    fn indirect_callee_expr(
        &mut self,
        call: CallSiteValue<'ctx>,
        location: Location,
    ) -> Result<Expr> {
        let raw = unsafe { LLVMGetCalledValue(call.as_value_ref()) };
        let callee = self.lower_value(unsafe { BasicValueEnum::new(raw) })?;
        let function_type = self.types.lower_function(call.get_called_fn_type())?;
        // LLVM spells indirect callees as pointers to functions; CBMC wants the callee expression
        // itself to have code type, so we cast to the recovered code-pointer type and dereference.
        Ok(callee
            .cast_to(function_type.clone().to_pointer())
            .dereference()
            .with_location(location))
    }

    fn indirect_callee_expr_from_instruction(
        &mut self,
        instruction: InstructionValue<'ctx>,
        raw: LLVMValueRef,
        location: Location,
    ) -> Result<Expr> {
        let callee = self.lower_value(unsafe { BasicValueEnum::new(raw) })?;
        let function_type = self.types.lower_function(unsafe {
            FunctionType::new(LLVMGetCalledFunctionType(instruction.as_value_ref()))
        })?;
        Ok(callee
            .cast_to(function_type.to_pointer())
            .dereference()
            .with_location(location))
    }

    fn branch_on_exception(
        &mut self,
        predecessor: BasicBlock<'ctx>,
        normal_dest: BasicBlock<'ctx>,
        unwind_dest: BasicBlock<'ctx>,
    ) -> Result<Stmt> {
        self.branch_on_exception_with_normal_stmts(
            predecessor,
            normal_dest,
            unwind_dest,
            Vec::new(),
        )
    }

    fn branch_on_exception_with_normal_stmts(
        &mut self,
        predecessor: BasicBlock<'ctx>,
        normal_dest: BasicBlock<'ctx>,
        unwind_dest: BasicBlock<'ctx>,
        normal_stmts: Vec<Stmt>,
    ) -> Result<Stmt> {
        let loc = self.block_location(normal_dest);
        let active = self.eh_active_expr(loc)?.cast_to(Type::bool());
        let mut stmts = Vec::with_capacity(normal_stmts.len() + 2);
        // EH dispatch mirrors the source structure directly: unwind when an exception is pending,
        // otherwise continue through the normal edge and any post-call contract checks.
        stmts.push(Stmt::if_then_else(
            active,
            self.branch_to(predecessor, unwind_dest)?,
            None,
            loc,
        ));
        stmts.push(self.branch_to_with_stmts(predecessor, normal_dest, normal_stmts)?);
        Ok(Stmt::block(stmts, loc))
    }

    fn lower_requires_helper(
        &mut self,
        contract: &FunctionContract,
        callee_name: &str,
        args: &[Expr],
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<()> {
        let Some(helper) = contract.requires.as_deref() else {
            return Ok(());
        };
        let cond =
            self.call_contract_helper(helper, args.to_vec(), body, "requires", loc, callee_name)?;
        body.push(Stmt::assume(cond.cast_to(Type::bool()), loc));
        Ok(())
    }

    fn lower_ensures_helper(
        &mut self,
        contract: &FunctionContract,
        callee_name: &str,
        args: &[Expr],
        snapshot: Option<Expr>,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<()> {
        let Some(helper) = contract.ensures.as_deref() else {
            return Ok(());
        };
        let result = (!instruction.get_type().is_void_type())
            .then(|| self.declare_instruction_result(instruction))
            .transpose()?;
        let helper_args =
            crate::contracts::inject::ensures_arguments(args.to_vec(), snapshot, result);
        let cond =
            self.call_contract_helper(helper, helper_args, body, "ensures", loc, callee_name)?;
        body.push(Stmt::assert(
            cond.cast_to(Type::bool()),
            "contract",
            &format!("ensures for {callee_name}"),
            loc,
        ));
        Ok(())
    }

    /// Capture helper-defined pre-state after `requires` and immediately before the target call.
    /// The returned value is kept in a caller local and later passed to `ensures` after the target
    /// parameters and before the target result.
    fn lower_snapshot_helper(
        &mut self,
        contract: &FunctionContract,
        callee_name: &str,
        args: &[Expr],
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<Option<Expr>> {
        let Some(helper) = contract.snapshot.as_deref() else {
            return Ok(None);
        };
        self.call_contract_helper(helper, args.to_vec(), body, "snapshot", loc, callee_name)
            .map(Some)
    }

    fn call_contract_helper(
        &mut self,
        helper: &str,
        args: Vec<Expr>,
        body: &mut Vec<Stmt>,
        kind: &str,
        loc: Location,
        callee_name: &str,
    ) -> Result<Expr> {
        let function_expr = self.function_expr(helper, loc)?;
        self.validate_helper_arity(helper, function_expr.typ(), args.len(), kind, callee_name)?;
        let result_type = function_expr
            .typ()
            .return_type()
            .cloned()
            .unwrap_or_else(Type::bool);
        if result_type == Type::empty() {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("{kind} helper `{helper}` for `{callee_name}` must return a value"),
            )
            .with_symbol(helper.to_owned())
            .into());
        }
        let result = self.declare_hidden_local(
            &format!("{kind}_{}", util::sanitize_name(callee_name)),
            result_type,
            loc,
        );
        let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &function_expr,
            args,
            self.symbols.machine_model(),
        );
        body.push(Stmt::function_call(
            Some(result.clone()),
            function_expr,
            arguments,
            loc,
        ));
        Ok(result)
    }

    fn validate_helper_arity(
        &self,
        helper: &str,
        helper_type: &Type,
        expected: usize,
        kind: &str,
        callee_name: &str,
    ) -> Result<()> {
        let actual = helper_type.parameters().map_or(0, Vec::len);
        if actual == expected {
            return Ok(());
        }
        Err(Diagnostic::unsupported_feature(
            UnsupportedFeature::Instruction,
            format!(
                "{kind} helper `{helper}` for `{callee_name}` expects {actual} args, got {expected}"
            ),
        )
        .with_symbol(helper.to_owned())
        .into())
    }

    fn try_lower_special_call(
        &mut self,
        instruction: InstructionValue<'ctx>,
        callee_name: &str,
        args: &[Expr],
        body: &mut Vec<Stmt>,
    ) -> Result<bool> {
        let loc = self.instruction_location(instruction);
        if callee_name == "__cxa_throw" {
            let exn = args.first().cloned().ok_or_else(|| {
                self.unsupported_intrinsic(callee_name, "missing __cxa_throw exception object")
            })?;
            self.raise_exception(exn, Expr::int_constant(1, Type::c_int()), body, loc)?;
            body.push(self.exceptional_return(loc)?);
            return Ok(true);
        }

        if callee_name == "__cxa_allocate_exception" {
            self.lower_builtin_call(
                BuiltinFn::Malloc,
                args.get(0..1).unwrap_or(&[]),
                instruction,
                body,
            )?;
            let result = self.declare_instruction_result(instruction)?;
            // The supported EH subset assumes exception allocation succeeds; a null result would
            // model `throw` as ordinary invalid-pointer execution instead of unwinding.
            body.push(Stmt::assume(
                result
                    .clone()
                    .cast_to(Type::void_pointer())
                    .neq(Type::void_pointer().null()),
                loc,
            ));
            return Ok(true);
        }

        if callee_name == "__cxa_begin_catch" {
            let exn = self.eh_ptr_expr(loc)?;
            self.assign_result(instruction, body, exn)?;
            return Ok(true);
        }

        if callee_name == "__cxa_end_catch" {
            self.clear_exception(body, loc)?;
            return Ok(true);
        }

        if callee_name == "__CPROVER_assume" || callee_name.starts_with("llvm.assume") {
            let cond = args
                .first()
                .cloned()
                .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing assume argument"))?
                .cast_to(Type::bool());
            body.push(Stmt::assume(cond, loc));
            return Ok(true);
        }

        if callee_name == "__CPROVER_assert" {
            let cond = args
                .first()
                .cloned()
                .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing assert argument"))?
                .cast_to(Type::bool());
            // Preserve the user's description so CBMC property reports stay meaningful; fall
            // back to the callee name when the message is not a constant string literal.
            let message = self
                .constant_string_operand(instruction, 1)
                .unwrap_or_else(|| callee_name.to_owned());
            body.push(Stmt::assert(cond, "assert", &message, loc));
            return Ok(true);
        }

        if callee_name.starts_with("__CPROVER_nondet_") {
            let lhs = self.declare_instruction_result(instruction)?;
            body.push(Stmt::assign(
                lhs.clone(),
                util::nondet_expr(lhs.typ().clone()),
                loc,
            ));
            return Ok(true);
        }

        if callee_name.starts_with("llvm.expect") {
            let value = args.first().cloned().ok_or_else(|| {
                self.unsupported_intrinsic(callee_name, "missing expect argument")
            })?;
            self.assign_result(instruction, body, value)?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.ctpop") {
            let value = args
                .first()
                .cloned()
                .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing ctpop argument"))?
                .popcount();
            self.assign_result(instruction, body, value)?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.cttz") {
            let value = args
                .first()
                .cloned()
                .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing cttz argument"))?
                .cttz(self.intrinsic_allow_zero(callee_name, args, "cttz")?);
            self.assign_result(instruction, body, value)?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.is.constant") {
            let value = instruction
                .get_operand(0)
                .and_then(|operand| operand.value())
                .ok_or_else(|| {
                    self.unsupported_intrinsic(callee_name, "missing is.constant argument")
                })?;
            // The intrinsic asks about LLVM's notion of manifest const-ness, not whether the
            // lowered GOTO expression happened to fold to a literal after translation.
            self.assign_result(instruction, body, Expr::bool_constant(value.is_const()))?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.bswap") {
            let value = args
                .first()
                .cloned()
                .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing bswap argument"))?
                .bswap();
            self.assign_result(instruction, body, value)?;
            return Ok(true);
        }

        if let Some(is_left) = if callee_name.starts_with("llvm.fshl") {
            Some(true)
        } else if callee_name.starts_with("llvm.fshr") {
            Some(false)
        } else {
            None
        } {
            self.lower_funnel_shift_intrinsic(is_left, callee_name, instruction, args, body)?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.umin")
            || callee_name.starts_with("llvm.umax")
            || callee_name.starts_with("llvm.smin")
            || callee_name.starts_with("llvm.smax")
        {
            self.lower_minmax_intrinsic(callee_name, instruction, args, body)?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.vector.reduce.") {
            self.lower_vector_reduce_intrinsic(callee_name, instruction, args, body)?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.uadd.sat")
            || callee_name.starts_with("llvm.usub.sat")
            || callee_name.starts_with("llvm.sadd.sat")
            || callee_name.starts_with("llvm.ssub.sat")
        {
            self.lower_saturating_intrinsic(callee_name, instruction, args, body)?;
            return Ok(true);
        }

        if let Some((double_fn, float_fn)) = self.float_builtin(callee_name) {
            self.lower_float_builtin_call(
                callee_name,
                args,
                instruction,
                body,
                double_fn,
                float_fn,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.sadd.with.overflow")
            || callee_name.starts_with("llvm.uadd.with.overflow")
        {
            self.lower_overflow_intrinsic(
                instruction,
                args,
                body,
                Expr::add_overflow_result,
                callee_name.starts_with("llvm.s"),
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.ssub.with.overflow")
            || callee_name.starts_with("llvm.usub.with.overflow")
        {
            self.lower_overflow_intrinsic(
                instruction,
                args,
                body,
                Expr::sub_overflow_result,
                callee_name.starts_with("llvm.s"),
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.smul.with.overflow")
            || callee_name.starts_with("llvm.umul.with.overflow")
        {
            self.lower_overflow_intrinsic(
                instruction,
                args,
                body,
                Expr::mul_overflow_result,
                callee_name.starts_with("llvm.s"),
            )?;
            return Ok(true);
        }

        if matches!(callee_name, "llvm.trap" | "llvm.debugtrap") {
            body.push(Stmt::assert_false("trap", callee_name, loc));
            return Ok(true);
        }

        if is_rust_panic_symbol(callee_name) {
            body.push(Stmt::assert_false("panic", "Rust panic", loc));
            return Ok(true);
        }

        if callee_name.starts_with("llvm.memcmp") {
            self.lower_builtin_call(
                BuiltinFn::Memcmp,
                args.get(0..3).unwrap_or(&[]),
                instruction,
                body,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.memcpy") {
            if self.is_zero_length_memory_intrinsic(args) {
                return Ok(true);
            }
            // LLVM allows `src == dst` (clang emits it for `*p = *p;`), while C `memcpy` forbids
            // any overlap. Guard the call so the equal case is the no-op LLVM promises, without
            // lowering to `memmove`, which would also accept the partial overlap LLVM forbids.
            let [dst, src, _, ..] = args else {
                return Err(self
                    .unsupported_intrinsic(callee_name, "missing memcpy operands")
                    .with_location(loc)
                    .into());
            };
            let (dst, src) = self.normalize_icmp_operands(dst.clone(), src.clone());
            let mut call = vec![];
            self.lower_builtin_call(BuiltinFn::Memcpy, &args[..3], instruction, &mut call)?;
            body.push(Stmt::if_then_else(
                dst.neq(src),
                Stmt::block(call, loc),
                None,
                loc,
            ));
            return Ok(true);
        }

        if callee_name.starts_with("llvm.memmove") || callee_name == "memmove" {
            // Keep plain libc `memmove` on the normal builtin path; this zero-length short-circuit
            // is only for LLVM intrinsics, where null pointers are allowed when `len == 0`.
            if callee_name.starts_with("llvm.memmove") && self.is_zero_length_memory_intrinsic(args)
            {
                return Ok(true);
            }
            self.lower_builtin_call(
                BuiltinFn::Memmove,
                args.get(0..3).unwrap_or(&[]),
                instruction,
                body,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.memset") {
            if self.is_zero_length_memory_intrinsic(args) {
                return Ok(true);
            }
            self.lower_builtin_call(
                BuiltinFn::Memset,
                args.get(0..3).unwrap_or(&[]),
                instruction,
                body,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.stacksave") {
            let key = instruction.as_value_ref() as usize;
            self.stack_saves.push((key, vec![]));
            // Opaque scope token used only by stackrestore; null keeps the SSA value defined.
            self.assign_result(instruction, body, Type::void_pointer().null())?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.stackrestore") {
            let save_key = instruction
                .get_operand(0)
                .and_then(|op| op.value())
                .map(|v| v.as_value_ref() as usize);
            if let Some(key) = save_key {
                // rposition handles non-LIFO restores; for well-nested scopes it's always last.
                if let Some(pos) = self.stack_saves.iter().rposition(|(k, _)| *k == key) {
                    for slot in self.stack_saves.drain(pos..).flat_map(|(_, slots)| slots) {
                        body.push(Stmt::dead(slot, loc));
                    }
                }
            }
            return Ok(true);
        }

        if callee_name.starts_with("llvm.lifetime.start")
            || callee_name.starts_with("llvm.lifetime.end")
        {
            // Lifetime markers only guide optimization. Dropping them keeps the memory state
            // unchanged, which is the intended behavior for these metadata-only intrinsics.
            return Ok(true);
        }

        if callee_name == "llvm.x86.sse2.pause" {
            // `pause` is only a processor scheduling hint for spin loops; it has no observable
            // effect on program state, so lowering it to a no-op is sound.
            return Ok(true);
        }

        if self.unsound.contains(UnsoundFeature::VarArgs)
            && (callee_name.starts_with("llvm.va_start")
                || callee_name.starts_with("llvm.va_end")
                || callee_name.starts_with("llvm.va_copy"))
        {
            // This is intentionally gated behind `--unsound`: variadic bodies that touch
            // `va_list` stay underconstrained until we model its target ABI precisely.
            self.report.approximation(
                UnsoundFeature::VarArgs,
                callee_name,
                &self.function_name,
                &loc,
            );
            return Ok(true);
        }

        if callee_name.starts_with("llvm.") {
            return Err(self
                .unsupported_intrinsic(
                    callee_name,
                    format!("unsupported intrinsic call to {callee_name}"),
                )
                .with_location(loc)
                .into());
        }

        Ok(false)
    }

    fn is_zero_length_memory_intrinsic(&self, args: &[Expr]) -> bool {
        // LLVM memory intrinsics are defined as no-ops when `len == 0`, even if the pointer
        // operands are null or otherwise invalid. CBMC's C library preconditions reject those
        // pointers before considering the zero length, so keep zero-length intrinsics in LLVM
        // space and drop them here.
        args.get(2)
            .and_then(Expr::int_constant_value)
            .is_some_and(|len| len == 0.into())
    }

    fn intrinsic_allow_zero(&self, callee_name: &str, args: &[Expr], kind: &str) -> Result<bool> {
        let flag = args.get(1).ok_or_else(|| {
            self.unsupported_intrinsic(callee_name, format!("missing {kind} zero flag"))
        })?;
        if let Some(flag) = flag.int_constant_value() {
            return Ok(flag != 0.into());
        }
        match flag.value() {
            cprover_bindings::goto_program::ExprValue::BoolConstant(value)
            | cprover_bindings::goto_program::ExprValue::CBoolConstant(value) => Ok(*value),
            _ => Err(self
                .unsupported_intrinsic(callee_name, format!("non-constant {kind} zero flag"))
                .into()),
        }
    }

    fn float_builtin(&self, callee_name: &str) -> Option<(BuiltinFn, BuiltinFn)> {
        [
            ("llvm.fabs", BuiltinFn::Fabs, BuiltinFn::Fabsf),
            ("llvm.floor", BuiltinFn::Floor, BuiltinFn::Floorf),
            ("llvm.ceil", BuiltinFn::Ceil, BuiltinFn::Ceilf),
            ("llvm.round", BuiltinFn::Round, BuiltinFn::Roundf),
            ("llvm.trunc", BuiltinFn::Trunc, BuiltinFn::Truncf),
            ("llvm.copysign", BuiltinFn::Copysign, BuiltinFn::Copysignf),
            ("llvm.sqrt", BuiltinFn::Sqrt, BuiltinFn::Sqrtf),
            ("llvm.fma", BuiltinFn::Fma, BuiltinFn::Fmaf),
            ("llvm.fmuladd", BuiltinFn::Fma, BuiltinFn::Fmaf),
            (
                "llvm.nearbyint",
                BuiltinFn::Nearbyint,
                BuiltinFn::Nearbyintf,
            ),
            ("llvm.rint", BuiltinFn::Rint, BuiltinFn::Rintf),
            ("llvm.maxnum", BuiltinFn::Fmax, BuiltinFn::Fmaxf),
            ("llvm.maximum", BuiltinFn::Fmax, BuiltinFn::Fmaxf),
            ("llvm.minnum", BuiltinFn::Fmin, BuiltinFn::Fminf),
            ("llvm.minimum", BuiltinFn::Fmin, BuiltinFn::Fminf),
        ]
        .into_iter()
        .find_map(|(prefix, double_builtin, float_builtin)| {
            callee_name
                .starts_with(prefix)
                .then_some((double_builtin, float_builtin))
        })
    }

    fn lower_float_builtin_call(
        &mut self,
        callee_name: &str,
        args: &[Expr],
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
        double_builtin: BuiltinFn,
        float_builtin: BuiltinFn,
    ) -> Result<()> {
        let result_type = self
            .types
            .lower_instruction_result(instruction)
            .map_err(anyhow::Error::from)?;
        let builtin = if result_type.is_float() {
            float_builtin
        } else if result_type.is_double() {
            double_builtin
        } else {
            return Err(self
                .unsupported_intrinsic(
                    callee_name,
                    format!(
                        "unsupported floating intrinsic result type {:?}",
                        result_type
                    ),
                )
                .with_location(self.instruction_location(instruction))
                .into());
        };
        self.lower_builtin_call(builtin, args, instruction, body)
    }

    fn lower_builtin_call(
        &mut self,
        builtin: BuiltinFn,
        args: &[Expr],
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        // CBMC may later replace builtin callees with library bodies; attach the call-site
        // location to the callee expression itself so that rewrite path never sees a nil source
        // location on the symbol expression being replaced.
        let function = builtin.as_expr().with_location(loc);
        let args = self.normalize_builtin_args(builtin, args);
        let arguments = Expr::cast_arguments_to_target_equivalent_function_parameter_types(
            &function,
            args,
            self.symbols.machine_model(),
        );
        let lhs = if instruction.get_type().is_void_type() {
            None
        } else {
            let result = self.declare_instruction_result(instruction)?;
            if result.typ() == &builtin.return_type() {
                Some(result)
            } else {
                // LLVM often spells libc-like results as plain integers (`i32`), while CBMC
                // models the builtin with its C type (`c_int`). Call through a builtin-typed
                // temp and cast once so the statement itself stays well-typed.
                let tmp = self.declare_hidden_local(
                    &format!("{}_ret", builtin),
                    builtin.return_type(),
                    self.instruction_location(instruction),
                );
                body.push(Stmt::function_call(
                    Some(tmp.clone()),
                    function.clone(),
                    arguments,
                    loc,
                ));
                body.push(Stmt::assign(
                    result.clone(),
                    tmp.cast_to(result.typ().clone()),
                    loc,
                ));
                return Ok(());
            }
        };
        body.push(Stmt::function_call(lhs, function, arguments, loc));
        Ok(())
    }

    fn lower_minmax_intrinsic(
        &mut self,
        callee_name: &str,
        instruction: InstructionValue<'ctx>,
        args: &[Expr],
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        debug_assert_eq!(args.len(), 2);
        let lhs = args
            .first()
            .cloned()
            .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing min/max lhs"))?;
        let rhs = args
            .get(1)
            .cloned()
            .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing min/max rhs"))?;
        let (lhs, rhs) = self.normalize_icmp_operands(lhs, rhs);

        if lhs.typ().is_vector() || rhs.typ().is_vector() {
            return self.assign_result(
                instruction,
                body,
                self.lower_vector_minmax_value(callee_name, lhs.clone(), rhs, lhs.typ().clone())?,
            );
        }

        let is_signed = callee_name.starts_with("llvm.s");
        let is_min = callee_name.starts_with("llvm.umin") || callee_name.starts_with("llvm.smin");
        let value = self.minmax_value(is_signed, is_min, lhs, rhs)?;
        self.assign_result(instruction, body, value)
    }

    /// `llvm.vector.reduce.<op>` over integer lanes: a left fold with the scalar operator. Integer
    /// `add`/`mul` wrap like their scalar LLVM counterparts, so the fold order is unobservable. The
    /// accumulator is a hidden local so `min`/`max` steps stay linear in size instead of
    /// duplicating the running value into both ternary arms.
    fn lower_vector_reduce_intrinsic(
        &mut self,
        callee_name: &str,
        instruction: InstructionValue<'ctx>,
        args: &[Expr],
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let op = callee_name
            .strip_prefix("llvm.vector.reduce.")
            .and_then(|rest| rest.split('.').next())
            .unwrap_or_default();
        if !matches!(
            op,
            "add" | "mul" | "and" | "or" | "xor" | "smin" | "smax" | "umin" | "umax"
        ) {
            return Err(self
                .unsupported_intrinsic(callee_name, "unsupported vector reduction")
                .into());
        }
        let vector = args
            .first()
            .cloned()
            .ok_or_else(|| self.unsupported_intrinsic(callee_name, "missing reduction operand"))?;
        let lane_type = vector
            .typ()
            .base_type()
            .filter(|typ| vector.typ().is_vector() && typ.is_integer())
            .cloned()
            .ok_or_else(|| {
                self.unsupported_intrinsic(callee_name, "vector reduction expects integer lanes")
            })?;
        let len = vector.typ().len().unwrap_or(0) as usize;
        let loc = self.instruction_location(instruction);
        let acc = self.declare_hidden_local("reduce", lane_type, loc);
        body.push(Stmt::assign(
            acc.clone(),
            self.aggregate_component(vector.clone(), 0)?,
            loc,
        ));
        for lane in 1..len {
            let x = self.aggregate_component(vector.clone(), lane)?;
            let a = acc.clone();
            let value = match op {
                "add" => a.plus(x),
                "mul" => a.mul(x),
                "and" => a.bitand(x),
                "or" => a.bitor(x),
                "xor" => a.bitxor(x),
                _ => self.minmax_value(op.starts_with('s'), op.ends_with("min"), a, x)?,
            };
            body.push(Stmt::assign(acc.clone(), value, loc));
        }
        self.assign_result(instruction, body, acc)
    }

    fn lower_funnel_shift_intrinsic(
        &mut self,
        is_left: bool,
        callee_name: &str,
        instruction: InstructionValue<'ctx>,
        args: &[Expr],
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        debug_assert_eq!(args.len(), 3);
        let lhs = args[0].clone();
        let rhs = args[1].clone();
        let shift = args[2].clone();

        if lhs.typ().is_vector() {
            return Err(self
                .unsupported_intrinsic(
                    callee_name,
                    "vector funnel shift intrinsics are unsupported",
                )
                .into());
        }

        let result_type = lhs.typ().clone();
        if !result_type.is_integer() {
            return Err(self
                .unsupported_intrinsic(
                    callee_name,
                    "funnel shift intrinsics expect integer operands",
                )
                .into());
        }

        let width = self.integer_width(&result_type)?;
        let width_expr = Expr::int_constant(width, result_type.clone());
        let zero = result_type.zero();
        let shift_mod = shift.cast_to(result_type.clone()).rem(width_expr.clone());
        let shift_is_zero = shift_mod.clone().eq(zero.clone());
        let unclamped_reverse_shift = width_expr.sub(shift_mod.clone());
        let reverse_shift = shift_is_zero
            .clone()
            .ternary(zero.clone(), unclamped_reverse_shift);
        debug_assert_eq!(rhs.typ(), &result_type);

        let shifted = if is_left {
            lhs.clone()
                .shl(shift_mod.clone())
                .bitor(rhs.clone().lshr(reverse_shift))
        } else {
            lhs.clone()
                .shl(reverse_shift)
                .bitor(rhs.clone().lshr(shift_mod.clone()))
        };
        let identity = if is_left { lhs } else { rhs };
        let value = shift_is_zero.ternary(identity, shifted);

        self.assign_result(instruction, body, value)
    }

    fn lower_overflow_intrinsic(
        &mut self,
        instruction: InstructionValue<'ctx>,
        args: &[Expr],
        body: &mut Vec<Stmt>,
        op: fn(Expr, Expr) -> Expr,
        signed: bool,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let result = self.declare_instruction_result(instruction)?;
        let lhs = args.first().cloned().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Intrinsic,
                format!("missing overflow intrinsic lhs in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let rhs = args.get(1).cloned().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Intrinsic,
                format!("missing overflow intrinsic rhs in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let (lhs, rhs) = if signed {
            (self.signed_view(lhs)?, self.signed_view(rhs)?)
        } else {
            (lhs, rhs)
        };
        // CBMC exposes overflow-with-result as its own struct type. Register that type so we can
        // project `result` / `overflowed`, then rebuild the LLVM aggregate result shape explicitly.
        self.ensure_overflow_result_symbol(lhs.typ().clone());
        let fields = result
            .typ()
            .lookup_components(self.symbols)
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Intrinsic,
                    format!(
                        "overflow intrinsic result is not a lowered aggregate in {}",
                        self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
            })?;
        if fields.len() < 2 {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Intrinsic,
                format!(
                    "overflow intrinsic result shape is unsupported in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }
        let result_type = fields[0].typ();
        let overflow_type = fields[1].typ();
        let overflow = op(lhs, rhs);
        let value = overflow
            .clone()
            .member("result", self.symbols)
            .cast_to(result_type);
        let overflowed = overflow
            .member("overflowed", self.symbols)
            .cast_to(overflow_type);
        let result_expr = Expr::struct_expr_from_values(
            result.typ().clone(),
            vec![value.clone(), overflowed.clone()],
            self.symbols,
        );

        body.push(Stmt::assign(result.clone(), result_expr, loc));
        self.aggregates
            .insert(instruction.as_value_ref() as usize, vec![value, overflowed]);
        Ok(())
    }

    fn lower_saturating_intrinsic(
        &mut self,
        callee_name: &str,
        instruction: InstructionValue<'ctx>,
        args: &[Expr],
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        debug_assert_eq!(args.len(), 2);
        let lhs = args[0].clone();
        let rhs = args[1].clone();

        if lhs.typ().is_vector() || rhs.typ().is_vector() {
            return Err(self
                .unsupported_intrinsic(
                    callee_name,
                    "vector saturating arithmetic intrinsics are unsupported",
                )
                .into());
        }

        let result_type = lhs.typ().clone();
        if !result_type.is_integer() {
            return Err(self
                .unsupported_intrinsic(
                    callee_name,
                    "saturating arithmetic intrinsics expect integer operands",
                )
                .into());
        }
        let value = if callee_name.starts_with("llvm.uadd.sat") {
            self.saturating_overflow_value(
                lhs,
                rhs,
                result_type.clone(),
                Expr::add_overflow_result,
                self.integer_max_value(&result_type)?,
            )?
        } else if callee_name.starts_with("llvm.usub.sat") {
            self.saturating_overflow_value(
                lhs,
                rhs,
                result_type.clone(),
                Expr::sub_overflow_result,
                result_type.zero(),
            )?
        } else {
            let lhs_signed = self.signed_view(lhs.clone())?;
            let rhs_signed = self.signed_view(rhs)?;
            let saturation = lhs_signed.clone().lt(lhs_signed.typ().zero()).ternary(
                self.integer_signed_min_value(&result_type)?,
                self.integer_signed_max_value(&result_type)?,
            );
            if callee_name.starts_with("llvm.sadd.sat") {
                self.saturating_overflow_value(
                    lhs_signed,
                    rhs_signed,
                    result_type,
                    Expr::add_overflow_result,
                    saturation,
                )?
            } else {
                self.saturating_overflow_value(
                    lhs_signed,
                    rhs_signed,
                    result_type,
                    Expr::sub_overflow_result,
                    saturation,
                )?
            }
        };

        self.assign_result(instruction, body, value)?;
        Ok(())
    }

    fn saturating_overflow_value(
        &mut self,
        lhs: Expr,
        rhs: Expr,
        result_type: Type,
        op: fn(Expr, Expr) -> Expr,
        on_overflow: Expr,
    ) -> Result<Expr> {
        self.ensure_overflow_result_symbol(lhs.typ().clone());
        let overflow = op(lhs, rhs);
        let value = overflow
            .clone()
            .member("result", self.symbols)
            .cast_to(result_type);
        let overflowed = overflow
            .member("overflowed", self.symbols)
            .cast_to(Type::bool());
        Ok(overflowed.ternary(on_overflow, value))
    }

    fn integer_max_value(&self, typ: &Type) -> Result<Expr> {
        let width = self.integer_width(typ)?;
        Ok(Expr::int_constant(
            (BigInt::from(1u8) << width) - 1u8,
            typ.clone(),
        ))
    }

    fn integer_signed_max_value(&self, typ: &Type) -> Result<Expr> {
        let width = self.integer_width(typ)?;
        Ok(Expr::int_constant(
            (BigInt::from(1u8) << (width - 1)) - 1u8,
            typ.clone(),
        ))
    }

    fn integer_signed_min_value(&self, typ: &Type) -> Result<Expr> {
        let width = self.integer_width(typ)?;
        Ok(Expr::int_constant(
            -(BigInt::from(1u8) << (width - 1)),
            typ.clone(),
        ))
    }

    fn integer_width(&self, typ: &Type) -> Result<u64> {
        if typ.is_c_bool() || typ.is_bool() {
            return Ok(1);
        }
        typ.native_width(self.symbols.machine_model())
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Intrinsic,
                    format!(
                        "expected fixed-width integer type for intrinsic in {}",
                        self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
                .into()
            })
    }

    fn ensure_overflow_result_symbol(&mut self, operand_type: Type) {
        let overflow_type = arithmetic_overflow_result_type(operand_type);
        let tag = overflow_type.tag().unwrap();
        let symbol_name = format!("tag-{tag}");
        if self.symbols.lookup(symbol_name).is_none() {
            self.symbols
                .insert(Symbol::aggr_ty(overflow_type, tag.to_string()));
        }
    }

    fn normalize_builtin_args(&self, builtin: BuiltinFn, args: &[Expr]) -> Vec<Expr> {
        let mut args = args.to_vec();
        if matches!(builtin, BuiltinFn::Memset)
            && let Some(value) = args.get_mut(1)
        {
            // CBMC models `memset` with the C ABI shape `(void *, int, size_t)`, not LLVM's
            // narrower byte-typed fill argument.
            *value = value.clone().cast_to(Type::c_int());
        }
        args
    }

    fn lower_atomic_rmw(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let result = self.declare_instruction_result(instruction)?;
        let ptr = self.lower_operand(instruction, 0)?;
        let value = self.lower_operand(instruction, 1)?;
        let elem_type = result.typ().clone();
        let deref = ptr.cast_to(elem_type.clone().to_pointer()).dereference();

        let rhs = match instruction.get_atomic_rmw_bin_op().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("malformed atomicrmw in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })? {
            AtomicRMWBinOp::Xchg => value.cast_to(elem_type.clone()),
            AtomicRMWBinOp::Add => result.clone().plus(value.cast_to(elem_type.clone())),
            AtomicRMWBinOp::Sub => result.clone().sub(value.cast_to(elem_type.clone())),
            AtomicRMWBinOp::And => result.clone().bitand(value.cast_to(elem_type.clone())),
            AtomicRMWBinOp::Or => result.clone().bitor(value.cast_to(elem_type.clone())),
            AtomicRMWBinOp::Xor => result.clone().bitxor(value.cast_to(elem_type.clone())),
            op => {
                return Err(Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!(
                        "unsupported atomicrmw op {:?} in {}",
                        op, self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
                .into());
            }
        };

        body.push(Stmt::atomic_block(
            vec![
                Stmt::assign(result.clone(), deref.clone(), loc),
                Stmt::assign(deref, rhs, loc),
            ],
            loc,
        ));
        Ok(())
    }

    fn lower_atomic_cmpxchg(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let result = self.declare_instruction_result(instruction)?;
        let fields = result
            .typ()
            .lookup_components(self.symbols)
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!(
                        "cmpxchg result is not a lowered aggregate in {}",
                        self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
            })?;
        if fields.len() < 2 {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "cmpxchg result shape is unsupported in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }

        let value_type = fields[0].typ().clone();
        let success_type = fields[1].typ().clone();
        let old = self.declare_hidden_local(
            "cmpxchg_old",
            value_type.clone(),
            self.instruction_location(instruction),
        );
        let success = self.declare_hidden_local(
            "cmpxchg_success",
            Type::bool(),
            self.instruction_location(instruction),
        );

        let ptr = self.lower_operand(instruction, 0)?;
        let cmp = self
            .lower_operand(instruction, 1)?
            .cast_to(value_type.clone());
        let new_value = self
            .lower_operand(instruction, 2)?
            .cast_to(value_type.clone());
        let deref = ptr.cast_to(value_type.clone().to_pointer()).dereference();
        let result_expr = Expr::struct_expr_from_values(
            result.typ().clone(),
            vec![old.clone(), success.clone().cast_to(success_type)],
            self.symbols,
        );

        body.push(Stmt::atomic_block(
            vec![
                Stmt::assign(old.clone(), deref.clone(), loc),
                Stmt::assign(success.clone(), old.clone().eq(cmp), loc),
                Stmt::if_then_else(
                    success.clone(),
                    Stmt::assign(deref, new_value, loc),
                    None,
                    loc,
                ),
                Stmt::assign(result.clone(), result_expr, loc),
            ],
            loc,
        ));
        // `extractvalue` on the immediate `cmpxchg` result is common; cache the lowered
        // field expressions so we can answer it without adding general aggregate SSA support.
        self.aggregates.insert(
            instruction.as_value_ref() as usize,
            vec![old, success.cast_to(Type::c_bool())],
        );
        Ok(())
    }

    fn lower_extractvalue(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let aggregate = self.operand_value(
            instruction,
            0,
            UnsupportedFeature::Instruction,
            "malformed extractvalue aggregate",
        )?;
        let indices = instruction.get_indices();
        if let Some(fields) = self.aggregates.get(&(aggregate.as_value_ref() as usize))
            && indices.len() == 1
        {
            let value = fields.get(indices[0] as usize).cloned().ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!(
                        "extractvalue index {} out of range in {}",
                        indices[0], self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
            })?;
            let result = self.declare_instruction_result(instruction)?;
            body.push(Stmt::assign(
                result.clone(),
                value.cast_to(result.typ().clone()),
                loc,
            ));
            return Ok(());
        }

        let aggregate = self.lower_value(aggregate)?;
        let value = self.project_aggregate_value(aggregate, &indices)?;
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            value.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn lower_insertvalue(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let indices = instruction.get_indices();
        if indices.is_empty() {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("insertvalue without indices in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }

        let aggregate = self.lower_operand(instruction, 0)?;
        let value = self.lower_operand(instruction, 1)?;
        let rebuilt = self.insert_aggregate_value(aggregate, &indices, value)?;
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            rebuilt.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn lower_insertelement(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let vector = self.lower_operand(instruction, 0)?;
        let value = self.lower_operand(instruction, 1)?;
        let index = self.lower_operand(instruction, 2)?;
        let typ = vector.typ().clone();
        let len = typ.len().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("insertelement expects a vector in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let elem_type = typ.base_type().cloned().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("insertelement expects a vector in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;

        // GOTO has vector values but no persistent single-lane update primitive, so model
        // insertelement by rebuilding the whole vector with a lane-wise select.
        let elems = (0..len)
            .map(|lane| {
                let current = self.aggregate_component(vector.clone(), lane as usize)?;
                Ok(index
                    .clone()
                    .eq(Expr::int_constant(lane, index.typ().clone()))
                    .ternary(value.clone().cast_to(elem_type.clone()), current))
            })
            .collect::<Result<Vec<_>>>()?;
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            Expr::vector_expr(result.typ().clone(), elems),
            loc,
        ));
        Ok(())
    }

    fn lower_extractelement(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let vector = self.lower_operand(instruction, 0)?;
        let index = self.lower_operand(instruction, 1)?;
        let value = self.array_like_component(vector, index)?;
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            value.cast_to(result.typ().clone()),
            loc,
        ));
        Ok(())
    }

    fn lower_shufflevector(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let vector1 = self.lower_operand(instruction, 0)?;
        let vector2 = self.lower_operand(instruction, 1)?;
        let result = self.declare_instruction_result(instruction)?;
        let vector1_len = vector1.typ().len().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "shufflevector expects vector operands in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let elem_type = result.typ().base_type().cloned().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "shufflevector expects a vector result in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
        })?;

        // Undef mask lanes are unconstrained in LLVM IR; emit nondet for those lanes rather
        // than silently pinning them to one arbitrary source element.
        let elems = (0..unsafe { LLVMGetNumMaskElements(instruction.as_value_ref()) })
            .map(|lane| {
                let mask = unsafe { LLVMGetMaskValue(instruction.as_value_ref(), lane) };
                if mask < 0 {
                    return Ok(util::nondet_expr(elem_type.clone()));
                }
                let index = mask as u64;
                if index < vector1_len {
                    self.aggregate_component(vector1.clone(), index as usize)
                } else {
                    self.aggregate_component(vector2.clone(), (index - vector1_len) as usize)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        body.push(Stmt::assign(
            result.clone(),
            Expr::vector_expr(result.typ().clone(), elems),
            loc,
        ));
        Ok(())
    }

    fn project_aggregate_value(&self, aggregate: Expr, indices: &[u32]) -> Result<Expr> {
        if indices.is_empty() {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("extractvalue without indices in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }

        let mut current = aggregate;
        for &index in indices {
            current = self.aggregate_component(current, index as usize)?;
        }
        Ok(current)
    }

    fn insert_aggregate_value(
        &self,
        aggregate: Expr,
        indices: &[u32],
        value: Expr,
    ) -> Result<Expr> {
        let (&index, rest) = indices.split_first().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("insertvalue without indices in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let mut components = self.aggregate_components(aggregate.clone())?;
        let slot = components.get_mut(index as usize).ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "aggregate index {} out of range in {}",
                    index, self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
        })?;

        *slot = if rest.is_empty() {
            value.cast_to(slot.typ().clone())
        } else {
            self.insert_aggregate_value(slot.clone(), rest, value)?
        };
        self.rebuild_aggregate(aggregate.typ().clone(), components)
    }

    fn aggregate_component(&self, aggregate: Expr, index: usize) -> Result<Expr> {
        let typ = aggregate.typ().clone();
        if typ.is_struct_tag() {
            let field = format!("field{index}");
            if typ.lookup_field(field.as_str(), self.symbols).is_none() {
                return Err(Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!(
                        "aggregate index {} out of range in {}",
                        index, self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
                .into());
            }
            return Ok(aggregate.member(field, self.symbols));
        }

        self.array_like_component(aggregate, Expr::int_constant(index as u64, Type::ssize_t()))
    }

    fn array_like_component(&self, aggregate: Expr, index: Expr) -> Result<Expr> {
        let typ = aggregate.typ().clone();
        let elem_type = typ.base_type().cloned().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("unsupported aggregate access in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let len = typ.len().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("unsupported aggregate access in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        // Only constant indices are range-checked here. Symbolic indices stay symbolic and are
        // delegated to CBMC, which keeps this helper small and avoids duplicating index semantics.
        if let Some(constant) = index.int_constant_value()
            && usize::try_from(constant.clone())
                .ok()
                .is_none_or(|index| index >= len as usize)
        {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "aggregate index {} out of range in {}",
                    constant, self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }

        Ok(aggregate
            .index(index.cast_to(Type::ssize_t()))
            .cast_to(elem_type))
    }

    fn aggregate_components(&self, aggregate: Expr) -> Result<Vec<Expr>> {
        let typ = aggregate.typ().clone();
        if typ.is_struct_tag() {
            let fields = typ.lookup_components(self.symbols).ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!("missing aggregate definition in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
            })?;
            return Ok(fields
                .iter()
                .filter(|field| !field.is_padding())
                .map(|field| aggregate.clone().member(field.name(), self.symbols))
                .collect());
        }

        let len = typ.len().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("unsupported aggregate update in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        Ok((0..len)
            .map(|index| {
                aggregate
                    .clone()
                    .index(Expr::int_constant(index, Type::ssize_t()))
            })
            .collect())
    }

    fn rebuild_aggregate(&self, typ: Type, components: Vec<Expr>) -> Result<Expr> {
        // LLVM aggregate updates are persistent values, not in-place mutation. Rebuild only the
        // traversed aggregate layer so nested `insertvalue` keeps SSA semantics without extra state.
        if typ.is_struct_tag() {
            return Ok(Expr::struct_expr_from_values(typ, components, self.symbols));
        }
        if typ.is_array() {
            return Ok(Expr::array_expr(typ, components));
        }
        if typ.is_vector() {
            return Ok(Expr::vector_expr(typ, components));
        }
        Err(Diagnostic::unsupported_feature(
            UnsupportedFeature::Instruction,
            format!("unsupported aggregate update in {}", self.function_name),
        )
        .with_symbol(self.function_name.clone())
        .into())
    }

    fn validate_landingpad_shape(&mut self, instruction: InstructionValue<'ctx>) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let clause_count = unsafe { LLVMGetNumClauses(instruction.as_value_ref()) };
        let cleanup = unsafe { LLVMIsCleanup(instruction.as_value_ref()) != 0 };
        if clause_count == 0 && cleanup {
            return Ok(());
        }
        if clause_count == 1 {
            let clause =
                unsafe { BasicValueEnum::new(LLVMGetClause(instruction.as_value_ref(), 0)) };
            if clause.is_pointer_value() && clause.into_pointer_value().is_null() {
                return Ok(());
            }
        }
        Err(Diagnostic::unsupported_feature(
            UnsupportedFeature::ControlFlow,
            format!("unsupported landingpad shape in {}", self.function_name),
        )
        .with_symbol(self.function_name.clone())
        .with_location(loc)
        .into())
    }

    fn eh_active_expr(&self, location: Location) -> Result<Expr> {
        self.runtime_symbol_expr(EH_ACTIVE_SYMBOL, location)
    }

    fn eh_ptr_expr(&self, location: Location) -> Result<Expr> {
        self.runtime_symbol_expr(EH_PTR_SYMBOL, location)
    }

    fn eh_selector_expr(&self, location: Location) -> Result<Expr> {
        self.runtime_symbol_expr(EH_SELECTOR_SYMBOL, location)
    }

    fn runtime_symbol_expr(&self, name: &str, location: Location) -> Result<Expr> {
        let symbol = self.symbols.lookup(name).ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("missing runtime symbol {name}"),
            )
            .with_symbol(name.to_owned())
            .with_location(location)
        })?;
        Ok(symbol.to_expr().with_location(location))
    }

    fn clear_exception(&self, body: &mut Vec<Stmt>, loc: Location) -> Result<()> {
        body.push(Stmt::assign(
            self.eh_active_expr(loc)?,
            Expr::bool_false(),
            loc,
        ));
        body.push(Stmt::assign(
            self.eh_ptr_expr(loc)?,
            Type::void_pointer().null(),
            loc,
        ));
        body.push(Stmt::assign(
            self.eh_selector_expr(loc)?,
            Expr::int_constant(0, Type::c_int()),
            loc,
        ));
        Ok(())
    }

    fn raise_exception(
        &self,
        exn: Expr,
        selector: Expr,
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<()> {
        body.push(Stmt::assign(
            self.eh_active_expr(loc)?,
            Expr::bool_true(),
            loc,
        ));
        body.push(Stmt::assign(
            self.eh_ptr_expr(loc)?,
            exn.cast_to(Type::void_pointer()),
            loc,
        ));
        body.push(Stmt::assign(
            self.eh_selector_expr(loc)?,
            selector.cast_to(Type::c_int()),
            loc,
        ));
        Ok(())
    }

    fn exceptional_return(&mut self, loc: Location) -> Result<Stmt> {
        let return_type = self.lowered_return_type();
        Ok(if return_type == Type::empty() {
            Stmt::ret(None, loc)
        } else {
            Stmt::ret(Some(util::nondet_expr(return_type)), loc)
        })
    }

    fn lower_value(&mut self, value: BasicValueEnum<'ctx>) -> Result<Expr> {
        if let Some(expr) = self.values.get(&(value.as_value_ref() as usize)) {
            return Ok(expr.clone());
        }

        let typ = self.types.lower_value(value)?;
        self.insert_pending_aggregate_symbols();

        // undef in a function body means "any value" — unlike global undef which
        // follows C static-storage zero-init (handled in GlobalLowerer).
        if unsafe { LLVMIsUndef(value.as_value_ref()) } != 0 {
            return Ok(util::nondet_expr(typ));
        }
        self.globals
            .lower_initializer(
                &self.function_name,
                value,
                &typ,
                self.types,
                self.symbols,
                &self.current_location(),
            )
            .map_err(|err| {
                let rendered = value.print_to_string().to_string();
                anyhow::anyhow!("{err} [while lowering value: {rendered}]")
            })
    }

    fn phi_edge_assignments(
        &mut self,
        predecessor: BasicBlock<'ctx>,
        successor: BasicBlock<'ctx>,
    ) -> Result<Vec<Stmt>> {
        let mut rhs_temps = Vec::new();
        let mut assigns = Vec::new();

        for instruction in successor.get_instructions() {
            if instruction.get_opcode() != InstructionOpcode::Phi {
                break;
            }

            let phi = PhiValue::try_from(instruction).map_err(|_| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::ControlFlow,
                    format!("invalid PHI wrapper in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
            })?;
            let (incoming, _) = phi
                .get_incomings()
                .find(|(_, block)| *block == predecessor)
                .ok_or_else(|| {
                    Diagnostic::unsupported_feature(
                        UnsupportedFeature::ControlFlow,
                        format!(
                            "missing PHI incoming edge from predecessor in {}",
                            self.function_name
                        ),
                    )
                    .with_symbol(self.function_name.clone())
                })?;
            let rhs = self.lower_value(incoming)?;
            let result = self.declare_instruction_result(phi.as_instruction())?;

            // LLVM PHI assignments happen simultaneously on an edge. Materialize each RHS
            // into a temp first so cyclic or cross-referencing PHIs keep edge semantics.
            let tmp = self.declare_hidden_local(
                "phi_edge",
                rhs.typ().clone(),
                self.instruction_location(phi.as_instruction()),
            );
            let loc = self.instruction_location(phi.as_instruction());
            rhs_temps.push(Stmt::assign(tmp.clone(), rhs, loc));
            assigns.push(Stmt::assign(
                result.clone(),
                tmp.cast_to(result.typ().clone()),
                loc,
            ));
        }

        rhs_temps.extend(assigns);
        Ok(rhs_temps)
    }

    fn compute_gep_address(
        &mut self,
        instruction: InstructionValue<'ctx>,
        base: Expr,
        mut current_type: Type,
    ) -> Result<Expr> {
        let mut ptr = base.clone().cast_to(current_type.clone().to_pointer());
        let indices = (1..instruction.get_num_operands())
            .map(|i| self.lower_operand(instruction, i))
            .collect::<Result<Vec<_>>>()?;

        for (position, index) in indices.into_iter().enumerate() {
            if position == 0 {
                if let Some((array, base_index)) = self.decayed_array_base(&base, &current_type) {
                    let index = index.cast_to(Type::ssize_t());
                    let total_index = if base_index
                        .int_constant_value()
                        .is_some_and(|value| value == 0.into())
                    {
                        index
                    } else {
                        base_index.plus(index)
                    };
                    ptr = array.index(total_index).address_of();
                    continue;
                }
                ptr = ptr.index(index.cast_to(Type::ssize_t())).address_of();
                continue;
            }

            if current_type.is_struct_tag() {
                let field = self.const_field_index(&index, &current_type)?;
                let field_name = format!("field{field}");
                current_type = current_type
                    .lookup_field_type(field_name.as_str(), self.symbols)
                    .ok_or_else(|| {
                        Diagnostic::unsupported_feature(
                            UnsupportedFeature::Instruction,
                            format!("unknown GEP field {field} in {}", self.function_name),
                        )
                        .with_symbol(self.function_name.clone())
                    })?;
                ptr = ptr
                    .dereference()
                    .member(field_name, self.symbols)
                    .address_of();
                continue;
            }

            let elem_type = current_type.base_type().cloned().ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!(
                        "unsupported GEP through non-aggregate in {}",
                        self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
            })?;
            // After the initial pointer-step, later GEP indices descend into the pointee
            // structure, so we dereference before indexing into arrays or vectors.
            ptr = ptr
                .dereference()
                .index(index.cast_to(Type::ssize_t()))
                .address_of();
            current_type = elem_type;
        }

        Ok(ptr)
    }

    /// Recognize pointers that came from counted/VLA `alloca` array decay (`&slot[0]`) so the
    /// first GEP index can be folded into `&slot[i]`.
    ///
    /// Only fold when `array` is the root `Symbol` itself: if `array` is a dereferenced pointer or
    /// aggregate subobject (such as a row of a 2D array), a subsequent 1-index GEP with a negative
    /// offset may legally step into the preceding subobject in the enclosing allocation, which
    /// GOTO `Index` on the sub-array value would reject as out of bounds.
    ///
    /// The current lowering only inserts at most one pointer typecast around such addresses, so a
    /// single `Typecast(AddressOf(...))` peel is sufficient here.
    fn decayed_array_base(&self, base: &Expr, pointee: &Type) -> Option<(Expr, Expr)> {
        let peeled = match base.value() {
            ExprValue::AddressOf(inner) => inner.clone(),
            ExprValue::Typecast(inner) => match inner.value() {
                ExprValue::AddressOf(inner) => inner.clone(),
                _ => return None,
            },
            _ => return None,
        };
        let ExprValue::Index { array, index } = peeled.value() else {
            return None;
        };
        (array.is_symbol()
            && array.typ().is_array_like()
            && array.typ().base_type() == Some(pointee))
        .then(|| (array.clone(), index.clone()))
    }

    /// Returns true when the expression is already a plain address and does not need a temporary
    /// local to preserve SSA semantics.
    fn is_direct_address_expr(&self, expr: &Expr) -> bool {
        match expr.value() {
            ExprValue::AddressOf(_) => true,
            ExprValue::Typecast(inner) => matches!(inner.value(), ExprValue::AddressOf(_)),
            _ => false,
        }
    }

    fn function_expr(&self, name: &str, location: Location) -> Result<Expr> {
        let symbol = self.symbols.lookup(name).ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("missing callee symbol {name}"),
            )
            .with_symbol(name.to_owned())
        })?;
        Ok(symbol.to_expr().with_location(location))
    }

    fn declare_instruction_result(&mut self, instruction: InstructionValue<'ctx>) -> Result<Expr> {
        let key = instruction.as_value_ref() as usize;
        if let Some(expr) = self.values.get(&key) {
            return Ok(expr.clone());
        }

        let typ = self.types.lower_instruction_result(instruction)?;
        self.insert_pending_aggregate_symbols();
        let base = self.value_base_name(
            self.next_tmp,
            "tmp",
            &instruction
                .get_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        self.next_tmp += 1;
        let location = self.instruction_location(instruction);
        let expr = self.insert_local_symbol(&base, typ, false, true, location);
        // LLVM SSA values can cross basic-block boundaries, so their storage must
        // outlive the block-local scopes we use to attach labels.
        self.prologue.push(Stmt::decl(expr.clone(), None, location));
        self.values.insert(key, expr.clone());
        Ok(expr)
    }

    fn declare_hidden_local(&mut self, stem: &str, typ: Type, location: Location) -> Expr {
        let (expr, _) = self.hidden_local_symbol(stem, typ, location);
        self.prologue.push(Stmt::decl(expr.clone(), None, location));
        expr
    }

    fn declare_hidden_local_at_point(
        &mut self,
        body: &mut Vec<Stmt>,
        stem: &str,
        typ: Type,
        location: Location,
    ) -> (Expr, InternedString) {
        // Some lowering paths need both the expression and the raw symbol identifier so the same
        // name can be referenced from a generated type, e.g. a VLA bound.
        let (expr, identifier) = self.hidden_local_symbol(stem, typ, location);
        body.push(Stmt::decl(expr.clone(), None, location));
        (expr, identifier)
    }

    fn hidden_local_symbol(
        &mut self,
        stem: &str,
        typ: Type,
        location: Location,
    ) -> (Expr, InternedString) {
        let base = format!("{stem}{}", self.next_tmp);
        self.next_tmp += 1;
        let identifier = self.make_local_identifier(&base);
        let expr = self.insert_local_symbol(&base, typ, false, true, location);
        (expr, identifier)
    }

    fn make_local_identifier(&self, base_name: &str) -> InternedString {
        format!("{}::{}", self.function_name, base_name).into()
    }

    fn insert_local_symbol(
        &mut self,
        base_name: &str,
        typ: Type,
        is_parameter: bool,
        hidden: bool,
        location: Location,
    ) -> Expr {
        let name = self.make_local_identifier(base_name);
        let symbol = Symbol::variable(name, base_name.to_owned(), typ.clone(), location)
            .with_is_parameter(is_parameter)
            .with_is_hidden(hidden)
            .with_is_state_var(false)
            .with_is_static_lifetime(false);
        self.symbols.insert(symbol);
        if is_parameter {
            self.symbols
                .insert_parameter(self.function_name.clone(), name);
        }
        Expr::symbol_expression(name, typ).with_location(location)
    }

    fn value_base_name(&self, index: usize, fallback: &str, llvm_name: &str) -> String {
        let candidate = if llvm_name.is_empty() {
            format!("{fallback}{index}")
        } else {
            llvm_name.to_owned()
        };
        util::sanitize_name(&candidate)
    }

    fn operand_value(
        &self,
        instruction: InstructionValue<'ctx>,
        index: u32,
        feature: UnsupportedFeature,
        message: &str,
    ) -> Result<BasicValueEnum<'ctx>> {
        instruction
            .get_operand(index)
            .and_then(|operand| operand.value())
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    feature,
                    format!("{message} in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
                .with_location(self.instruction_location(instruction))
                .into()
            })
    }

    /// Resolve an operand that points at a constant NUL-terminated string global, such as the
    /// message of `__CPROVER_assert`. Returns `None` for non-literal (runtime) pointers.
    fn constant_string_operand(
        &self,
        instruction: InstructionValue<'ctx>,
        index: u32,
    ) -> Option<String> {
        let mut raw = instruction.get_operand(index)?.value()?.as_value_ref();
        // Clang spells string arguments as `@.str` or `getelementptr (@.str, 0, 0)`; peel
        // constant expressions down to the underlying global.
        while unsafe { !LLVMIsAConstantExpr(raw).is_null() } {
            raw = unsafe { LLVMGetOperand(raw, 0) };
        }
        if unsafe { LLVMIsAGlobalVariable(raw).is_null() } {
            return None;
        }
        let BasicValueEnum::ArrayValue(array) =
            unsafe { GlobalValue::new(raw) }.get_initializer()?
        else {
            return None;
        };
        let bytes = array.as_const_string()?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    fn operand_block(
        &self,
        instruction: InstructionValue<'ctx>,
        index: u32,
        feature: UnsupportedFeature,
        message: &str,
    ) -> Result<BasicBlock<'ctx>> {
        instruction
            .get_operand(index)
            .and_then(|operand| operand.block())
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    feature,
                    format!("{message} in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
                .with_location(self.instruction_location(instruction))
                .into()
            })
    }

    fn lower_operand(&mut self, instruction: InstructionValue<'ctx>, index: u32) -> Result<Expr> {
        let value = self.operand_value(
            instruction,
            index,
            UnsupportedFeature::Instruction,
            "malformed instruction operand",
        )?;
        self.lower_value(value).map_err(|err| {
            let consumer = instruction.print_to_string().to_string();
            anyhow::anyhow!("{err} [operand of instruction: {consumer}]")
        })
    }

    /// True when `*(&slot)` may be lowered as plain `slot`.
    ///
    /// In GOTO, `*(&a[i])` is pointer arithmetic: an out-of-object `i` is a `--pointer-check`
    /// failure. Bare `a[i]` is array value selection instead: an out-of-range read is
    /// unconstrained, an out-of-range write is dropped, and only `--bounds-check` reports it.
    /// LLVM GEPs may legally step out of a subarray into its neighbour, so a bare `Index` is only
    /// kept on VLA `alloca` symbols, where CBMC cannot bound `*(&vla[i])` and reports a spurious
    /// "pointer outside object bounds"; out-of-range VLA accesses therefore rely on
    /// `--bounds-check`. `Member` and `Dereference` keep pointer semantics as long as they contain
    /// no such `Index`.
    fn is_peelable_slot(slot: &Expr) -> bool {
        match slot.value() {
            ExprValue::Symbol { .. } | ExprValue::Dereference(_) => true,
            ExprValue::Member { lhs, .. } => Self::is_peelable_slot(lhs),
            ExprValue::Index { array, .. } => {
                array.is_symbol() && matches!(array.typ(), Type::VariableArray { .. })
            }
            _ => false,
        }
    }

    fn direct_store_slot(&self, ptr: &Expr, pointee: &Type) -> Option<Expr> {
        let slot = match ptr.value() {
            ExprValue::AddressOf(slot) => slot.clone(),
            ExprValue::Typecast(inner) => match inner.value() {
                ExprValue::AddressOf(slot) => slot.clone(),
                _ => return None,
            },
            _ => return None,
        };
        (Self::is_peelable_slot(&slot) && slot.typ() == pointee).then_some(slot)
    }

    fn direct_load_pointee(&self, ptr: &Expr, pointee: &Type) -> Option<Expr> {
        let slot = match ptr.value() {
            ExprValue::AddressOf(slot) => slot.clone(),
            ExprValue::Typecast(inner) => match inner.value() {
                ExprValue::AddressOf(slot) => slot.clone(),
                _ => return None,
            },
            _ => return None,
        };
        if !Self::is_peelable_slot(&slot) {
            return None;
        }
        if slot.typ() == pointee {
            return Some(slot);
        }
        // Local pointer slots often flow through empty* before we have enough context to recover
        // their pointee type. When the access is still just `*(&slot)` and both sides are pointers,
        // a direct cast of the slot is equivalent and avoids reintroducing a dereference chain that
        // later CBMC concurrency passes treat much more pessimistically.
        (slot.typ().is_pointer() && pointee.is_pointer()).then(|| slot.cast_to(pointee.clone()))
    }

    fn assign_result(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
        value: Expr,
    ) -> Result<()> {
        let result = self.declare_instruction_result(instruction)?;
        body.push(Stmt::assign(
            result.clone(),
            value.cast_to(result.typ().clone()),
            self.instruction_location(instruction),
        ));
        Ok(())
    }

    /// Build a fail-closed control-flow diagnostic for the function being lowered.
    fn unsupported_control_flow(&self, message: &str, loc: Location) -> Diagnostic {
        Diagnostic::unsupported_feature(
            UnsupportedFeature::ControlFlow,
            format!("{message} in {}", self.function_name),
        )
        .with_symbol(self.function_name.clone())
        .with_location(loc)
    }

    fn unsupported_intrinsic(&self, name: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::unsupported_feature(UnsupportedFeature::Intrinsic, message)
            .with_symbol(name.to_owned())
    }

    fn validate_inline_asm_model_signature(
        &mut self,
        instruction: InstructionValue<'ctx>,
        model: &str,
        function: &Expr,
        arguments: &[Expr],
    ) -> Result<()> {
        let typ = function.typ();
        let parameters = typ
            .parameters()
            .ok_or_else(|| anyhow::anyhow!("inline asm model `{model}` is not a function"))?;
        if typ.is_variadic_code() || parameters.len() != arguments.len() {
            anyhow::bail!(
                "inline asm model `{model}` has an incompatible signature: expected {} fixed arguments, found {}{}",
                arguments.len(),
                parameters.len(),
                if typ.is_variadic_code() {
                    " plus variadic arguments"
                } else {
                    ""
                }
            );
        }
        for (index, (parameter, argument)) in parameters.iter().zip(arguments).enumerate() {
            if !Self::inline_asm_types_compatible(parameter.typ(), argument.typ()) {
                anyhow::bail!(
                    "inline asm model `{model}` argument {index} has type {:?}, expected {:?}",
                    parameter.typ(),
                    argument.typ()
                );
            }
        }
        let expected_return = if instruction.get_type().is_void_type() {
            Type::empty()
        } else {
            self.declare_instruction_result(instruction)?.typ().clone()
        };
        let actual_return = typ.return_type().cloned().unwrap_or_else(Type::empty);
        if !Self::inline_asm_types_compatible(&actual_return, &expected_return) {
            anyhow::bail!(
                "inline asm model `{model}` returns {:?}, expected {:?}",
                actual_return,
                expected_return
            );
        }
        Ok(())
    }

    fn inline_asm_types_compatible(model: &Type, asm: &Type) -> bool {
        // LLVM 22 uses opaque pointers, while pointee recovery gives the GOTO
        // representation extra local type information. Any two lowered pointer
        // types therefore correspond to the same LLVM `ptr` signature type.
        model == asm || (model.is_pointer() && asm.is_pointer())
    }

    fn instruction_location(&self, instruction: InstructionValue<'ctx>) -> Location {
        util::instruction_location(instruction, &self.function_name)
    }

    fn lowered_return_type(&self) -> Type {
        self.symbols
            .lookup(self.function_name.as_str())
            .and_then(|symbol| symbol.typ.return_type().cloned())
            .unwrap_or_else(Type::empty)
    }

    fn block_location(&self, block: BasicBlock<'ctx>) -> Location {
        for instruction in block.get_instructions() {
            let location = self.instruction_location(instruction);
            if !location.is_none() {
                return location;
            }
        }
        Location::none()
    }

    fn function_location(&self) -> Location {
        util::function_location(self.function)
    }

    fn current_location(&self) -> Location {
        self.current_block
            .map(|block| self.block_location(block))
            .unwrap_or_else(|| self.function_location())
    }

    fn signed_view(&self, expr: Expr) -> Result<Expr> {
        // LLVM `i1` currently lowers to `c_bool` for general scalar use, but signed LLVM
        // operations still interpret that value as a 1-bit two's-complement integer where
        // `true` is `-1` and `false` is `0`. Keep `bool` here as a defensive fallback in case
        // an earlier lowering step already normalized the value to CBMC's native boolean type.
        let typ = expr
            .typ()
            .to_signed()
            .or_else(|| {
                if expr.typ().is_c_bool() || expr.typ().is_bool() {
                    Some(Type::signed_int(1_u32))
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::Instruction,
                    format!(
                        "expected signed-capable integer type in {}",
                        self.function_name
                    ),
                )
                .with_symbol(self.function_name.clone())
            })?;
        Ok(expr.cast_to(typ))
    }

    fn const_field_index(&self, expr: &Expr, typ: &Type) -> Result<usize> {
        let index = expr.int_constant_value().ok_or_else(|| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "only constant struct GEP indices are currently supported in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
        })?;
        let index = usize::try_from(index).map_err(|_| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("invalid negative GEP field index in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
        })?;
        if typ
            .lookup_field(format!("field{index}"), self.symbols)
            .is_none()
        {
            return Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!(
                    "GEP field index {index} out of range in {}",
                    self.function_name
                ),
            )
            .with_symbol(self.function_name.clone())
            .into());
        }
        Ok(index)
    }

    fn block_label(&self, block: BasicBlock<'ctx>) -> Result<&str> {
        self.block_labels
            .get(&block)
            .map(String::as_str)
            .ok_or_else(|| {
                Diagnostic::unsupported_feature(
                    UnsupportedFeature::ControlFlow,
                    format!("missing basic block label in {}", self.function_name),
                )
                .with_symbol(self.function_name.clone())
                .into()
            })
    }

    fn insert_pending_aggregate_symbols(&mut self) {
        // Body lowering can discover fresh aggregate tags (for example anonymous struct
        // temporaries). Flush them immediately so subsequent expression construction can
        // resolve their fields through the symbol table.
        for symbol in self.types.take_aggregate_symbols() {
            if self.symbols.lookup(symbol.name).is_none() {
                self.symbols.insert(symbol);
            }
        }
    }
}

/// Recognize Rust's `core::panicking::*` and `core::slice::index::slice_index_fail` ABI entry
/// points across both v0 (`_R...`) and legacy (`_ZN...E`) symbol mangling.
fn is_rust_panic_symbol(name: &str) -> bool {
    if let Some(rest) = name
        .strip_prefix("_ZN4core")
        .and_then(|s| s.strip_suffix('E'))
    {
        return is_rust_core_panic_path(rest);
    }
    let Some(rest) = name.strip_prefix("_R") else {
        return false;
    };
    let rest = rest.strip_prefix('I').unwrap_or(rest);
    let Some(mut rest) = rest.strip_prefix("NvNt") else {
        return false;
    };
    while let Some(next) = rest.strip_prefix("Nt") {
        rest = next;
    }
    let Some(mut rest) = rest.strip_prefix('C') else {
        return false;
    };
    if let Some(after_s) = rest.strip_prefix('s') {
        let Some((_, after_underscore)) = after_s.split_once('_') else {
            return false;
        };
        rest = after_underscore;
    }
    rest.strip_prefix("4core")
        .is_some_and(is_rust_core_panic_path)
}

fn is_rust_core_panic_path(rest: &str) -> bool {
    if let Some(tail) = rest.strip_prefix("9panicking") {
        return tail.as_bytes().first().is_some_and(u8::is_ascii_digit);
    }
    rest.strip_prefix("5slice5index16slice_index_fail")
        .is_some_and(|tail| tail.is_empty() || tail.starts_with("17h"))
}
