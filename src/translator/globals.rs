// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026, Marco Elver <me@marcoelver.com>

//! Global-variable lowering and constant initializer handling.
//!
//! Global lowering is also the shared lowering path for LLVM constant
//! expressions. Keeping those cases in one place avoids a second,
//! partially-overlapping constant evaluator for globals versus instruction
//! operands.

use cprover_bindings::goto_program::{Expr, Location, SymbolTable, Type};
use inkwell::llvm_sys::LLVMOpcode;
use inkwell::llvm_sys::core::{
    LLVMAliasGetAliasee, LLVMGetAggregateElement, LLVMGetConstOpcode, LLVMGetNumOperands,
    LLVMGetOperand, LLVMIsAConstantDataSequential, LLVMIsAConstantExpr, LLVMIsAGlobalAlias,
    LLVMIsAGlobalVariable,
};
use inkwell::values::{
    ArrayValue, AsValueRef, BasicValueEnum, FloatValue, FunctionValue, GlobalValue, IntValue,
    PointerValue, StructValue, VectorValue,
};

use super::Diagnostic;
use super::types::TypeLowerer;

/// Lowers LLVM global initializers into GOTO expressions.
///
/// Unsupported constant-expression forms fail closed here instead of being
/// pre-classified elsewhere, so the supported surface is defined by the code
/// that actually lowers it.
#[derive(Debug, Default)]
pub struct GlobalLowerer;

impl GlobalLowerer {
    /// Lower one LLVM constant initializer.
    pub fn lower_initializer<'ctx>(
        &mut self,
        symbol: &str,
        value: BasicValueEnum<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        if is_constant_expr(value.as_value_ref()) {
            return self.lower_const_expr(symbol, value, typ, lowerer, symbols, location);
        }

        match value {
            BasicValueEnum::IntValue(value) => self.lower_int(symbol, value, typ),
            BasicValueEnum::FloatValue(value) => self.lower_float(symbol, value),
            BasicValueEnum::PointerValue(value) => {
                self.lower_pointer(symbol, value, typ, lowerer, symbols, location)
            }
            BasicValueEnum::ArrayValue(value) => {
                self.lower_array(symbol, value, typ, lowerer, symbols, location)
            }
            BasicValueEnum::StructValue(value) => {
                self.lower_struct(symbol, value, typ, lowerer, symbols, location)
            }
            BasicValueEnum::VectorValue(value) => {
                self.lower_vector(symbol, value, typ, lowerer, symbols, location)
            }
            BasicValueEnum::ScalableVectorValue(_) => {
                Err(self.unsupported(symbol, "unsupported scalable vector initializer"))
            }
        }
    }

    fn lower_const_expr<'ctx>(
        &mut self,
        symbol: &str,
        value: BasicValueEnum<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        // Reuse initializer lowering recursively for constant-expression operands so globals,
        // constant call arguments, and other folded LLVM values share one fail-closed path.
        let raw = value.as_value_ref();
        let operands = self.value_operands(raw);

        match unsafe { LLVMGetConstOpcode(raw) } {
            LLVMOpcode::LLVMBitCast
            | LLVMOpcode::LLVMAddrSpaceCast
            | LLVMOpcode::LLVMPtrToInt
            | LLVMOpcode::LLVMIntToPtr
            | LLVMOpcode::LLVMTrunc => Ok(self
                .lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?
                .cast_to(typ.clone())),
            LLVMOpcode::LLVMAdd => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).plus(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMSub => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).sub(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMXor => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).bitxor(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMAnd => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).bitand(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMOr => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).bitor(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMShl => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).shl(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMLShr => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).lshr(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMAShr => {
                let lhs =
                    self.lower_const_operand(symbol, &operands, 0, lowerer, symbols, location)?;
                let rhs =
                    self.lower_const_operand(symbol, &operands, 1, lowerer, symbols, location)?;
                Ok(lhs.cast_to(typ.clone()).ashr(rhs.cast_to(typ.clone())))
            }
            LLVMOpcode::LLVMGetElementPtr => match value {
                BasicValueEnum::PointerValue(value) => {
                    self.lower_const_gep(symbol, value, typ, lowerer, symbols, location)
                }
                _ => Err(self.unsupported(symbol, "non-pointer GEP constant expression")),
            },
            opcode => Err(self.unsupported(
                symbol,
                format!("unsupported constant expression opcode {opcode:?}"),
            )),
        }
    }

    fn lower_const_operand<'ctx>(
        &mut self,
        symbol: &str,
        operands: &[BasicValueEnum<'ctx>],
        index: usize,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        let operand = operands
            .get(index)
            .copied()
            .ok_or_else(|| self.unsupported(symbol, "malformed constant expression"))?;
        let operand_type = lowerer.lower_basic(operand.get_type())?;
        self.lower_initializer(symbol, operand, &operand_type, lowerer, symbols, location)
    }

    fn lower_int<'ctx>(
        &self,
        symbol: &str,
        value: IntValue<'ctx>,
        typ: &Type,
    ) -> std::result::Result<Expr, Diagnostic> {
        let bits = value.get_type().get_bit_width();

        if value.is_null() || value.is_undef() {
            return Ok(typ.zero());
        }

        if bits == 1 {
            return Ok(Expr::c_bool_constant(
                value
                    .get_zero_extended_constant()
                    .ok_or_else(|| self.unsupported(symbol, "expected constant i1"))?
                    != 0,
            ));
        }

        Ok(Expr::int_constant(
            value
                .get_zero_extended_constant()
                .ok_or_else(|| self.unsupported(symbol, "expected constant integer initializer"))?,
            typ.clone(),
        ))
    }

    fn lower_float<'ctx>(
        &self,
        symbol: &str,
        value: FloatValue<'ctx>,
    ) -> std::result::Result<Expr, Diagnostic> {
        if value.is_null() || value.is_undef() {
            return Ok(match value.get_type().get_bit_width() {
                16 => Type::float16().zero(),
                32 => Type::float().zero(),
                64 => Type::double().zero(),
                128 => Type::float128().zero(),
                bits => {
                    return Err(self
                        .unsupported(symbol, format!("unsupported floating-point width {bits}")));
                }
            });
        }

        let (constant, _) = value
            .get_constant()
            .ok_or_else(|| self.unsupported(symbol, "expected constant float initializer"))?;

        match value.get_type().get_bit_width() {
            32 => Ok(Expr::float_constant(constant as f32)),
            64 => Ok(Expr::double_constant(constant)),
            bits => Err(self.unsupported(
                symbol,
                format!("unsupported non-zero floating-point initializer width {bits}"),
            )),
        }
    }

    fn lower_pointer<'ctx>(
        &mut self,
        symbol: &str,
        value: PointerValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        if value.is_null() || value.is_undef() {
            return Ok(typ.zero());
        }

        if let Some(expr) = self.lower_symbol_address(value, typ, lowerer, location)? {
            return Ok(expr);
        }

        if is_constant_expr(value.as_value_ref()) {
            return self.lower_const_pointer_expr(symbol, value, typ, lowerer, symbols, location);
        }

        Err(self.unsupported(symbol, "unsupported non-null pointer initializer"))
    }

    fn lower_array<'ctx>(
        &mut self,
        symbol: &str,
        value: ArrayValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        if value.is_null() || value.is_undef() {
            return Ok(typ.zero_initializer(symbols));
        }

        let elem_ll = value.get_type().get_element_type();
        if elem_ll.is_int_type()
            && elem_ll.into_int_type().get_bit_width() == 8
            && let Some(bytes) = value.as_const_string()
        {
            let elem_type = lowerer.lower_basic(value.get_type().get_element_type())?;
            let size = value.get_type().len() as usize;
            let mut elems = Vec::with_capacity(size);

            for &byte in bytes.iter().take(size) {
                elems.push(Expr::int_constant(byte, elem_type.clone()));
            }
            while elems.len() < size {
                elems.push(elem_type.zero());
            }

            return Ok(Expr::array_expr(typ.clone(), elems));
        }

        let elem_type = lowerer.lower_basic(value.get_type().get_element_type())?;
        let elems = if unsafe { !LLVMIsAConstantDataSequential(value.as_value_ref()).is_null() } {
            (0..value.get_type().len())
                .map(|index| {
                    let elem = self.aggregate_element(symbol, value.as_value_ref(), index)?;
                    self.lower_initializer(symbol, elem, &elem_type, lowerer, symbols, location)
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            self.value_operands(value.as_value_ref())
                .into_iter()
                .map(|elem| {
                    self.lower_initializer(symbol, elem, &elem_type, lowerer, symbols, location)
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        if elems.len() == value.get_type().len() as usize {
            Ok(Expr::array_expr(typ.clone(), elems))
        } else {
            Err(self.unsupported(symbol, "unsupported non-string array initializer"))
        }
    }

    fn lower_struct<'ctx>(
        &mut self,
        symbol: &str,
        value: StructValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        if value.is_null() || value.is_undef() {
            return Ok(typ.zero_initializer(symbols));
        }

        let fields = value
            .get_fields()
            .map(|field| {
                let field_type = lowerer.lower_basic(field.get_type())?;
                self.lower_initializer(symbol, field, &field_type, lowerer, symbols, location)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Expr::struct_expr_from_values(typ.clone(), fields, symbols))
    }

    fn lower_vector<'ctx>(
        &mut self,
        symbol: &str,
        value: VectorValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        if value.is_null() || value.is_undef() {
            return Ok(typ.zero_initializer(symbols));
        }

        let elem_type = lowerer.lower_basic(value.get_type().get_element_type())?;
        let elems = (0..value.get_type().get_size())
            .map(|index| {
                self.lower_initializer(
                    symbol,
                    value.get_element_as_constant(index),
                    &elem_type,
                    lowerer,
                    symbols,
                    location,
                )
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Expr::vector_expr(typ.clone(), elems))
    }

    fn unsupported(&self, symbol: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::unsupported_initializer(message).with_symbol(symbol)
    }

    fn lower_symbol_address<'ctx>(
        &mut self,
        value: PointerValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        location: &Location,
    ) -> std::result::Result<Option<Expr>, Diagnostic> {
        let mut raw = value.as_value_ref();
        while unsafe { !LLVMIsAGlobalAlias(raw).is_null() } {
            // Clang emits common C++ entry-point aliases like `D1 -> D2`. Follow aliases to their
            // aliasee so the rest of constant-pointer lowering can stay in terms of real symbols.
            raw = unsafe { LLVMAliasGetAliasee(raw) };
        }

        if let Some(function) = unsafe { FunctionValue::new(raw) } {
            let name = function.get_name().to_string_lossy().into_owned();
            let code = lowerer.lower_function(function.get_type())?;
            return Ok(Some(
                Expr::symbol_expression(name, code)
                    .with_location(*location)
                    .address_of()
                    .with_location(*location)
                    .cast_to(typ.clone()),
            ));
        }

        if unsafe { !LLVMIsAGlobalVariable(raw).is_null() } {
            let global = unsafe { GlobalValue::new(raw) };
            let name = global.get_name().to_string_lossy().into_owned();
            let object_type = lowerer.lower_any(global.get_value_type())?;
            return Ok(Some(
                Expr::symbol_expression(name, object_type)
                    .with_location(*location)
                    .address_of()
                    .with_location(*location)
                    .cast_to(typ.clone()),
            ));
        }

        Ok(None)
    }

    fn lower_const_pointer_expr<'ctx>(
        &mut self,
        symbol: &str,
        value: PointerValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        match unsafe { LLVMGetConstOpcode(value.as_value_ref()) } {
            LLVMOpcode::LLVMBitCast | LLVMOpcode::LLVMAddrSpaceCast => {
                let operand = self
                    .value_operands(value.as_value_ref())
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        self.unsupported(symbol, "malformed pointer cast initializer")
                    })?;
                let operand = match operand {
                    BasicValueEnum::PointerValue(operand) => operand,
                    _ => {
                        return Err(
                            self.unsupported(symbol, "unsupported non-pointer cast initializer")
                        );
                    }
                };
                Ok(self
                    .lower_pointer(symbol, operand, typ, lowerer, symbols, location)?
                    .cast_to(typ.clone()))
            }
            LLVMOpcode::LLVMGetElementPtr => {
                self.lower_const_gep(symbol, value, typ, lowerer, symbols, location)
            }
            opcode => Err(self.unsupported(
                symbol,
                format!("unsupported constant pointer expression opcode {opcode:?}"),
            )),
        }
    }

    fn lower_const_gep<'ctx>(
        &mut self,
        symbol: &str,
        value: PointerValue<'ctx>,
        typ: &Type,
        lowerer: &mut TypeLowerer,
        symbols: &SymbolTable,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        let operands = self.value_operands(value.as_value_ref());
        let (base, indexes) = operands
            .split_first()
            .ok_or_else(|| self.unsupported(symbol, "malformed constant GEP initializer"))?;
        let mut current = self.lower_gep_root(symbol, *base, lowerer, location)?;

        for (position, index) in indexes.iter().enumerate() {
            let index = self.constant_index(symbol, *index)?;
            let current_type = current.typ().clone();
            let idx_expr = Expr::int_constant(index as u64, Type::ssize_t());

            if position == 0 {
                if index == 0 {
                    continue;
                }
                // LLVM uses a leading non-zero GEP index to step across array roots such as
                // vtables and RTTI objects. Treat that first hop like ordinary array indexing
                // instead of rejecting it as "not a field access".
                if current_type.is_array_like() || current_type.is_vector() {
                    current = current.index(idx_expr);
                    continue;
                }
                return Err(
                    self.unsupported(symbol, "unsupported non-zero root index in constant GEP")
                );
            }

            if current_type.is_array_like() || current_type.is_vector() {
                current = current.index(idx_expr);
            } else if current_type.is_struct_like() {
                current = current.member(format!("field{index}"), symbols);
            } else {
                return Err(self.unsupported(
                    symbol,
                    format!("unsupported constant GEP step on type {current_type:?}"),
                ));
            }
        }

        Ok(current
            .address_of()
            .with_location(*location)
            .cast_to(typ.clone()))
    }

    fn lower_gep_root<'ctx>(
        &mut self,
        symbol: &str,
        base: BasicValueEnum<'ctx>,
        lowerer: &mut TypeLowerer,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        match base {
            BasicValueEnum::PointerValue(base) => {
                self.lower_gep_root_pointer(symbol, base, lowerer, location)
            }
            _ => Err(self.unsupported(symbol, "unsupported constant GEP base")),
        }
    }

    fn lower_gep_root_pointer<'ctx>(
        &mut self,
        symbol: &str,
        value: PointerValue<'ctx>,
        lowerer: &mut TypeLowerer,
        location: &Location,
    ) -> std::result::Result<Expr, Diagnostic> {
        let raw = value.as_value_ref();

        if unsafe { !LLVMIsAGlobalVariable(raw).is_null() } {
            let global = unsafe { GlobalValue::new(raw) };
            let name = global.get_name().to_string_lossy().into_owned();
            let object_type = lowerer.lower_any(global.get_value_type())?;
            return Ok(Expr::symbol_expression(name, object_type).with_location(*location));
        }

        if is_constant_expr(value.as_value_ref()) {
            match unsafe { LLVMGetConstOpcode(raw) } {
                LLVMOpcode::LLVMBitCast | LLVMOpcode::LLVMAddrSpaceCast => {
                    let operand =
                        self.value_operands(raw).into_iter().next().ok_or_else(|| {
                            self.unsupported(symbol, "malformed constant GEP base")
                        })?;
                    return self.lower_gep_root(symbol, operand, lowerer, location);
                }
                _ => {}
            }
        }

        Err(self.unsupported(symbol, "unsupported constant GEP base"))
    }

    fn value_operands<'ctx>(
        &self,
        value: inkwell::llvm_sys::prelude::LLVMValueRef,
    ) -> Vec<BasicValueEnum<'ctx>> {
        (0..unsafe { LLVMGetNumOperands(value) as u32 })
            .map(|index| unsafe { BasicValueEnum::new(LLVMGetOperand(value, index)) })
            .collect()
    }

    fn aggregate_element<'ctx>(
        &self,
        symbol: &str,
        value: inkwell::llvm_sys::prelude::LLVMValueRef,
        index: u32,
    ) -> std::result::Result<BasicValueEnum<'ctx>, Diagnostic> {
        let elem = unsafe { LLVMGetAggregateElement(value, index) };
        if elem.is_null() {
            Err(self.unsupported(symbol, "missing aggregate element"))
        } else {
            Ok(unsafe { BasicValueEnum::new(elem) })
        }
    }

    fn constant_index<'ctx>(
        &self,
        symbol: &str,
        value: BasicValueEnum<'ctx>,
    ) -> std::result::Result<u32, Diagnostic> {
        let BasicValueEnum::IntValue(value) = value else {
            return Err(self.unsupported(symbol, "expected integer constant GEP index"));
        };
        let index = value
            .get_sign_extended_constant()
            .ok_or_else(|| self.unsupported(symbol, "expected constant GEP index"))?;
        if index < 0 {
            return Err(self.unsupported(symbol, "unsupported negative constant GEP index"));
        }
        u32::try_from(index)
            .map_err(|_| self.unsupported(symbol, "constant GEP index out of range"))
    }
}

fn is_constant_expr(value: inkwell::llvm_sys::prelude::LLVMValueRef) -> bool {
    unsafe { !LLVMIsAConstantExpr(value).is_null() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translator::types::TypeLowerer;
    use cprover_bindings::goto_program::SymbolTable;
    use inkwell::context::Context;
    use inkwell::values::BasicValue;

    #[test]
    fn lowers_integer_initializer() {
        let context = Context::create();
        let value = context
            .i32_type()
            .const_int(42, false)
            .as_basic_value_enum();
        let mut lowerer = TypeLowerer::default();
        let expr = GlobalLowerer
            .lower_initializer(
                "g",
                value,
                &Type::unsigned_int(32_u32),
                &mut lowerer,
                &SymbolTable::new(crate::util::machine_model(
                    crate::MachineModel::Lp64,
                    "x86_64".to_owned(),
                    false,
                )),
                &Location::none(),
            )
            .expect("failed to lower integer initializer");

        assert_eq!(expr.int_constant_value().expect("constant"), 42.into());
    }
}
