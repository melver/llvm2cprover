// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026, Marco Elver <me@marcoelver.com>

//! LLVM instruction lowering into GOTO function bodies.
//!
//! This module owns actual executable lowering:
//!
//! - SSA values become declared temporaries
//! - PHI nodes are lowered on predecessor edges
//! - basic blocks become labeled GOTO blocks
//! - contracts are injected at call sites, not parsed as a separate DSL
//!
//! Unsupported instructions fail where they are encountered, which keeps
//! diagnostics specific and avoids a second feature-classification pass.

use std::{collections::HashMap, slice};

use anyhow::Result;
use cprover_bindings::goto_program::{
    BuiltinFn, Expr, ExprValue, Location, Stmt, SwitchCase, Symbol, SymbolTable, Type,
    arithmetic_overflow_result_type,
};
use inkwell::AtomicRMWBinOp;
use inkwell::FloatPredicate;
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::llvm_sys::core::{
    LLVMGetCalledFunctionType, LLVMGetCalledValue, LLVMGetClause, LLVMGetInlineAsmAsmString,
    LLVMGetInlineAsmCanUnwind, LLVMGetMaskValue, LLVMGetNormalDest, LLVMGetNumArgOperands,
    LLVMGetNumClauses, LLVMGetNumMaskElements, LLVMGetSwitchCaseValue, LLVMGetUnwindDest,
    LLVMIsAInlineAsm, LLVMIsCleanup,
};
use inkwell::llvm_sys::prelude::LLVMValueRef;
use inkwell::types::FunctionType;
use inkwell::values::{
    AsValueRef, BasicValue, BasicValueEnum, CallSiteValue, FunctionValue, InstructionOpcode,
    InstructionValue, PhiValue,
};

use super::globals::GlobalLowerer;
use super::types::TypeLowerer;
use super::{Diagnostic, EH_ACTIVE_SYMBOL, EH_PTR_SYMBOL, EH_SELECTOR_SYMBOL, UnsupportedFeature};
use crate::contracts::{ContractMap, FunctionContract};
use crate::util;

/// Lowers supported LLVM function bodies into GOTO statements.
///
/// The lowerer is shared across functions but all per-function state lives in
/// the internal [`FunctionLowerer`].
#[derive(Debug)]
pub struct InstructionLowerer<'a> {
    contracts: &'a ContractMap,
    unsound: bool,
}

impl<'a> InstructionLowerer<'a> {
    /// Create an instruction lowerer with the active contract map.
    pub fn new(contracts: &'a ContractMap, unsound: bool) -> Self {
        Self { contracts, unsound }
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
            self.unsound,
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
    unsound: bool,
    suppress_contracts: bool,
    block_labels: HashMap<BasicBlock<'ctx>, String>,
    current_block: Option<BasicBlock<'ctx>>,
    values: HashMap<usize, Expr>,
    aggregates: HashMap<usize, Vec<Expr>>,
    prologue: Vec<Stmt>,
    next_tmp: usize,
}

impl<'a, 'ctx, 'c> FunctionLowerer<'a, 'ctx, 'c> {
    fn new(
        function: FunctionValue<'ctx>,
        symbols: &'a mut SymbolTable,
        types: &'a mut TypeLowerer,
        globals: &'a mut GlobalLowerer,
        contracts: &'c ContractMap,
        unsound: bool,
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
            unsound,
            block_labels: HashMap::new(),
            current_block: None,
            values: HashMap::new(),
            aggregates: HashMap::new(),
            prologue: Vec::new(),
            next_tmp: 0,
        }
    }

    fn lower(mut self) -> Result<()> {
        self.declare_parameters()?;
        let blocks = self.function.get_basic_blocks();
        if blocks.is_empty() {
            return Ok(());
        }
        self.init_block_labels(&blocks);

        self.declare_phi_results()?;
        let mut body = Vec::new();
        for block in blocks {
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
        for (index, param) in self.function.get_params().into_iter().enumerate() {
            let typ = self.types.lower_parameter(param)?;
            self.insert_pending_aggregate_symbols();
            let base =
                self.value_base_name(index, "arg", param.get_name().to_string_lossy().as_ref());
            let expr = self.insert_local_symbol(&base, typ, true, false, self.function_location());
            self.values.insert(param.as_value_ref() as usize, expr);
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
            InstructionOpcode::Br => self.lower_branch(instruction, body),
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
            InstructionOpcode::Select => self.lower_select(instruction, body),
            InstructionOpcode::Return => self.lower_return(instruction, body),
            InstructionOpcode::Call => self.lower_call(instruction, body),
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
            InstructionOpcode::Shl => lhs.cast_to(target.clone()).shl(rhs.cast_to(target.clone())),
            InstructionOpcode::LShr => lhs
                .cast_to(target.clone())
                .lshr(rhs.cast_to(target.clone())),
            InstructionOpcode::AShr => self
                .signed_view(lhs)?
                .ashr(rhs.cast_to(target.clone()))
                .cast_to(target.clone()),
            _ => unreachable!("binary opcode dispatched incorrectly"),
        };
        body.push(Stmt::assign(result, value, loc));
        Ok(())
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
                // The LLVM C API returns branch successors in reverse order:
                // operand 1 = false destination, operand 2 = true destination.
                let false_dest = self.operand_block(
                    instruction,
                    1,
                    UnsupportedFeature::ControlFlow,
                    "malformed false branch target",
                )?;
                let true_dest = self.operand_block(
                    instruction,
                    2,
                    UnsupportedFeature::ControlFlow,
                    "malformed true branch target",
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
        let (lhs, rhs) = self.normalize_icmp_operands(lhs, rhs);
        let result = self.declare_instruction_result(instruction)?;
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
        let _ = body;
        let loc = self.instruction_location(instruction);
        let count = instruction
            .get_operand(0)
            .and_then(|operand| operand.value())
            .map(|value| self.const_usize(value))
            .transpose()?
            .unwrap_or(1);
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
        if count != 1 {
            allocated = allocated.array_of(count);
        }

        let slot = self.declare_hidden_local("alloca", allocated, loc);
        let result_type = self.types.lower_instruction_result(instruction)?;
        // Model `%p = alloca T` as a hidden local `T slot;` plus the SSA value `%p = &slot`.
        // That matches the shape CBMC's C frontend emits for ordinary stack locals and keeps
        // later load/store simplifications local instead of routing everything through a
        // synthetic pointer temporary.
        self.values.insert(
            instruction.as_value_ref() as usize,
            slot.address_of().cast_to(result_type).with_location(loc),
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
        let result = self.declare_instruction_result(instruction)?;
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
            InstructionOpcode::ZExt
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
            | InstructionOpcode::UIToFP => value.cast_to(result.typ().clone()),
            _ => unreachable!("cast opcode dispatched incorrectly"),
        };
        body.push(Stmt::assign(result, cast, loc));
        Ok(())
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

    fn lower_return(
        &mut self,
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
    ) -> Result<()> {
        let loc = self.instruction_location(instruction);
        let value = (instruction.get_num_operands() != 0)
            .then(|| self.lower_operand(instruction, 0))
            .transpose()?;
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

        let args = (0..call.count_arguments())
            .map(|index| self.lower_operand(instruction, index))
            .collect::<Result<Vec<_>>>()?;
        let raw_callee = unsafe { LLVMGetCalledValue(call.as_value_ref()) };
        if self.try_lower_inline_asm_call(instruction, raw_callee, body)? {
            return Ok(());
        }
        if let Some(callee) = call.get_called_fn_value() {
            let callee_name = callee.get_name().to_string_lossy().into_owned();
            if self.try_lower_special_call(instruction, &callee_name, &args, body)? {
                return Ok(());
            }

            let contract = (!self.suppress_contracts)
                .then(|| self.contracts.function(callee_name.as_str()))
                .flatten()
                .cloned();
            if let Some(contract) = &contract {
                self.lower_requires_helper(contract, &callee_name, &args, body, loc)?;
            }

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
            if let Some(contract) = &contract {
                self.lower_ensures_helper(contract, &callee_name, &args, instruction, body, loc)?;
            }
            return Ok(());
        }

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
            if self.try_lower_special_call(instruction, &callee_name, &args, body)? {
                body.push(self.branch_on_exception(predecessor, normal_dest, unwind_dest)?);
                return Ok(());
            }

            let contract = (!self.suppress_contracts)
                .then(|| self.contracts.function(callee_name.as_str()))
                .flatten()
                .cloned();
            if let Some(contract) = &contract {
                self.lower_requires_helper(contract, &callee_name, &args, body, loc)?;
            }

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

        let loc = self.instruction_location(instruction);
        let asm = self.inline_asm_string(raw_callee);
        if asm.is_empty() && unsafe { LLVMGetInlineAsmCanUnwind(raw_callee) == 0 } {
            // Empty inline asm with only clobbers is a compiler barrier, not a stateful machine
            // instruction. Lowering it away keeps verifier state unchanged without pretending to
            // understand real target assembly.
            if instruction.get_type().is_void_type() {
                return Ok(true);
            }
        }

        if self.unsound {
            if instruction.get_type().is_void_type() {
                return Ok(true);
            }
            let lhs = self.declare_instruction_result(instruction)?;
            body.push(Stmt::assign(
                lhs.clone(),
                Expr::nondet(lhs.typ().clone()),
                loc,
            ));
            return Ok(true);
        }

        Err(Diagnostic::unsupported_feature(
            UnsupportedFeature::InlineAsm,
            format!("unsupported inline asm call in {}", self.function_name),
        )
        .with_symbol(self.function_name.clone())
        .with_location(loc)
        .into())
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
        instruction: InstructionValue<'ctx>,
        body: &mut Vec<Stmt>,
        loc: Location,
    ) -> Result<()> {
        let Some(helper) = contract.ensures.as_deref() else {
            return Ok(());
        };
        let mut helper_args = args.to_vec();
        if !instruction.get_type().is_void_type() {
            helper_args.push(self.declare_instruction_result(instruction)?);
        }
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
            body.push(Stmt::assert(cond, "assert", "__CPROVER_assert", loc));
            return Ok(true);
        }

        if callee_name.starts_with("__CPROVER_nondet_") {
            let lhs = self.declare_instruction_result(instruction)?;
            body.push(Stmt::assign(
                lhs.clone(),
                Expr::nondet(lhs.typ().clone()),
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
            self.lower_builtin_call(
                BuiltinFn::Memcpy,
                args.get(0..3).unwrap_or(&[]),
                instruction,
                body,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.memmove") || callee_name == "memmove" {
            self.lower_builtin_call(
                BuiltinFn::Memmove,
                args.get(0..3).unwrap_or(&[]),
                instruction,
                body,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.memset") {
            self.lower_builtin_call(
                BuiltinFn::Memset,
                args.get(0..3).unwrap_or(&[]),
                instruction,
                body,
            )?;
            return Ok(true);
        }

        if callee_name.starts_with("llvm.lifetime.start")
            || callee_name.starts_with("llvm.lifetime.end")
        {
            // Lifetime markers only guide optimization. Dropping them keeps the memory state
            // unchanged, which is the intended Phase 1 fail-open behavior for these intrinsics.
            return Ok(true);
        }

        if callee_name == "llvm.x86.sse2.pause" {
            // `pause` is only a processor scheduling hint for spin loops; it has no observable
            // effect on program state, so lowering it to a no-op is sound.
            return Ok(true);
        }

        if self.unsound
            && (callee_name.starts_with("llvm.va_start")
                || callee_name.starts_with("llvm.va_end")
                || callee_name.starts_with("llvm.va_copy"))
        {
            // This is intentionally gated behind `--unsound`: variadic bodies that touch
            // `va_list` stay underconstrained until we model its target ABI precisely.
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
                    return Ok(Expr::nondet(elem_type.clone()));
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
        let return_type = self
            .symbols
            .lookup(self.function_name.as_str())
            .and_then(|symbol| symbol.typ.return_type().cloned())
            .unwrap_or_else(Type::empty);
        Ok(if return_type == Type::empty() {
            Stmt::ret(None, loc)
        } else {
            Stmt::ret(Some(Expr::nondet(return_type)), loc)
        })
    }

    fn lower_value(&mut self, value: BasicValueEnum<'ctx>) -> Result<Expr> {
        if let Some(expr) = self.values.get(&(value.as_value_ref() as usize)) {
            return Ok(expr.clone());
        }

        let typ = self.types.lower_value(value)?;
        self.insert_pending_aggregate_symbols();
        self.globals
            .lower_initializer(
                &self.function_name,
                value,
                &typ,
                self.types,
                self.symbols,
                &self.current_location(),
            )
            .map_err(Into::into)
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
        let mut ptr = base.cast_to(current_type.clone().to_pointer());
        let indices = (1..instruction.get_num_operands())
            .map(|i| self.lower_operand(instruction, i))
            .collect::<Result<Vec<_>>>()?;

        for (position, index) in indices.into_iter().enumerate() {
            if position == 0 {
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
        let base = format!("{stem}{}", self.next_tmp);
        self.next_tmp += 1;
        let expr = self.insert_local_symbol(&base, typ, false, true, location);
        self.prologue.push(Stmt::decl(expr.clone(), None, location));
        expr
    }

    fn insert_local_symbol(
        &mut self,
        base_name: &str,
        typ: Type,
        is_parameter: bool,
        hidden: bool,
        location: Location,
    ) -> Expr {
        let name = format!("{}::{}", self.function_name, base_name);
        let symbol = Symbol::variable(name.clone(), base_name.to_owned(), typ.clone(), location)
            .with_is_parameter(is_parameter)
            .with_is_hidden(hidden)
            .with_is_state_var(false)
            .with_is_static_lifetime(false);
        self.symbols.insert(symbol);
        if is_parameter {
            self.symbols
                .insert_parameter(self.function_name.clone(), name.clone());
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
        self.lower_value(value)
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
        // Preserve direct stack-slot accesses where the LLVM pointer is just `&local`.
        // This keeps the emitted GOTO closer to CBMC's own C frontend and avoids
        // manufacturing extra dereference traffic for automatic locals.
        (slot.typ() == pointee).then_some(slot)
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

    fn unsupported_intrinsic(&self, name: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::unsupported_feature(UnsupportedFeature::Intrinsic, message)
            .with_symbol(name.to_owned())
    }

    fn inline_asm_string(&self, asm: LLVMValueRef) -> String {
        let mut len = 0;
        let ptr = unsafe { LLVMGetInlineAsmAsmString(asm, &mut len) };
        if ptr.is_null() || len == 0 {
            return String::new();
        }
        String::from_utf8_lossy(unsafe { slice::from_raw_parts(ptr.cast::<u8>(), len) })
            .into_owned()
    }

    fn instruction_location(&self, instruction: InstructionValue<'ctx>) -> Location {
        util::instruction_location(instruction, &self.function_name)
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
        let typ = expr.typ().to_signed().ok_or_else(|| {
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

    fn const_usize(&self, value: BasicValueEnum<'ctx>) -> Result<usize> {
        match value {
            BasicValueEnum::IntValue(value) => value
                .get_zero_extended_constant()
                .map(|value| value as usize)
                .ok_or_else(|| {
                    Diagnostic::unsupported_feature(
                        UnsupportedFeature::Instruction,
                        format!("expected constant integer in {}", self.function_name),
                    )
                    .with_symbol(self.function_name.clone())
                    .into()
                }),
            _ => Err(Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("expected integer operand in {}", self.function_name),
            )
            .with_symbol(self.function_name.clone())
            .into()),
        }
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
