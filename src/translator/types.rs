// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! LLVM type lowering into CBMC GOTO types.
//!
//! LLVM's type system does not map directly onto CBMC's:
//!
//! - integers are signless in LLVM but signedness matters in GOTO types
//! - opaque pointers need a recovered pointee type to avoid collapsing
//!   everything to `void *`
//! - named and anonymous aggregates must become stable GOTO type symbols
//!
//! This module handles those mismatches and records any aggregate symbols that
//! must be inserted into the symbol table before later expressions can refer to
//! them.

use std::collections::{BTreeSet, HashMap};

use cprover_bindings::goto_program::{DatatypeComponent, Symbol, Type};
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::targets::TargetData;
use inkwell::types::{AnyType, AnyTypeEnum, AsTypeRef, BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{AsValueRef, BasicValueEnum, FunctionValue, InstructionValue};

use super::Diagnostic;

#[derive(Debug, Clone)]
enum PointerHint {
    Known(Type),
    Conflict,
}

/// Lowers LLVM types and records any aggregate type symbols they require.
///
/// Pointer lowering is deliberately context-sensitive: other translation stages
/// feed back recovered pointee hints from loads, stores, GEPs, calls, and
/// returns so typed pointers can be reconstructed from opaque LLVM `ptr`
/// values.
#[derive(Debug, Default)]
pub struct TypeLowerer {
    aggregate_symbols: Vec<Symbol>,
    named_structs: BTreeSet<String>,
    anonymous_structs: HashMap<usize, String>,
    next_anonymous_struct: usize,
    pointer_hints: HashMap<usize, PointerHint>,
    function_return_hints: HashMap<usize, PointerHint>,
    target_data: Option<TargetData>,
}

impl TypeLowerer {
    /// Set the LLVM data layout used for struct padding computation.
    ///
    /// Without a data layout the lowerer emits struct fields with no explicit
    /// padding.  CBMC's static bounds checker then computes a sizeof by summing
    /// field sizes without alignment gaps, which is incorrect for structs that
    /// contain fields requiring non-trivial alignment.  Providing the module's
    /// data layout string lets the lowerer insert explicit `$pad` components
    /// that match the gaps LLVM itself would place, so CBMC sees the correct
    /// layout.
    pub fn set_target_data(&mut self, data_layout: &str) {
        self.target_data = Some(TargetData::create(data_layout));
    }

    /// Lower any LLVM type into its GOTO representation.
    pub fn lower_any<'ctx>(
        &mut self,
        typ: AnyTypeEnum<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        match typ {
            AnyTypeEnum::ArrayType(typ) => Ok(self
                .lower_basic(typ.get_element_type())?
                .array_of(typ.len())),
            AnyTypeEnum::FloatType(typ) => self.lower_float(typ.get_bit_width()),
            AnyTypeEnum::FunctionType(typ) => self.lower_function(typ),
            AnyTypeEnum::IntType(typ) => Ok(self.lower_int(typ.get_bit_width())),
            AnyTypeEnum::PointerType(_) => Ok(Type::void_pointer()),
            AnyTypeEnum::StructType(typ) => self.lower_struct(typ),
            AnyTypeEnum::VoidType(_) => Ok(Type::empty()),
            AnyTypeEnum::VectorType(typ) => Ok(Type::vector(
                self.lower_basic(typ.get_element_type())?,
                typ.get_size().into(),
            )),
            AnyTypeEnum::ScalableVectorType(_) => Err(Diagnostic::unsupported_type(
                "unsupported scalable vector type",
            )),
        }
    }

    /// Lower a basic LLVM type into its GOTO representation.
    pub fn lower_basic<'ctx>(
        &mut self,
        typ: BasicTypeEnum<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        self.lower_any(typ.as_any_type_enum())
    }

    /// Lower one SSA value type, using any recovered pointee information for pointers.
    pub fn lower_value<'ctx>(
        &mut self,
        value: BasicValueEnum<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        self.lower_basic_with_hint(
            value.get_type(),
            self.pointer_pointee(value.as_value_ref() as usize),
        )
    }

    /// Lower one instruction result type, using recovered pointee information for pointers.
    pub fn lower_instruction_result<'ctx>(
        &mut self,
        instruction: InstructionValue<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        self.lower_any_with_hint(
            instruction.get_type(),
            self.pointer_pointee(instruction.as_value_ref() as usize),
        )
    }

    /// Lower a function parameter type, using recovered pointee information for opaque pointers.
    pub fn lower_parameter<'ctx>(
        &mut self,
        value: BasicValueEnum<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        self.lower_value(value)
    }

    /// Lower the `byval(T)` pointee type for parameter `index` of `function`, if present.
    pub fn lower_param_byval<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
        index: u32,
    ) -> std::result::Result<Option<Type>, Diagnostic> {
        let kind = Attribute::get_named_enum_kind_id("byval");
        function
            .get_enum_attribute(AttributeLoc::Param(index), kind)
            .filter(|attr| attr.is_type())
            .map(|attr| self.lower_any(attr.get_type_value()))
            .transpose()
    }

    /// Lower a function return type, using recovered pointee information for opaque pointers.
    pub fn lower_function_return<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        function
            .get_type()
            .get_return_type()
            .map_or(Ok(Type::empty()), |typ| {
                self.lower_basic_with_hint(
                    typ,
                    self.function_return_pointee(function.as_value_ref() as usize),
                )
            })
    }

    /// Lower an LLVM function type into a GOTO code type.
    pub fn lower_function<'ctx>(
        &mut self,
        typ: FunctionType<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        let mut params = Vec::with_capacity(typ.count_param_types() as usize);

        for param in typ.get_param_types() {
            let basic: BasicTypeEnum<'ctx> = param
                .try_into()
                .map_err(|_| Diagnostic::unsupported_type("unsupported metadata parameter type"))?;
            params.push(self.lower_basic(basic)?);
        }

        let return_type = typ
            .get_return_type()
            .map_or(Ok(Type::empty()), |typ| self.lower_basic(typ))?;

        if typ.is_var_arg() {
            // `cprover_bindings` already models C-style variadics; keep the named prefix precise
            // and let the call-site checker admit only the extra trailing arguments.
            Ok(Type::variadic_code_with_unnamed_parameters(
                params,
                return_type,
            ))
        } else {
            Ok(Type::code_with_unnamed_parameters(params, return_type))
        }
    }

    /// Return newly discovered aggregate type symbols.
    pub fn take_aggregate_symbols(&mut self) -> Vec<Symbol> {
        std::mem::take(&mut self.aggregate_symbols)
    }

    /// Record a recovered pointee type for one LLVM value.
    pub fn note_pointer_pointee(&mut self, value: usize, pointee: Type) -> bool {
        Self::note_hint(&mut self.pointer_hints, value, pointee)
    }

    /// Record a recovered pointee type for one function's pointer return value.
    pub fn note_function_return_pointee(&mut self, function: usize, pointee: Type) -> bool {
        Self::note_hint(&mut self.function_return_hints, function, pointee)
    }

    /// Return the recovered pointee type for one LLVM value, if it is not ambiguous.
    pub fn pointer_pointee(&self, value: usize) -> Option<Type> {
        self.pointer_hints.get(&value).and_then(Self::known_hint)
    }

    /// Return the recovered pointee type for one function's pointer return, if known.
    pub fn function_return_pointee(&self, function: usize) -> Option<Type> {
        self.function_return_hints
            .get(&function)
            .and_then(Self::known_hint)
    }

    fn lower_float(&self, bits: u32) -> std::result::Result<Type, Diagnostic> {
        match bits {
            16 => Ok(Type::float16()),
            32 => Ok(Type::float()),
            64 => Ok(Type::double()),
            128 => Ok(Type::float128()),
            _ => Err(Diagnostic::unsupported_type(format!(
                "unsupported floating-point width {bits}"
            ))),
        }
    }

    fn lower_int(&self, bits: u32) -> Type {
        if bits == 1 {
            Type::c_bool()
        } else {
            Type::unsigned_int(bits)
        }
    }

    fn lower_any_with_hint<'ctx>(
        &mut self,
        typ: AnyTypeEnum<'ctx>,
        pointer_hint: Option<Type>,
    ) -> std::result::Result<Type, Diagnostic> {
        match typ {
            AnyTypeEnum::PointerType(_) => {
                Ok(pointer_hint.map_or_else(Type::void_pointer, |typ| typ.to_pointer()))
            }
            _ => self.lower_any(typ),
        }
    }

    fn lower_basic_with_hint<'ctx>(
        &mut self,
        typ: BasicTypeEnum<'ctx>,
        pointer_hint: Option<Type>,
    ) -> std::result::Result<Type, Diagnostic> {
        self.lower_any_with_hint(typ.as_any_type_enum(), pointer_hint)
    }

    fn lower_struct<'ctx>(
        &mut self,
        typ: StructType<'ctx>,
    ) -> std::result::Result<Type, Diagnostic> {
        // Lower each LLVM field to its GOTO type first so we can query
        // target_data for field offsets and sizes without borrow conflicts.
        let field_types = typ.get_field_types();
        let goto_fields: Vec<Type> = field_types
            .iter()
            .map(|&f| self.lower_basic(f))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Build components, inserting explicit padding wherever the data layout
        // leaves a gap between consecutive fields (and after the last field).
        // Without explicit padding CBMC's static sizeof sums field widths with
        // no alignment gaps, producing a value smaller than the real struct
        // size; its bounds checker then statically proves byte-offset accesses
        // are out-of-range and emits `ASSERT false`.
        let nfields = field_types.len();
        let mut components: Vec<DatatypeComponent> = Vec::new();
        let mut pad_counter: usize = 0;

        if let Some(td) = &self.target_data {
            let mut cursor: u64 = 0; // running byte offset
            for i in 0..nfields {
                let field_offset = td.offset_of_element(&typ, i as u32).unwrap_or(cursor);
                // Insert padding between cursor and this field's actual start.
                if field_offset > cursor {
                    let pad_bits = (field_offset - cursor) * 8;
                    components.push(DatatypeComponent::padding(
                        format!("$pad{pad_counter}"),
                        pad_bits,
                    ));
                    pad_counter += 1;
                }
                components.push(DatatypeComponent::field(
                    format!("field{i}"),
                    goto_fields[i].clone(),
                ));
                cursor = field_offset + td.get_abi_size(&field_types[i]);
            }
            // Trailing padding to reach the struct's total ABI size.
            let struct_size = td.get_abi_size(&typ);
            if struct_size > cursor {
                let pad_bits = (struct_size - cursor) * 8;
                components.push(DatatypeComponent::padding(
                    format!("$pad{pad_counter}"),
                    pad_bits,
                ));
            }
        } else {
            // No data layout available; emit fields without padding.  CBMC
            // will compute an incorrect sizeof for structs with alignment gaps,
            // but this path is only hit for unusual translation configurations.
            components = goto_fields
                .into_iter()
                .enumerate()
                .map(|(index, goto_type)| {
                    DatatypeComponent::field(format!("field{index}"), goto_type)
                })
                .collect();
        }

        let tag = match typ.get_name().and_then(|name| name.to_str().ok()) {
            Some(name) => {
                let tag = name.to_owned();
                if self.named_structs.insert(tag.clone()) {
                    self.aggregate_symbols.push(Symbol::struct_type(
                        tag.clone(),
                        tag.clone().into(),
                        components.clone(),
                    ));
                }
                tag
            }
            None => {
                let key = typ.as_type_ref() as usize;
                // Anonymous LLVM structs still need stable tags on the GOTO side so later
                // lookups (for example `cmpxchg` result reconstruction) see one type.
                if let Some(tag) = self.anonymous_structs.get(&key) {
                    return Ok(Type::struct_tag(tag.clone()));
                }

                let tag = format!("anon.struct.{}", self.next_anonymous_struct);
                self.next_anonymous_struct += 1;
                self.anonymous_structs.insert(key, tag.clone());
                self.aggregate_symbols.push(Symbol::struct_type(
                    tag.clone(),
                    tag.clone().into(),
                    components,
                ));
                tag
            }
        };

        Ok(Type::struct_tag(tag))
    }

    fn note_hint(hints: &mut HashMap<usize, PointerHint>, key: usize, pointee: Type) -> bool {
        match hints.get_mut(&key) {
            None => {
                hints.insert(key, PointerHint::Known(pointee));
                true
            }
            Some(PointerHint::Known(existing)) if *existing == pointee => false,
            Some(PointerHint::Conflict) => false,
            Some(slot) => {
                // Conflicting recovery sites fall back to `void *` by marking the hint
                // ambiguous instead of picking one arbitrary pointee type.
                *slot = PointerHint::Conflict;
                true
            }
        }
    }

    fn known_hint(hint: &PointerHint) -> Option<Type> {
        match hint {
            PointerHint::Known(typ) => Some(typ.clone()),
            PointerHint::Conflict => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use inkwell::types::AnyType;
    use inkwell::values::BasicValue;

    #[test]
    fn lowers_i32_to_unsigned_bv() {
        let context = Context::create();
        let lowered = TypeLowerer::default()
            .lower_any(context.i32_type().as_any_type_enum())
            .expect("failed to lower i32");

        assert_eq!(lowered, Type::unsigned_int(32_u32));
    }

    #[test]
    fn lowers_function_types() {
        let context = Context::create();
        let fn_type = context
            .void_type()
            .fn_type(&[context.i32_type().into()], false);
        let lowered = TypeLowerer::default()
            .lower_function(fn_type)
            .expect("failed to lower function");

        assert!(lowered.is_code());
    }

    #[test]
    fn lowers_pointer_values_with_recovered_pointees() {
        let context = Context::create();
        let module = context.create_module("ptr");
        let function = module.add_function(
            "f",
            context
                .ptr_type(inkwell::AddressSpace::default())
                .fn_type(&[], false),
            None,
        );
        let block = context.append_basic_block(function, "entry");
        let builder = context.create_builder();
        builder.position_at_end(block);
        let ptr = builder
            .build_alloca(context.i32_type(), "ptr")
            .expect("alloca should build");

        let mut lowerer = TypeLowerer::default();
        lowerer.note_pointer_pointee(ptr.as_value_ref() as usize, Type::unsigned_int(32_u32));
        let lowered = lowerer
            .lower_value(ptr.as_basic_value_enum())
            .expect("pointer should lower");

        assert_eq!(lowered, Type::unsigned_int(32_u32).to_pointer());
    }
}
