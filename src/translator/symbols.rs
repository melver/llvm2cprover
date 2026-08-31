// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Symbol discovery for globals, functions, and aggregate tags.
//!
//! Symbol construction happens before final body lowering so later stages can
//! refer to stable function, global, local, and aggregate identifiers. This
//! module also runs the pointer-pointee recovery fixed point that reconstructs
//! typed pointers from opaque LLVM `ptr` values.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use cprover_bindings::MachineModel as GotoMachineModel;
use cprover_bindings::goto_program::{
    Expr, Location, Parameter, Symbol, SymbolTable, SymbolValues, Type,
};
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{
    AsValueRef, BasicValueEnum, CallSiteValue, FunctionValue, InstructionOpcode, InstructionValue,
};

use crate::UnsoundOptions;
use crate::contracts::ContractMap;
use crate::util;

use super::globals::GlobalLowerer;
use super::instructions::InstructionLowerer;
use super::report::ReportCollector;
use super::types::TypeLowerer;
use super::{Diagnostic, EH_ACTIVE_SYMBOL, EH_PTR_SYMBOL, EH_SELECTOR_SYMBOL, UnsupportedFeature};

/// Builds an initial GOTO symbol table from parsed LLVM declarations.
///
/// The builder performs three tightly-coupled tasks:
///
/// - recover pointee hints
/// - declare globals, functions, and aggregate tags
/// - hand defined functions off to instruction lowering
#[derive(Debug, Default)]
pub struct SymbolBuilder {
    globals: GlobalLowerer,
    lowerer: TypeLowerer,
}

impl SymbolBuilder {
    /// Discover global and function declarations in one LLVM module.
    ///
    /// Report bookkeeping is discarded unless a recording [`ReportCollector`] is
    /// supplied; see [`SymbolBuilder::build_reporting`].
    pub fn build<'ctx>(
        &mut self,
        module: &Module<'ctx>,
        machine_model: GotoMachineModel,
        contracts: &ContractMap,
        unsound: &UnsoundOptions,
    ) -> Result<SymbolTable> {
        let replacements = BTreeSet::new();
        let enforcements = BTreeSet::new();
        self.build_reporting(
            module,
            machine_model,
            contracts,
            &replacements,
            &enforcements,
            unsound,
            &mut ReportCollector::default(),
        )
    }

    /// Discover declarations while recording translation-report inputs.
    pub(crate) fn build_reporting<'ctx>(
        &mut self,
        module: &Module<'ctx>,
        machine_model: GotoMachineModel,
        contracts: &ContractMap,
        replacements: &BTreeSet<String>,
        enforcements: &BTreeSet<String>,
        unsound: &UnsoundOptions,
        report: &mut ReportCollector,
    ) -> Result<SymbolTable> {
        // Provide the LLVM data layout so the type lowerer can insert explicit
        // struct padding fields.  Without this CBMC computes a sizeof that sums
        // field sizes without alignment gaps and produces wrong bounds checks.
        // This must happen before pointer-hint collection because hint recovery
        // lowers pointee types for `alloca`, GEP, loads, and stores; if a named
        // struct is first interned there without target data, the unpadded tag
        // gets cached and later body lowering reuses the wrong layout.
        let data_layout = module.get_data_layout();
        let data_layout_str = data_layout.as_str().to_str().unwrap_or_default();
        if !data_layout_str.is_empty() {
            self.lowerer.set_target_data(data_layout_str);
        }
        self.collect_pointer_hints(module)?;
        let mut symbols = SymbolTable::new(machine_model);
        insert_runtime_symbols(&mut symbols);

        for global in module.get_globals() {
            let name = global.get_name().to_string_lossy().into_owned();
            let typ = self
                .lowerer
                .lower_any(global.get_value_type())
                .map_err(|diag| diag.with_symbol(name.clone()))?;
            self.insert_pending_aggregate_symbols(&mut symbols);
            let mut symbol =
                Symbol::static_variable(name.clone(), name.clone(), typ, Location::none())
                    .with_is_extern(global.is_declaration());
            symbol.set_is_static_const(global.is_constant());
            if let Some(initializer) = global.get_initializer() {
                let expr = self.globals.lower_initializer(
                    &name,
                    initializer,
                    &symbol.typ,
                    &mut self.lowerer,
                    &symbols,
                    &Location::none(),
                )?;
                symbol.update(SymbolValues::Expr(expr));
            }
            symbols.insert(symbol);
        }

        for function in module.get_functions() {
            let name = function.get_name().to_string_lossy().into_owned();
            if symbols.lookup(name.as_str()).is_some() {
                if contracts.is_helper(&name) {
                    // Keeping the builtin would silently discard the compiled helper body that
                    // the manifest binds, so every helper class fails closed here.
                    bail!(
                        "manifest helper function `{name}` collides with a predeclared runtime symbol"
                    );
                }
                // CBMC and `cprover_bindings` seed builtin declarations up front. Keep those
                // canonical symbols instead of trying to re-declare the same name from LLVM IR.
                continue;
            }
            let typ = self
                .lower_function_symbol(function)
                .map_err(|diag| diag.with_symbol(name.clone()))?;
            self.insert_pending_aggregate_symbols(&mut symbols);
            let symbol = Symbol::function(
                name.clone(),
                typ,
                None,
                name,
                util::function_location(function),
            )
            .with_is_extern(function.count_basic_blocks() == 0);
            symbols.insert(symbol);
        }

        self.insert_pending_aggregate_symbols(&mut symbols);

        crate::contracts::inject::validate_contracts(&symbols, contracts)?;
        let replacement_map =
            crate::contracts::inject::prepare_replacements(&mut symbols, contracts, replacements)?;
        crate::contracts::inject::prepare_enforcements(
            &mut symbols,
            contracts,
            enforcements,
            replacements,
        )?;
        let mut instructions =
            InstructionLowerer::new(contracts, &replacement_map, unsound, report);
        for function in module.get_functions() {
            if function.count_basic_blocks() != 0 {
                instructions.lower_function(
                    function,
                    &mut symbols,
                    &mut self.lowerer,
                    &mut self.globals,
                )?;
            }
        }
        Ok(symbols)
    }

    fn insert_pending_aggregate_symbols(&mut self, symbols: &mut SymbolTable) {
        for symbol in self.lowerer.take_aggregate_symbols() {
            if symbols.lookup(symbol.name).is_none() {
                symbols.insert(symbol);
            }
        }
    }

    fn lower_function_symbol<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
    ) -> std::result::Result<Type, super::Diagnostic> {
        let function_name = function.get_name().to_string_lossy().into_owned();
        if let Some(typ) = self.override_runtime_function_type(function_name.as_str()) {
            return Ok(typ);
        }
        let mut parameters = Vec::with_capacity(function.count_params() as usize);
        for (index, param) in function.get_params().into_iter().enumerate() {
            let typ = self.lowerer.lower_parameter(param)?;
            let llvm_name = param.get_name().to_string_lossy();
            let base = if llvm_name.is_empty() {
                format!("arg{index}")
            } else {
                util::sanitize_name(llvm_name.as_ref())
            };
            parameters.push(Parameter::new(
                Some(base.clone()),
                Some(format!("{function_name}::{base}")),
                typ,
            ));
        }
        let return_type = self.lowerer.lower_function_return(function)?;
        if function.get_type().is_var_arg() {
            Ok(Type::variadic_code(parameters, return_type))
        } else {
            Ok(Type::code(parameters, return_type))
        }
    }

    fn override_runtime_function_type(&self, function_name: &str) -> Option<Type> {
        // Opaque-pointer LLVM declarations erase the libc pthread signatures down to `ptr`, but
        // CBMC's built-in pthread library still needs the thread-handle ABI surface to line up.
        // Keep the override as narrow as possible: the thread argument must stay `pthread_t`,
        // while the opaque attribute pointer can remain `void *`.
        match function_name {
            "pthread_create" => {
                let pthread_t = pthread_t_type();
                let pthread_attr_t = Type::union_tag("pthread_attr_t").to_typedef("pthread_attr_t");
                let start_routine = Type::code(
                    vec![anonymous_parameter(Type::void_pointer())],
                    Type::void_pointer(),
                )
                .to_pointer();
                Some(Type::code(
                    vec![
                        anonymous_parameter(pthread_t.to_pointer()),
                        anonymous_parameter(pthread_attr_t.to_pointer()),
                        anonymous_parameter(start_routine),
                        anonymous_parameter(Type::void_pointer()),
                    ],
                    Type::c_int(),
                ))
            }
            "pthread_join" => Some(Type::code(
                vec![
                    anonymous_parameter(pthread_t_type()),
                    anonymous_parameter(Type::void_pointer().to_pointer()),
                ],
                Type::c_int(),
            )),
            _ => None,
        }
    }

    fn collect_pointer_hints<'ctx>(&mut self, module: &Module<'ctx>) -> Result<()> {
        loop {
            let mut changed = false;
            for function in module.get_functions() {
                if function.count_basic_blocks() == 0 {
                    continue;
                }
                changed |= self.collect_function_pointer_hints(function)?;
            }
            // Call edges can refine pointer types across functions, so iterate to a fixed point
            // instead of baking a separate global inference data structure.
            if !changed {
                return Ok(());
            }
        }
    }

    fn collect_function_pointer_hints<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
    ) -> Result<bool> {
        let mut changed = false;
        for block in function.get_basic_blocks() {
            for instruction in block.get_instructions() {
                changed |= self.collect_instruction_pointer_hints(function, instruction)?;
            }
        }
        Ok(changed)
    }

    fn collect_instruction_pointer_hints<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
        instruction: InstructionValue<'ctx>,
    ) -> Result<bool> {
        match instruction.get_opcode() {
            InstructionOpcode::Alloca => self.collect_alloca_hint(instruction),
            InstructionOpcode::Load => self.collect_load_hint(instruction),
            InstructionOpcode::Store => self.collect_store_hint(instruction),
            InstructionOpcode::GetElementPtr => self.collect_gep_hint(instruction),
            InstructionOpcode::BitCast | InstructionOpcode::AddrSpaceCast => {
                self.collect_pointer_copy_hint(instruction, 0)
            }
            InstructionOpcode::Select => self.collect_select_hint(instruction),
            InstructionOpcode::Phi => self.collect_phi_hint(function, instruction),
            InstructionOpcode::Call => self.collect_call_hint(function, instruction),
            InstructionOpcode::Return => self.collect_return_hint(function, instruction),
            _ => Ok(false),
        }
    }

    fn collect_alloca_hint<'ctx>(&mut self, instruction: InstructionValue<'ctx>) -> Result<bool> {
        let pointee = self
            .lowerer
            .lower_basic(instruction.get_allocated_type().map_err(|_| {
                super::Diagnostic::unsupported_feature(
                    super::UnsupportedFeature::Instruction,
                    "malformed alloca while recovering pointer types",
                )
            })?)?;
        Ok(self
            .lowerer
            .note_pointer_pointee(instruction.as_value_ref() as usize, pointee))
    }

    fn collect_load_hint<'ctx>(&mut self, instruction: InstructionValue<'ctx>) -> Result<bool> {
        let pointee = self.lowerer.lower_any(instruction.get_type())?;
        Ok(self
            .pointer_operand(instruction, 0)
            .is_some_and(|value| self.note_value_pointee(value, pointee)))
    }

    fn collect_store_hint<'ctx>(&mut self, instruction: InstructionValue<'ctx>) -> Result<bool> {
        let Some(value) = self.operand_value(instruction, 0) else {
            return Ok(false);
        };
        let pointee = self.lowerer.lower_value(value)?;
        Ok(self
            .pointer_operand(instruction, 1)
            .is_some_and(|ptr| self.note_value_pointee(ptr, pointee)))
    }

    fn collect_gep_hint<'ctx>(&mut self, instruction: InstructionValue<'ctx>) -> Result<bool> {
        let source = instruction.get_gep_source_element_type().map_err(|_| {
            super::Diagnostic::unsupported_feature(
                super::UnsupportedFeature::Instruction,
                "malformed getelementptr while recovering pointer types",
            )
        })?;
        let source_type = self.lowerer.lower_basic(source)?;
        let mut changed = self
            .pointer_operand(instruction, 0)
            .is_some_and(|base| self.note_value_pointee(base, source_type.clone()));
        let pointee = self.resolve_gep_pointee(instruction, source)?;
        changed |= self.note_instruction_pointee(instruction, pointee);
        Ok(changed)
    }

    fn collect_pointer_copy_hint<'ctx>(
        &mut self,
        instruction: InstructionValue<'ctx>,
        operand_index: u32,
    ) -> Result<bool> {
        Ok(self
            .operand_value(instruction, operand_index)
            .and_then(|value| self.lowerer.pointer_pointee(value.as_value_ref() as usize))
            .is_some_and(|pointee| self.note_instruction_pointee(instruction, pointee)))
    }

    fn collect_select_hint<'ctx>(&mut self, instruction: InstructionValue<'ctx>) -> Result<bool> {
        let mut changed = false;
        for operand_index in [1_u32, 2_u32] {
            changed |= self.collect_pointer_copy_hint(instruction, operand_index)?;
        }
        Ok(changed)
    }

    fn collect_phi_hint<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
        instruction: InstructionValue<'ctx>,
    ) -> Result<bool> {
        let function_name = function.get_name().to_string_lossy().into_owned();
        let phi = inkwell::values::PhiValue::try_from(instruction).map_err(|_| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::ControlFlow,
                format!("invalid PHI wrapper in {function_name}"),
            )
            .with_symbol(function_name.clone())
        })?;
        let mut changed = false;
        for (incoming, _) in phi.get_incomings() {
            if let Some(pointee) = self
                .lowerer
                .pointer_pointee(incoming.as_value_ref() as usize)
            {
                changed |= self.note_instruction_pointee(instruction, pointee);
            }
        }
        Ok(changed)
    }

    fn collect_call_hint<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
        instruction: InstructionValue<'ctx>,
    ) -> Result<bool> {
        let function_name = function.get_name().to_string_lossy().into_owned();
        let call = CallSiteValue::try_from(instruction).map_err(|_| {
            Diagnostic::unsupported_feature(
                UnsupportedFeature::Instruction,
                format!("invalid call wrapper in {function_name}"),
            )
            .with_symbol(function_name.clone())
        })?;
        let Some(callee) = call.get_called_fn_value() else {
            return Ok(false);
        };

        let mut changed = false;
        for index in 0..call.count_arguments() {
            let Some(arg) = self.operand_value(instruction, index) else {
                continue;
            };
            let Some(param) = callee.get_nth_param(index) else {
                break;
            };
            let Some(pointee) = self.lowerer.pointer_pointee(param.as_value_ref() as usize) else {
                continue;
            };
            changed |= self.note_value_pointee(arg, pointee);
        }

        if instruction.get_type().is_pointer_type()
            && let Some(pointee) = self
                .lowerer
                .function_return_pointee(callee.as_value_ref() as usize)
        {
            changed |= self.note_instruction_pointee(instruction, pointee);
        }

        Ok(changed)
    }

    fn collect_return_hint<'ctx>(
        &mut self,
        function: FunctionValue<'ctx>,
        instruction: InstructionValue<'ctx>,
    ) -> Result<bool> {
        if !function
            .get_type()
            .get_return_type()
            .is_some_and(|typ| typ.is_pointer_type())
        {
            return Ok(false);
        }
        let Some(value) = self.operand_value(instruction, 0) else {
            return Ok(false);
        };
        let Some(pointee) = self.lowerer.pointer_pointee(value.as_value_ref() as usize) else {
            return Ok(false);
        };
        Ok(self
            .lowerer
            .note_function_return_pointee(function.as_value_ref() as usize, pointee))
    }

    fn operand_value<'ctx>(
        &self,
        instruction: InstructionValue<'ctx>,
        index: u32,
    ) -> Option<BasicValueEnum<'ctx>> {
        instruction
            .get_operand(index)
            .and_then(|operand| operand.value())
    }

    fn pointer_operand<'ctx>(
        &self,
        instruction: InstructionValue<'ctx>,
        index: u32,
    ) -> Option<BasicValueEnum<'ctx>> {
        self.operand_value(instruction, index)
            .filter(|value| value.get_type().is_pointer_type())
    }

    fn note_value_pointee<'ctx>(&mut self, value: BasicValueEnum<'ctx>, pointee: Type) -> bool {
        self.lowerer
            .note_pointer_pointee(value.as_value_ref() as usize, pointee)
    }

    fn note_instruction_pointee<'ctx>(
        &mut self,
        instruction: InstructionValue<'ctx>,
        pointee: Type,
    ) -> bool {
        self.lowerer
            .note_pointer_pointee(instruction.as_value_ref() as usize, pointee)
    }

    fn resolve_gep_pointee<'ctx>(
        &mut self,
        instruction: InstructionValue<'ctx>,
        mut current: BasicTypeEnum<'ctx>,
    ) -> Result<Type> {
        // This runs before aggregate tags are inserted into the symbol table, so follow the LLVM
        // types directly instead of trying to peel GOTO struct tags back open.
        for position in 1..instruction.get_num_operands() {
            if position == 1 {
                continue;
            }
            if current.is_struct_type() {
                let Some(index) = instruction
                    .get_operand(position)
                    .and_then(|operand| operand.value())
                    .and_then(Self::constant_usize)
                else {
                    break;
                };
                let fields = current.into_struct_type().get_field_types();
                let Some(field_type) = fields.get(index).copied() else {
                    break;
                };
                current = field_type;
                continue;
            }
            current = match current {
                BasicTypeEnum::ArrayType(typ) => typ.get_element_type(),
                BasicTypeEnum::VectorType(typ) => typ.get_element_type(),
                _ => break,
            };
        }
        self.lowerer.lower_basic(current).map_err(Into::into)
    }

    fn constant_usize<'ctx>(value: BasicValueEnum<'ctx>) -> Option<usize> {
        match value {
            BasicValueEnum::IntValue(value) => value
                .get_zero_extended_constant()
                .map(|value| value as usize),
            _ => None,
        }
    }
}

fn anonymous_parameter(typ: Type) -> Parameter {
    Parameter::new::<String>(None, None, typ)
}

fn pthread_t_type() -> Type {
    Type::c_size_t().to_typedef("pthread_t")
}

pub(super) fn insert_runtime_symbols(symbols: &mut SymbolTable) {
    if symbols.lookup("__CPROVER_deallocated").is_none() {
        // CBMC's own C front-end seeds this pointer before library instrumentation runs.
        // LLVM-originated GOTO lacks that path, so we add the same global explicitly.
        let typ = Type::void_pointer();
        let mut symbol = Symbol::static_variable(
            "__CPROVER_deallocated",
            "__CPROVER_deallocated",
            typ.clone(),
            Location::none(),
        );
        symbol.update(SymbolValues::Expr(typ.null()));
        symbols.insert(symbol);
    }
    if symbols.lookup("pthread_t").is_none() {
        let width = symbols.machine_model().pointer_width;
        symbols.insert(Symbol::typedef(
            "pthread_t",
            "pthread_t",
            Type::unsigned_int(width).to_typedef("pthread_t"),
            Location::none(),
        ));
    }
    if symbols.lookup("tag-pthread_attr_t").is_none() {
        // CBMC only needs the opaque tag name to merge our declaration with its pthread model;
        // the built-in library supplies the concrete layout when it links its own definition.
        symbols.insert(Symbol::incomplete_union(
            "pthread_attr_t",
            "union pthread_attr_t",
        ));
    }
    if symbols.lookup("pthread_attr_t").is_none() {
        symbols.insert(Symbol::typedef(
            "pthread_attr_t",
            "pthread_attr_t",
            Type::union_tag("pthread_attr_t").to_typedef("pthread_attr_t"),
            Location::none(),
        ));
    }
    insert_eh_runtime_symbol(symbols, EH_ACTIVE_SYMBOL, Type::bool(), Expr::bool_false());
    insert_eh_runtime_symbol(
        symbols,
        EH_PTR_SYMBOL,
        Type::void_pointer(),
        Type::void_pointer().null(),
    );
    insert_eh_runtime_symbol(
        symbols,
        EH_SELECTOR_SYMBOL,
        Type::c_int(),
        Expr::int_constant(0, Type::c_int()),
    );
}

fn insert_eh_runtime_symbol(symbols: &mut SymbolTable, name: &str, typ: Type, init: Expr) {
    if symbols.lookup(name).is_some() {
        return;
    }
    // The narrow EH model keeps one in-flight exception record per thread. Global hidden state is
    // enough for the current subset and gives `throw`/`invoke` a shared rendezvous across calls.
    let mut symbol =
        Symbol::static_variable(name, name, typ, Location::none()).with_is_thread_local(true);
    symbol.update(SymbolValues::Expr(init));
    symbols.insert(symbol);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translator::DiagnosticKind;
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;

    // Keep the IR fixture inline in each test; these helpers only remove the repeated
    // parse/build boilerplate so the assertion stays the focal point.
    fn parse_module<'ctx>(context: &'ctx Context, ir: &[u8]) -> Module<'ctx> {
        let buffer = MemoryBuffer::create_from_memory_range_copy(ir, "symbols");
        context
            .create_module_from_ir(buffer)
            .expect("failed to parse test IR")
    }

    fn build_symbols(module: &Module<'_>) -> SymbolTable {
        build_symbols_with_config(module, false)
    }

    fn unsound_options(all: bool) -> crate::UnsoundOptions {
        if all {
            crate::UnsoundOptions::all()
        } else {
            crate::UnsoundOptions::default()
        }
    }

    fn build_symbols_with_config(module: &Module<'_>, unsound: bool) -> SymbolTable {
        let unsound = unsound_options(unsound);
        SymbolBuilder::default()
            .build(
                module,
                crate::util::resolve_machine_model(module, crate::MachineModel::Auto)
                    .expect("failed to resolve machine model"),
                &ContractMap::default(),
                &unsound,
            )
            .expect("failed to build symbol table")
    }

    fn build_symbols_from_ir(ir: &[u8]) -> SymbolTable {
        let context = Context::create();
        let module = parse_module(&context, ir);
        build_symbols(&module)
    }

    fn build_symbols_result_from_ir(ir: &[u8]) -> Result<SymbolTable> {
        build_symbols_result_with_config_from_ir(ir, false)
    }

    fn build_symbols_result_with_config_from_ir(ir: &[u8], unsound: bool) -> Result<SymbolTable> {
        let context = Context::create();
        let module = parse_module(&context, ir);
        let unsound = unsound_options(unsound);
        SymbolBuilder::default().build(
            &module,
            crate::util::resolve_machine_model(&module, crate::MachineModel::Auto)
                .expect("failed to resolve machine model"),
            &ContractMap::default(),
            &unsound,
        )
    }

    fn expr_value<'a>(
        symbols: &'a SymbolTable,
        name: &str,
    ) -> &'a cprover_bindings::goto_program::Expr {
        let symbol = symbols.lookup(name).expect("missing symbol");
        match &symbol.value {
            SymbolValues::Expr(expr) => expr,
            value => panic!("expected expression initializer, got {value:?}"),
        }
    }

    fn stmt_debug(symbols: &SymbolTable, name: &str) -> String {
        let symbol = symbols.lookup(name).expect("missing function");
        match &symbol.value {
            SymbolValues::Stmt(body) => format!("{body:?}"),
            value => panic!("expected statement body, got {value:?}"),
        }
    }

    #[test]
    fn builds_symbols_for_functions_and_globals() {
        let context = Context::create();
        let module = context.create_module("symbols");
        module.add_global(context.i32_type(), None, "g");
        module.add_function("f", context.void_type().fn_type(&[], false), None);

        let symbols = build_symbols(&module);

        assert!(symbols.lookup("g").is_some());
        assert!(symbols.lookup("f").is_some());
    }

    #[test]
    fn seeds_cprover_deallocated_runtime_symbol() {
        let context = Context::create();
        let module = context.create_module("symbols");

        let symbols = build_symbols(&module);

        assert!(symbols.lookup("__CPROVER_deallocated").is_some());
    }

    #[test]
    fn propagates_function_debug_location() {
        let symbols = build_symbols_from_ir(
            br#"
            define dso_local i32 @f(i32 %x) !dbg !10 {
            entry:
              %tmp = add i32 %x, 1, !dbg !18
              ret i32 %tmp, !dbg !19
            }

            !llvm.dbg.cu = !{!0}
            !llvm.module.flags = !{!2, !3}
            !0 = distinct !DICompileUnit(language: DW_LANG_C11, file: !1, producer: "clang", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)
            !1 = !DIFile(filename: "/tmp/f.c", directory: "")
            !2 = !{i32 7, !"Dwarf Version", i32 5}
            !3 = !{i32 2, !"Debug Info Version", i32 3}
            !10 = distinct !DISubprogram(name: "f", scope: !1, file: !1, line: 3, type: !12, scopeLine: 3, spFlags: DISPFlagDefinition, unit: !0)
            !12 = !DISubroutineType(types: !13)
            !13 = !{!14, !14}
            !14 = !DIBasicType(name: "int", size: 32, encoding: DW_ATE_signed)
            !18 = !DILocation(line: 4, column: 10, scope: !10)
            !19 = !DILocation(line: 5, column: 3, scope: !10)
        "#,
        );

        let function = symbols.lookup("f").expect("function symbol");
        assert_eq!(function.location.filename().as_deref(), Some("/tmp/f.c"));
        assert_eq!(function.location.start_line(), Some(4));
    }

    #[test]
    fn names_function_type_parameters() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i32 %x, i32 %y) {
            entry:
              %sum = add i32 %x, %y
              ret i32 %sum
            }
        "#,
        );

        let function = symbols.lookup("f").expect("function symbol");
        let params = function.typ.parameters().expect("code parameters");
        assert_eq!(
            params[0].identifier().map(|id| id.to_string()).as_deref(),
            Some("f::x")
        );
        assert_eq!(
            params[1].identifier().map(|id| id.to_string()).as_deref(),
            Some("f::y")
        );
    }

    #[test]
    fn skips_duplicate_cprover_builtin_declarations() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @__CPROVER_assume(i1)
            define i32 @main() {
            entry:
              call void @__CPROVER_assume(i1 true)
              ret i32 0
            }
        "#,
        );

        assert!(symbols.lookup("__CPROVER_assume").is_some());
        assert!(symbols.lookup("main").is_some());
    }

    #[test]
    fn rejects_inline_asm_model_colliding_with_predeclared_symbol() {
        let context = Context::create();
        let module = parse_module(
            &context,
            br#"
            define i32 @abort(i32 %value) {
            entry:
              ret i32 %value
            }
        "#,
        );
        let contracts = crate::contracts::parse_contracts(
            r#"
            [[inline_asm]]
            asm = "opaque"
            constraints = "=r,r"
            dialect = "att"
            side_effects = true
            align_stack = false
            model = "abort"
        "#,
        )
        .expect("failed to parse inline asm model");
        let error = SymbolBuilder::default()
            .build(
                &module,
                crate::util::resolve_machine_model(&module, crate::MachineModel::Auto)
                    .expect("failed to resolve machine model"),
                &contracts,
                &crate::UnsoundOptions::default(),
            )
            .expect_err("runtime symbol collision must fail closed");

        assert!(
            error
                .to_string()
                .contains("collides with a predeclared runtime symbol")
        );
    }

    #[test]
    fn lowers_global_integer_initializer() {
        let context = Context::create();
        let module = context.create_module("symbols");
        let global = module.add_global(context.i32_type(), None, "g");
        global.set_initializer(&context.i32_type().const_int(7, false));

        let symbols = build_symbols(&module);
        assert_eq!(
            expr_value(&symbols, "g")
                .int_constant_value()
                .expect("constant"),
            7.into()
        );
    }

    #[test]
    fn lowers_pointer_initializer_to_global_symbol() {
        let symbols = build_symbols_from_ir(
            br#"
            @g = global i32 7
            @p = global ptr @g
        "#,
        );

        assert!(format!("{:?}", expr_value(&symbols, "p")).contains("g"));
    }

    #[test]
    fn lowers_pointer_initializer_to_function_symbol() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @f()
            @fp = global ptr @f
        "#,
        );

        assert!(format!("{:?}", expr_value(&symbols, "fp")).contains("f"));
    }

    #[test]
    fn lowers_constant_gep_string_initializer() {
        let symbols = build_symbols_from_ir(
            br#"
            @str = private unnamed_addr constant [4 x i8] c"abc\00"
            @ps = global ptr getelementptr inbounds ([4 x i8], ptr @str, i64 0, i64 0)
        "#,
        );

        assert!(format!("{:?}", expr_value(&symbols, "ps")).contains("str"));
    }

    #[test]
    fn lowers_non_string_array_initializer() {
        let symbols = build_symbols_from_ir(
            br#"
            @arr = global [2 x i32] [i32 1, i32 2]
        "#,
        );

        let debug = format!("{:?}", expr_value(&symbols, "arr"));
        assert!(debug.contains("IntConstant(1)"));
        assert!(debug.contains("IntConstant(2)"));
    }

    #[test]
    fn lowers_constant_integer_expression_initializer() {
        let symbols = build_symbols_from_ir(
            br#"
            @g = global i32 add (i32 1, i32 2)
            @h = global i16 trunc (i32 258 to i16)
        "#,
        );

        assert_eq!(
            expr_value(&symbols, "g")
                .int_constant_value()
                .expect("constant-folded add result"),
            3.into()
        );
        assert_eq!(
            expr_value(&symbols, "h").typ(),
            &cprover_bindings::goto_program::Type::unsigned_int(16_u32)
        );
    }

    #[test]
    fn lowers_inttoptr_pointer_expression_with_symbol_context() {
        let symbols = build_symbols_from_ir(
            br#"
            @g = global ptr inttoptr (i64 1 to ptr)
        "#,
        );

        let debug = format!("{:?}", expr_value(&symbols, "g"));
        assert!(debug.contains("Typecast"));
    }

    #[test]
    fn lowers_function_return_body() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i32 %x) {
            entry:
              ret i32 %x
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Return"));
        assert!(debug.contains("f::x"));
    }

    #[test]
    fn lowers_indirect_function_pointer_call() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @add_one(i32 %x) {
            entry:
              %out = add i32 %x, 1
              ret i32 %out
            }

            define i32 @f(ptr %fp, i32 %x) {
            entry:
              %out = call i32 %fp(i32 %x)
              ret i32 %out
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Dereference"));
        assert!(debug.contains("FunctionCall"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_calls_to_preseeded_c_int_builtins() {
        let symbols = build_symbols_from_ir(
            br#"
            @path = private unnamed_addr constant [5 x i8] c"/tmp\00"

            declare i32 @unlink(ptr)

            define i32 @f() {
            entry:
              %rc = call i32 @unlink(ptr @path)
              ret i32 %rc
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("FunctionCall"));
        assert!(debug.contains("Assign"));
        assert!(debug.contains("call_ret"));
    }

    #[test]
    fn rejects_variadic_setup_intrinsics_in_sound_mode() {
        let error = build_symbols_result_from_ir(
            br#"
            %struct.__va_list_tag = type { i32, i32, ptr, ptr }

            define void @f(ptr %fmt, ...) {
            entry:
              %ap = alloca [1 x %struct.__va_list_tag], align 16
              %slot = getelementptr inbounds [1 x %struct.__va_list_tag], ptr %ap, i64 0, i64 0
              call void @llvm.va_start.p0(ptr %slot)
              call void @llvm.va_end.p0(ptr %slot)
              ret void
            }

            declare void @llvm.va_start.p0(ptr)
            declare void @llvm.va_end.p0(ptr)
        "#,
        )
        .expect_err("sound mode should reject va_start");

        let diagnostic = error
            .downcast_ref::<Diagnostic>()
            .expect("expected structured diagnostic");
        assert!(diagnostic.message.contains("llvm.va_start.p0"));
    }

    #[test]
    fn lowers_variadic_setup_intrinsics_in_unsound_mode() {
        let symbols = build_symbols_result_with_config_from_ir(
            br#"
            %struct.__va_list_tag = type { i32, i32, ptr, ptr }

            define void @f(ptr %fmt, ...) {
            entry:
              %ap = alloca [1 x %struct.__va_list_tag], align 16
              %slot = getelementptr inbounds [1 x %struct.__va_list_tag], ptr %ap, i64 0, i64 0
              call void @llvm.va_start.p0(ptr %slot)
              call void @llvm.va_end.p0(ptr %slot)
              ret void
            }

            declare void @llvm.va_start.p0(ptr)
            declare void @llvm.va_end.p0(ptr)
        "#,
            true,
        )
        .expect("unsound mode should allow va_start");

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Return"));
    }

    #[test]
    fn rejects_void_inline_asm_in_unsound_mode() {
        let error = build_symbols_result_with_config_from_ir(
            br#"
            define void @f(ptr %p) {
            entry:
              call void asm sideeffect "movq $$0, ($0)", "r,~{memory}"(ptr %p)
              ret void
            }
        "#,
            true,
        )
        .expect_err("unsound mode must not discard stateful void inline asm");

        let diagnostic = error
            .downcast_ref::<Diagnostic>()
            .expect("expected structured diagnostic");
        assert_eq!(
            diagnostic.kind,
            DiagnosticKind::UnsupportedFeature(UnsupportedFeature::InlineAsm)
        );
        assert_eq!(diagnostic.symbol.as_deref(), Some("f"));
    }

    #[test]
    fn rejects_result_inline_asm_in_unsound_mode() {
        let error = build_symbols_result_with_config_from_ir(
            br#"
            define i64 @f(ptr %p) {
            entry:
              %result = call i64 asm sideeffect "movq ($1), $0", "=r,r,~{memory}"(ptr %p)
              ret i64 %result
            }
        "#,
            true,
        )
        .expect_err(
            "unsound mode must not replace stateful inline asm results with nondeterminism",
        );

        let diagnostic = error
            .downcast_ref::<Diagnostic>()
            .expect("expected structured diagnostic");
        assert_eq!(
            diagnostic.kind,
            DiagnosticKind::UnsupportedFeature(UnsupportedFeature::InlineAsm)
        );
        assert_eq!(diagnostic.symbol.as_deref(), Some("f"));
    }

    #[test]
    fn lowers_pointer_comparisons_with_mismatched_pointees() {
        let symbols = build_symbols_from_ir(
            br#"
            declare ptr @malloc(i64)

            define i1 @f() {
            entry:
              %heap = call ptr @malloc(i64 4)
              %slot = alloca i32, align 4
              %cmp = icmp ne ptr %heap, %slot
              ret i1 %cmp
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Notequal"));
        assert!(debug.contains("Typecast"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_llvm_assume_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @llvm.assume(i1)

            define void @f(i1 %cond) {
            entry:
              call void @llvm.assume(i1 %cond)
              ret void
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Assume"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_lifetime_intrinsics_as_nops() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @llvm.lifetime.start.p0(i64 immarg, ptr nocapture)
            declare void @llvm.lifetime.end.p0(i64 immarg, ptr nocapture)

            define void @f(ptr %p) {
            entry:
              call void @llvm.lifetime.start.p0(i64 4, ptr %p)
              call void @llvm.lifetime.end.p0(i64 4, ptr %p)
              ret void
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(!debug.contains("lifetime"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_memset_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)

            define void @f(ptr %dst) {
            entry:
              call void @llvm.memset.p0.i64(ptr %dst, i8 0, i64 4, i1 false)
              ret void
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("FunctionCall"));
        assert!(debug.contains("memset"));
    }

    #[test]
    fn lowers_memcpy_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)

            define void @f(ptr %dst, ptr %src) {
            entry:
              call void @llvm.memcpy.p0.p0.i64(ptr %dst, ptr %src, i64 4, i1 false)
              ret void
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("FunctionCall"));
        assert!(debug.contains("memcpy"));
    }

    #[test]
    fn lowers_memmove_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)

            define void @f(ptr %dst, ptr %src) {
            entry:
              call void @llvm.memmove.p0.p0.i64(ptr %dst, ptr %src, i64 4, i1 false)
              ret void
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("FunctionCall"));
        assert!(debug.contains("memmove"));
    }

    #[test]
    fn lowers_ctpop_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare i32 @llvm.ctpop.i32(i32)

            define i32 @f(i32 %x) {
            entry:
              %y = call i32 @llvm.ctpop.i32(i32 %x)
              ret i32 %y
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Popcount"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_bswap_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare i32 @llvm.bswap.i32(i32)

            define i32 @f(i32 %x) {
            entry:
              %y = call i32 @llvm.bswap.i32(i32 %x)
              ret i32 %y
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Bswap"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_memcmp_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare i32 @llvm.memcmp.p0.p0.i64(ptr, ptr, i64, i1)

            define i32 @f(ptr %lhs, ptr %rhs) {
            entry:
              %cmp = call i32 @llvm.memcmp.p0.p0.i64(ptr %lhs, ptr %rhs, i64 4, i1 false)
              ret i32 %cmp
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("FunctionCall"));
        assert!(debug.contains("memcmp"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_unreachable_instruction() {
        let symbols = build_symbols_from_ir(
            br#"
            define void @f() {
            entry:
              unreachable
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Assert"));
        assert!(debug.contains("llvm.unreachable"));
    }

    #[test]
    fn lowers_integer_ops_and_signed_compare() {
        let symbols = build_symbols_from_ir(
            br#"
            define i1 @f(i32 %a, i32 %b) {
            entry:
              %sum = add i32 %a, %b
              %quo = sdiv i32 %sum, %b
              %cmp = icmp sgt i32 %quo, %a
              ret i1 %cmp
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Plus"));
        assert!(debug.contains("Div"));
        assert!(debug.contains("Gt"));
    }

    #[test]
    fn lowers_float_ops_casts_and_compare() {
        let symbols = build_symbols_from_ir(
            br#"
            define i1 @f(float %a, float %b) {
            entry:
              %sum = fadd float %a, %b
              %neg = fneg float %sum
              %wide = fpext float %neg to double
              %back = fptrunc double %wide to float
              %cmp = fcmp oeq float %back, %neg
              ret i1 %cmp
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Plus"));
        assert!(debug.contains("UnaryMinus"));
        assert!(debug.contains("Typecast"));
        assert!(debug.contains("IeeeFloatEqual"));
    }

    #[test]
    fn lowers_float_math_intrinsic_calls() {
        let symbols = build_symbols_from_ir(
            br#"
            declare float @llvm.fabs.f32(float)
            declare float @llvm.sqrt.f32(float)
            declare float @llvm.fma.f32(float, float, float)

            define float @f(float %x, float %y) {
            entry:
              %a = call float @llvm.fabs.f32(float %x)
              %b = call float @llvm.sqrt.f32(float %a)
              %c = call float @llvm.fma.f32(float %b, float %y, float %x)
              ret float %c
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("fabsf"));
        assert!(debug.contains("sqrtf"));
        assert!(debug.contains("fmaf"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_float_global_initializer() {
        let symbols = build_symbols_from_ir(
            br#"
            @g = global double 3.25
        "#,
        );

        let debug = format!("{:?}", expr_value(&symbols, "g"));
        assert!(debug.contains("DoubleConstant(3.25"));
    }

    #[test]
    fn lowers_alloca_store_and_load() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i32 %x) {
            entry:
              %slot = alloca i32, align 4
              store i32 %x, ptr %slot, align 4
              %y = load i32, ptr %slot, align 4
              ret i32 %y
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("alloca"));
        assert!(!debug.contains("Dereference"));
    }

    #[test]
    fn recovers_parameter_pointee_from_load() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(ptr %p) {
            entry:
              %x = load i32, ptr %p, align 4
              ret i32 %x
            }
        "#,
        );

        let function = symbols.lookup("f").expect("function symbol");
        let params = function.typ.parameters().expect("code parameters");
        assert_eq!(
            params[0].typ().base_type(),
            Some(&Type::unsigned_int(32_u32))
        );
    }

    #[test]
    fn propagates_pointer_pointees_through_direct_calls() {
        let symbols = build_symbols_from_ir(
            br#"
            define void @callee(ptr %p) {
            entry:
              store i32 1, ptr %p, align 4
              ret void
            }

            define void @caller(ptr %q) {
            entry:
              call void @callee(ptr %q)
              ret void
            }
        "#,
        );

        let function = symbols.lookup("caller").expect("caller symbol");
        let params = function.typ.parameters().expect("code parameters");
        assert_eq!(
            params[0].typ().base_type(),
            Some(&Type::unsigned_int(32_u32))
        );
    }

    #[test]
    fn lowers_struct_gep() {
        let symbols = build_symbols_from_ir(
            br#"
            %pair = type { i32, i32 }

            define ptr @f(ptr %p) {
            entry:
              %field = getelementptr %pair, ptr %p, i64 0, i32 1
              ret ptr %field
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Member"));
        assert!(debug.contains("field1"));
    }

    #[test]
    fn recovers_pointer_results_for_alloca_and_gep() {
        let symbols = build_symbols_from_ir(
            br#"
            %pair = type { i32, i32 }

            define ptr @f(ptr %p) {
            entry:
              %slot = alloca i32, align 4
              %field = getelementptr %pair, ptr %p, i64 0, i32 1
              ret ptr %field
            }
        "#,
        );

        let slot = symbols
            .iter()
            .find_map(|(_, symbol)| {
                (symbol.name.to_string().starts_with("f::alloca")
                    && symbol.typ == Type::unsigned_int(32_u32))
                .then_some(symbol)
            })
            .expect("alloca slot");
        let debug = stmt_debug(&symbols, "f");
        let function = symbols.lookup("f").expect("function symbol");
        assert!(slot.is_auxiliary);
        assert!(symbols.lookup("f::field").is_none());
        assert!(debug.contains("Member"));
        assert!(debug.contains("AddressOf"));
        assert_eq!(
            function.typ.return_type().and_then(Type::base_type),
            Some(&Type::unsigned_int(32_u32))
        );
    }

    #[test]
    fn lowers_select_and_sext() {
        let symbols = build_symbols_from_ir(
            br#"
            define i64 @f(i1 %cond, i32 %a, i32 %b) {
            entry:
              %pick = select i1 %cond, i32 %a, i32 %b
              %wide = sext i32 %pick to i64
              ret i64 %wide
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("If"));
        assert!(debug.contains("Typecast"));
    }

    #[test]
    fn lowers_multiblock_conditional_branch() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i1 %cond) {
            entry:
              br i1 %cond, label %then, label %else

            then:
              ret i32 1

            else:
              ret i32 0
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Label"));
        assert!(debug.contains("Goto"));
        assert!(debug.contains("Ifthenelse"));
    }

    #[test]
    fn lowers_phi_on_diamond_edges() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i1 %cond, i32 %a, i32 %b) {
            entry:
              br i1 %cond, label %then, label %else

            then:
              br label %join

            else:
              br label %join

            join:
              %x = phi i32 [ %a, %then ], [ %b, %else ]
              ret i32 %x
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("phi_edge"));
        assert!(debug.contains("f::x"));
        assert!(debug.contains("Goto"));
    }

    #[test]
    fn lowers_loop_phi_on_backedge() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f() {
            entry:
              br label %loop

            loop:
              %i = phi i32 [ 0, %entry ], [ %next, %body ]
              %keep_going = icmp ult i32 %i, 3
              br i1 %keep_going, label %body, label %exit

            body:
              %next = add i32 %i, 1
              br label %loop

            exit:
              ret i32 %i
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("f::i"));
        assert!(debug.contains("phi_edge"));
        assert!(debug.contains("Ifthenelse"));
    }

    #[test]
    fn lowers_switch_dispatch() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i32 %x) {
            entry:
              switch i32 %x, label %default [
                i32 0, label %zero
                i32 7, label %seven
              ]

            zero:
              ret i32 10

            seven:
              ret i32 20

            default:
              ret i32 30
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Switch"));
        assert!(debug.contains("IntConstant(0)"));
        assert!(debug.contains("IntConstant(7)"));
        assert!(debug.contains("Goto"));
    }

    #[test]
    fn lowers_insertvalue_extractvalue_body() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i32 %x) {
            entry:
              %agg = insertvalue { i32, i32 } zeroinitializer, i32 %x, 1
              %out = extractvalue { i32, i32 } %agg, 1
              ret i32 %out
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Struct"));
        assert!(debug.contains("field1"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_nested_insertvalue_extractvalue_body() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(i32 %x) {
            entry:
              %agg = insertvalue { { i32, i32 }, i1 } zeroinitializer, i32 %x, 0, 1
              %out = extractvalue { { i32, i32 }, i1 } %agg, 0, 1
              ret i32 %out
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Struct"));
        assert!(debug.contains("field0"));
        assert!(debug.contains("field1"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_vector_element_ops() {
        let symbols = build_symbols_from_ir(
            br#"
            define i32 @f(<4 x i32> %v, i32 %x) {
            entry:
              %v1 = insertelement <4 x i32> %v, i32 %x, i32 2
              %out = extractelement <4 x i32> %v1, i32 2
              ret i32 %out
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Vector"));
        assert!(debug.contains("If"));
        assert!(debug.contains("Index"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_shufflevector_body() {
        let symbols = build_symbols_from_ir(
            br#"
            define <2 x i32> @f(<2 x i32> %a, <2 x i32> %b) {
            entry:
              %out = shufflevector <2 x i32> %a, <2 x i32> %b, <2 x i32> <i32 1, i32 2>
              ret <2 x i32> %out
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("Vector"));
        assert!(debug.contains("Index"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn lowers_overflow_intrinsic_call() {
        let symbols = build_symbols_from_ir(
            br#"
            declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)

            define i1 @f(i32 %x, i32 %y) {
            entry:
              %pair = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %x, i32 %y)
              %flag = extractvalue { i32, i1 } %pair, 1
              ret i1 %flag
            }
        "#,
        );

        let debug = stmt_debug(&symbols, "f");
        assert!(debug.contains("overflowed"));
        assert!(debug.contains("Signedbv"));
        assert!(debug.contains("Struct"));
        assert!(debug.contains("Return"));
    }

    #[test]
    fn rejects_unsupported_intrinsic_call_with_location() {
        let err = build_symbols_result_from_ir(
            br#"
            source_filename = "/tmp/unsupported_intrinsic.ll"

            declare i64 @llvm.readcyclecounter()

            define i64 @f() {
            entry:
              %y = call i64 @llvm.readcyclecounter()
              ret i64 %y
            }
        "#,
        )
        .expect_err("unsupported intrinsic should fail closed");

        let debug = format!("{err:#}");
        assert!(debug.contains("unsupported intrinsic call to llvm.readcyclecounter"));
        assert!(debug.contains("llvm.readcyclecounter"));
        assert!(debug.contains("/tmp/unsupported_intrinsic.ll:1"));
    }

    #[test]
    fn lowers_atomicrmw_body() {
        let context = Context::create();
        let module = parse_module(
            &context,
            br#"
                        define i32 @f(ptr %p, i32 %v) {
                        entry:
                          %old = atomicrmw add ptr %p, i32 %v seq_cst
                          ret i32 %old
                        }
                    "#,
        );

        let symbols = SymbolBuilder::default()
            .build(
                &module,
                crate::util::resolve_machine_model(&module, crate::MachineModel::Auto)
                    .expect("failed to resolve machine model"),
                &crate::contracts::ContractMap::default(),
                &crate::UnsoundOptions::default(),
            )
            .expect("atomicrmw body should lower");
        let symbol = symbols.lookup("f").expect("missing function");

        match &symbol.value {
            SymbolValues::Stmt(body) => {
                let debug = format!("{body:?}");
                assert!(debug.contains("AtomicBlock"));
                assert!(debug.contains("Plus"));
            }
            value => panic!("expected statement body, got {value:?}"),
        }
    }

    #[test]
    fn lowers_cmpxchg_extractvalue_body() {
        let context = Context::create();
        let module = parse_module(
            &context,
            br#"
                        define i32 @f(ptr %p, i32 %cmp, i32 %new) {
                        entry:
                          %pair = cmpxchg ptr %p, i32 %cmp, i32 %new seq_cst seq_cst
                          %old = extractvalue { i32, i1 } %pair, 0
                          ret i32 %old
                        }
                    "#,
        );

        let symbols = SymbolBuilder::default()
            .build(
                &module,
                crate::util::resolve_machine_model(&module, crate::MachineModel::Auto)
                    .expect("failed to resolve machine model"),
                &crate::contracts::ContractMap::default(),
                &crate::UnsoundOptions::default(),
            )
            .expect("cmpxchg body should lower");
        let symbol = symbols.lookup("f").expect("missing function");

        match &symbol.value {
            SymbolValues::Stmt(body) => {
                let debug = format!("{body:?}");
                assert!(debug.contains("AtomicBlock"));
                assert!(debug.contains("cmpxchg_old"));
                assert!(debug.contains("Return"));
            }
            value => panic!("expected statement body, got {value:?}"),
        }
    }
}
