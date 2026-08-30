// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Coherent module-level symbol renaming before GOTO lowering.
//!
//! Renaming the LLVM values themselves makes LLVM update every reference and lets subsequent
//! lowering derive function-local identifiers and labels from the renamed containing function.
//! External declarations deliberately remain shared so independently translated variants retain
//! common unresolved dependencies and CBMC library bindings. LLVM intrinsics, `__CPROVER_*`
//! verifier builtins, and llvm2cprover's internal runtime namespace also stay shared.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use cprover_bindings::MachineModel;
use cprover_bindings::goto_program::SymbolTable;
use inkwell::module::Module;
use inkwell::values::AsValueRef;

#[derive(Clone, Copy)]
enum ModuleSymbol<'ctx> {
    Function(inkwell::values::FunctionValue<'ctx>),
    Global(inkwell::values::GlobalValue<'ctx>),
}

impl ModuleSymbol<'_> {
    fn set_name(self, name: &str) {
        match self {
            Self::Function(value) => {
                // FunctionValue does not expose the common Value::set_name wrapper in inkwell.
                // LLVMSetValueName2 updates all uses because they refer to the LLVM value, not its
                // textual spelling.
                unsafe {
                    inkwell::llvm_sys::core::LLVMSetValueName2(
                        value.as_value_ref(),
                        name.as_ptr().cast(),
                        name.len(),
                    );
                }
            }
            Self::Global(value) => value.set_name(name),
        }
    }
}

struct Rename<'ctx> {
    value: ModuleSymbol<'ctx>,
    original: String,
    output: String,
}

/// Prefix every non-reserved LLVM function and global definition in `module`.
///
/// The operation validates the complete mapping first and uses temporary names while applying it,
/// so an existing symbol whose name already begins with the requested prefix cannot perturb LLVM's
/// uniquing behavior. Invalid mappings fail before the module is changed.
pub(super) fn rename_module_symbols(
    module: &Module<'_>,
    machine_model: &MachineModel,
    prefix: &str,
) -> Result<()> {
    if prefix.is_empty() {
        return Ok(());
    }
    if prefix.contains('\0') {
        bail!("rename prefix contains a NUL byte");
    }

    let mut renames = Vec::new();
    let mut original_names = BTreeSet::new();
    let mut unrenamed_names = BTreeSet::new();
    for function in module.get_functions() {
        let original = function.get_name().to_string_lossy().into_owned();
        original_names.insert(original.clone());
        if function.count_basic_blocks() != 0 && !is_reserved(&original) {
            renames.push(Rename {
                value: ModuleSymbol::Function(function),
                output: format!("{prefix}{original}"),
                original,
            });
        } else {
            unrenamed_names.insert(original);
        }
    }
    for global in module.get_globals() {
        let original = global.get_name().to_string_lossy().into_owned();
        original_names.insert(original.clone());
        if !global.is_declaration() && !is_reserved(&original) {
            renames.push(Rename {
                value: ModuleSymbol::Global(global),
                output: format!("{prefix}{original}"),
                original,
            });
        } else {
            unrenamed_names.insert(original);
        }
    }

    validate_mapping(machine_model, &renames, &unrenamed_names)?;

    let mut temporary_names = BTreeSet::new();
    for (index, rename) in renames.iter().enumerate() {
        let mut temporary = format!("__llvm2cprover_rename_tmp_{index}");
        while original_names.contains(&temporary) || temporary_names.contains(&temporary) {
            temporary.push('_');
        }
        temporary_names.insert(temporary.clone());
        rename.value.set_name(&temporary);
    }
    for rename in renames {
        rename.value.set_name(&rename.output);
    }
    Ok(())
}

fn validate_mapping(
    machine_model: &MachineModel,
    renames: &[Rename<'_>],
    unrenamed_names: &BTreeSet<String>,
) -> Result<()> {
    let mut outputs = BTreeMap::<&str, &str>::new();
    for rename in renames {
        if is_reserved(&rename.output) {
            bail!(
                "renaming `{}` to `{}` would enter a reserved runtime or intrinsic namespace",
                rename.original,
                rename.output
            );
        }
        if let Some(previous) = outputs.insert(&rename.output, &rename.original) {
            bail!(
                "symbol rename collision: `{previous}` and `{}` would both become `{}`",
                rename.original,
                rename.output
            );
        }
        if unrenamed_names.contains(&rename.output) {
            bail!(
                "symbol rename collision: `{}` would become `{}`, which is an unrenamed declaration or reserved symbol",
                rename.original,
                rename.output
            );
        }
    }

    // A prefixed LLVM declaration must not be silently replaced by a GOTO builtin seeded before
    // module discovery. This check is independent of declarations already present in the module.
    let mut builtins = SymbolTable::new(machine_model.clone());
    super::symbols::insert_runtime_symbols(&mut builtins);
    for rename in renames {
        if builtins.lookup(rename.output.as_str()).is_some() {
            bail!(
                "renaming `{}` to `{}` would collide with a GOTO runtime symbol",
                rename.original,
                rename.output
            );
        }
    }
    Ok(())
}

fn is_reserved(name: &str) -> bool {
    name.starts_with("llvm.")
        || name.starts_with("__CPROVER_")
        || name.starts_with("__llvm2cprover_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;

    fn module_from_ir<'ctx>(context: &'ctx Context, ir: &str) -> Module<'ctx> {
        context
            .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
                ir.as_bytes(),
                "rename",
            ))
            .expect("failed to parse test IR")
    }

    fn rename(module: &Module<'_>, prefix: &str) -> Result<()> {
        let machine_model = crate::util::resolve_machine_model(module, crate::MachineModel::Auto)
            .expect("failed to resolve test machine model");
        rename_module_symbols(module, &machine_model, prefix)
    }

    #[test]
    fn renames_definitions_and_uses_but_shares_external_declarations() {
        let context = Context::create();
        let module = module_from_ir(
            &context,
            r#"
                @g = global i32 1
                @external_g = external global i32
                @fp = global ptr @callee
                declare i32 @external(i32)
                declare void @llvm.assume(i1)
                declare void @__CPROVER_assert(i1, ptr)
                define i32 @callee(i32 %x) {
                entry:
                  %value = load i32, ptr @g
                  %shared = load i32, ptr @external_g
                  %sum = call i32 @external(i32 %shared)
                  %done = icmp eq i32 %x, 0
                  br i1 %done, label %exit, label %recurse
                recurse:
                  %next = sub i32 %x, 1
                  %recursive = call i32 @callee(i32 %next)
                  ret i32 %recursive
                exit:
                  ret i32 %sum
                }
            "#,
        );

        rename(&module, "old_").expect("renaming should succeed");
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("@old_g = global"));
        assert!(ir.contains("@old_fp = global ptr @old_callee"));
        assert!(ir.contains("@external_g = external global"));
        assert!(ir.contains("declare i32 @external"));
        assert!(ir.contains("define i32 @old_callee"));
        assert!(ir.contains("load i32, ptr @old_g"));
        assert!(ir.contains("load i32, ptr @external_g"));
        assert!(ir.contains("call i32 @external"));
        assert!(ir.contains("call i32 @old_callee"));
        assert!(ir.contains("declare void @llvm.assume"));
        assert!(ir.contains("declare void @__CPROVER_assert"));
    }

    #[test]
    fn rejects_collision_with_unrenamed_external_declaration() {
        let context = Context::create();
        let module = module_from_ir(
            &context,
            r#"
                declare void @old_f()
                define void @f() {
                entry:
                  ret void
                }
            "#,
        );

        let error = rename(&module, "old_").expect_err("declaration collision must fail");
        assert!(error.to_string().contains("unrenamed declaration"));
        assert!(module.get_function("f").is_some());
        assert!(module.get_function("old_f").is_some());
    }

    #[test]
    fn rejects_collision_with_translator_runtime_symbol() {
        let context = Context::create();
        let module = module_from_ir(
            &context,
            r#"
                define void @t() {
                entry:
                  ret void
                }
            "#,
        );

        let error = rename(&module, "pthread_").expect_err("runtime collision must fail");
        assert!(error.to_string().contains("GOTO runtime symbol"));
        assert!(module.get_function("t").is_some());
    }

    #[test]
    fn rejects_collision_with_unrenamed_reserved_symbol() {
        let context = Context::create();
        let module = module_from_ir(
            &context,
            r#"
                declare void @__CPROVER_assert(i1, ptr)
                define void @assert() {
                entry:
                  ret void
                }
            "#,
        );

        let error = rename(&module, "__CPROVER_").expect_err("reserved collision must fail");
        assert!(error.to_string().contains("reserved"));
        assert!(module.get_function("assert").is_some());
    }

    #[test]
    fn two_phase_rename_handles_existing_prefixed_names() {
        let context = Context::create();
        let module = module_from_ir(
            &context,
            r#"
                define void @f() { ret void }
                define void @p_f() { ret void }
            "#,
        );

        rename(&module, "p_").expect("renaming should succeed");
        assert!(module.get_function("p_f").is_some());
        assert!(module.get_function("p_p_f").is_some());
        assert_eq!(module.get_functions().count(), 2);
    }
}
