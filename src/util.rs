// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Shared translation utilities.
//!
//! This module keeps small cross-cutting helpers out of the lowering code:
//!
//! - input-format detection
//! - output-path selection
//! - machine-model inference
//! - best-effort source-location recovery
//! - LLVM-name normalization for GOTO identifiers

use std::path::{Path, PathBuf};
use std::slice;

use anyhow::{Result, bail};
use cprover_bindings::goto_program::Location;
use cprover_bindings::{MachineModel as GotoMachineModel, RoundingMode};
use inkwell::llvm_sys::core::{
    LLVMGetDebugLocColumn, LLVMGetDebugLocDirectory, LLVMGetDebugLocFilename, LLVMGetDebugLocLine,
    LLVMGetGlobalParent, LLVMGetSourceFileName,
};
use inkwell::module::Module;
use inkwell::values::{AsValueRef, FunctionValue, InstructionValue};

use crate::{InputFormat, MachineModel, TranslationInput};

/// Resolve the effective machine model for one module, honoring explicit overrides.
pub fn resolve_machine_model(
    module: &Module<'_>,
    requested: MachineModel,
) -> Result<GotoMachineModel> {
    let model = match requested {
        MachineModel::Auto => infer_machine_model(module)?,
        model => model,
    };
    let triple = module
        .get_triple()
        .as_str()
        .to_str()
        .unwrap_or_default()
        .to_owned();
    let arch = triple_arch(&triple);
    let big_endian = is_big_endian(module);
    Ok(machine_model(model, arch, big_endian))
}

/// Infer the abstract machine model from the LLVM module's triple and data layout.
pub fn infer_machine_model(module: &Module<'_>) -> Result<MachineModel> {
    let triple = module
        .get_triple()
        .as_str()
        .to_str()
        .map_err(|_| anyhow::anyhow!("failed to decode target triple"))?
        .to_ascii_lowercase();
    match pointer_width(module)? {
        32 => Ok(MachineModel::Ilp32),
        64 if triple.contains("windows") => Ok(MachineModel::Llp64),
        64 => Ok(MachineModel::Lp64),
        width => bail!("unsupported target pointer width {width}"),
    }
}

/// Map the public machine model onto the `cprover_bindings` equivalent.
pub fn machine_model(
    model: MachineModel,
    architecture: String,
    is_big_endian: bool,
) -> GotoMachineModel {
    let mut machine = common_machine_model(architecture, is_big_endian);
    match model {
        MachineModel::Auto => unreachable!("auto must be resolved before building a symbol table"),
        MachineModel::Ilp32 => {
            machine.long_int_width = 32;
            machine.memory_operand_size = 4;
            machine.pointer_width = 32;
            machine.word_size = 32;
            machine
        }
        MachineModel::Lp64 => machine,
        MachineModel::Llp64 => {
            machine.long_double_width = 64;
            machine.long_int_width = 32;
            machine.wchar_t_width = 16;
            machine
        }
    }
}

fn common_machine_model(architecture: String, is_big_endian: bool) -> GotoMachineModel {
    GotoMachineModel {
        alignment: 1,
        architecture,
        bool_width: 8,
        char_is_unsigned: false,
        char_width: 8,
        double_width: 64,
        float_width: 32,
        int_width: 32,
        is_big_endian,
        long_double_width: 128,
        long_int_width: 64,
        long_long_int_width: 64,
        memory_operand_size: 8,
        null_is_zero: true,
        pointer_width: 64,
        rounding_mode: RoundingMode::ToNearest,
        short_int_width: 16,
        single_width: 32,
        wchar_t_is_unsigned: false,
        wchar_t_width: 32,
        word_size: 64,
    }
}

/// Pick the `.goto` output path for a translation request.
pub fn output_path(input: &TranslationInput, output: Option<&Path>) -> PathBuf {
    output.map_or_else(|| default_output_path(input), Path::to_path_buf)
}

/// Detect the LLVM input encoding, respecting any explicit caller override.
pub fn detect_input_format(
    path: Option<&PathBuf>,
    requested: InputFormat,
    bytes: &[u8],
) -> Result<InputFormat> {
    match requested {
        InputFormat::Auto => autodetect_input_format(path, bytes),
        format => Ok(format),
    }
}

fn default_output_path(input: &TranslationInput) -> PathBuf {
    match input {
        TranslationInput::Path(path) => {
            let mut path = path.clone();
            path.set_extension("goto");
            path
        }
        TranslationInput::Stdin => PathBuf::from("a.goto"),
    }
}

fn autodetect_input_format(path: Option<&PathBuf>, bytes: &[u8]) -> Result<InputFormat> {
    if let Some(extension) = path
        .and_then(|path| path.extension())
        .and_then(|ext| ext.to_str())
    {
        match extension {
            "bc" => return Ok(InputFormat::Bitcode),
            "ll" => return Ok(InputFormat::LlvmIr),
            _ => {}
        }
    }

    if bytes.starts_with(b"BC\xc0\xde") {
        return Ok(InputFormat::Bitcode);
    }

    if std::str::from_utf8(bytes).is_ok() {
        return Ok(InputFormat::LlvmIr);
    }

    bail!("failed to auto-detect LLVM input format")
}

fn pointer_width(module: &Module<'_>) -> Result<u32> {
    let data_layout = module.get_data_layout();
    let data_layout = data_layout.as_str().to_str().unwrap_or_default();
    if !data_layout.is_empty() {
        for token in data_layout.split('-') {
            if let Some(width) = token.strip_prefix("p:") {
                let bits = width.split(':').next().unwrap_or_default();
                return bits.parse().map_err(|_| {
                    anyhow::anyhow!("failed to parse pointer width from data layout")
                });
            }
        }
    }
    let triple = module.get_triple();
    let arch = triple_arch(triple.as_str().to_str().unwrap_or_default());
    if matches!(
        arch.as_str(),
        "x86_64" | "aarch64" | "riscv64" | "powerpc64" | "powerpc64le" | "s390x"
    ) {
        return Ok(64);
    }
    if matches!(
        arch.as_str(),
        "i386" | "i486" | "i586" | "i686" | "arm" | "thumb" | "mips" | "riscv32"
    ) {
        return Ok(32);
    }
    Ok(64)
}

fn is_big_endian(module: &Module<'_>) -> bool {
    let data_layout = module.get_data_layout();
    let bytes = data_layout.as_str().to_bytes();
    if let Some(byte) = bytes.first() {
        return *byte == b'E';
    }
    let triple = module.get_triple();
    triple
        .as_str()
        .to_str()
        .map(|triple| triple.contains("powerpc64") && !triple.contains("le"))
        .unwrap_or(false)
}

fn triple_arch(triple: &str) -> String {
    triple.split('-').next().unwrap_or("unknown").to_owned()
}

/// Normalize LLVM-derived names into the restricted identifier shape used in GOTO symbols.
pub fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "tmp".to_owned()
    } else {
        out
    }
}

/// Recover a best-effort source location for one LLVM instruction.
pub fn instruction_location(instruction: InstructionValue<'_>, function_name: &str) -> Location {
    llvm_value_location(instruction, Some(function_name)).unwrap_or_else(|| {
        instruction
            .get_parent()
            .and_then(|block| block.get_parent())
            .map_or_else(Location::none, fallback_function_location)
    })
}

/// Recover a best-effort source location for a function from its first tagged instruction.
pub fn function_location(function: FunctionValue<'_>) -> Location {
    let function_name = function.get_name().to_string_lossy();
    for block in function.get_basic_blocks() {
        for instruction in block.get_instructions() {
            if let Some(location) = llvm_value_location(instruction, Some(function_name.as_ref())) {
                return location;
            }
        }
    }
    fallback_function_location(function)
}

fn llvm_value_location<V: AsValueRef>(value: V, function_name: Option<&str>) -> Option<Location> {
    let value = value.as_value_ref();
    let line = unsafe { LLVMGetDebugLocLine(value) };
    if line == 0 {
        return None;
    }

    let mut file_len = 0;
    let file = unsafe { LLVMGetDebugLocFilename(value, &mut file_len) };
    if file.is_null() || file_len == 0 {
        return None;
    }

    let mut dir_len = 0;
    let dir = unsafe { LLVMGetDebugLocDirectory(value, &mut dir_len) };
    // LLVM splits debug paths into directory + file. Recombine them here so downstream
    // diagnostics and GOTO locations carry one stable path string.
    let file = unsafe {
        String::from_utf8_lossy(slice::from_raw_parts(file.cast::<u8>(), file_len as usize))
    }
    .into_owned();
    let dir = if dir.is_null() || dir_len == 0 {
        String::new()
    } else {
        unsafe {
            String::from_utf8_lossy(slice::from_raw_parts(dir.cast::<u8>(), dir_len as usize))
        }
        .into_owned()
    };
    let file = if dir.is_empty() || file.starts_with('/') {
        file
    } else {
        format!("{dir}/{file}")
    };
    let col = unsafe { LLVMGetDebugLocColumn(value) };
    let col = (col != 0).then_some(u64::from(col));

    Some(Location::new(
        file,
        function_name,
        u64::from(line),
        col,
        u64::from(line),
        col,
        &[],
    ))
}

fn fallback_function_location(function: FunctionValue<'_>) -> Location {
    let function_name = function.get_name().to_string_lossy().into_owned();
    // Some LLVM inputs omit debug metadata entirely. Emit a synthetic file:1 location from the
    // module source filename so CBMC still sees non-nil source locations on generated symbols.
    Location::new(
        module_source_file(function).unwrap_or_else(|| "<llvm>".to_owned()),
        Some(function_name),
        1_u64,
        None,
        1_u64,
        None,
        &[],
    )
}

fn module_source_file(function: FunctionValue<'_>) -> Option<String> {
    let mut len = 0;
    let module = unsafe { LLVMGetGlobalParent(function.as_value_ref()) };
    if module.is_null() {
        return None;
    }
    let ptr = unsafe { LLVMGetSourceFileName(module, &mut len) };
    if ptr.is_null() || len == 0 {
        return None;
    }
    let file = unsafe { slice::from_raw_parts(ptr.cast::<u8>(), len) };
    let file = String::from_utf8_lossy(file).into_owned();
    (!file.is_empty()).then_some(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;

    const DEBUG_IR: &str = r#"
        define dso_local i32 @f(i32 noundef %0) #0 !dbg !10 {
          %2 = alloca i32, align 4
          store i32 %0, ptr %2, align 4
          %3 = load i32, ptr %2, align 4, !dbg !18
          %4 = add nsw i32 %3, 1, !dbg !19
          ret i32 %4, !dbg !20
        }

        attributes #0 = { noinline nounwind sspstrong uwtable }

        !llvm.dbg.cu = !{!0}
        !llvm.module.flags = !{!2, !3}
        !0 = distinct !DICompileUnit(language: DW_LANG_C11, file: !1, producer: "clang", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)
        !1 = !DIFile(filename: "/tmp/loc_test.c", directory: "")
        !2 = !{i32 7, !"Dwarf Version", i32 5}
        !3 = !{i32 2, !"Debug Info Version", i32 3}
        !10 = distinct !DISubprogram(name: "f", scope: !11, file: !11, line: 1, type: !12, scopeLine: 1, spFlags: DISPFlagDefinition, unit: !0)
        !11 = !DIFile(filename: "/tmp/loc_test.c", directory: "")
        !12 = !DISubroutineType(types: !13)
        !13 = !{!14, !14}
        !14 = !DIBasicType(name: "int", size: 32, encoding: DW_ATE_signed)
        !18 = !DILocation(line: 1, column: 23, scope: !10)
        !19 = !DILocation(line: 1, column: 25, scope: !10)
        !20 = !DILocation(line: 1, column: 16, scope: !10)
    "#;

    fn parse_module<'ctx>(context: &'ctx Context, ir: &str) -> inkwell::module::Module<'ctx> {
        let buffer = MemoryBuffer::create_from_memory_range_copy(ir.as_bytes(), "debug");
        context
            .create_module_from_ir(buffer)
            .expect("failed to parse debug IR")
    }

    #[test]
    fn stdin_defaults_to_a_goto() {
        assert_eq!(
            output_path(&TranslationInput::Stdin, None),
            PathBuf::from("a.goto")
        );
    }

    #[test]
    fn file_input_replaces_extension() {
        assert_eq!(
            output_path(&TranslationInput::Path(PathBuf::from("input.bc")), None),
            PathBuf::from("input.goto")
        );
    }

    #[test]
    fn detects_bitcode_magic() {
        assert_eq!(
            detect_input_format(None, InputFormat::Auto, b"BC\xc0\xde...").expect("detect"),
            InputFormat::Bitcode
        );
    }

    #[test]
    fn detects_text_ir_from_utf8() {
        assert_eq!(
            detect_input_format(None, InputFormat::Auto, b"define void @f() {\n}\n")
                .expect("detect"),
            InputFormat::LlvmIr
        );
    }

    #[test]
    fn explicit_format_overrides_detection() {
        assert_eq!(
            detect_input_format(None, InputFormat::LlvmIr, b"BC\xc0\xde").expect("detect"),
            InputFormat::LlvmIr
        );
    }

    #[test]
    fn infers_lp64_from_linux_x86_64_module() {
        let context = Context::create();
        let module = parse_module(
            &context,
            r#"
                target datalayout = "e-m:e-p:64:64-i64:64-i128:128-n8:16:32:64-S128"
                target triple = "x86_64-pc-linux-gnu"

                define void @f() {
                entry:
                  ret void
                }
            "#,
        );

        assert_eq!(
            infer_machine_model(&module).expect("detect"),
            MachineModel::Lp64
        );
        assert_eq!(
            resolve_machine_model(&module, MachineModel::Auto)
                .expect("resolve")
                .pointer_width,
            64
        );
    }

    #[test]
    fn infers_llp64_from_windows_x86_64_module() {
        let context = Context::create();
        let module = parse_module(
            &context,
            r#"
                target datalayout = "e-m:w-p:64:64-i64:64-i128:128-n8:16:32:64-S128"
                target triple = "x86_64-pc-windows-msvc"

                define void @f() {
                entry:
                  ret void
                }
            "#,
        );

        let model = resolve_machine_model(&module, MachineModel::Auto).expect("resolve");
        assert_eq!(
            infer_machine_model(&module).expect("detect"),
            MachineModel::Llp64
        );
        assert_eq!(model.pointer_width, 64);
        assert_eq!(model.long_int_width, 32);
        assert_eq!(model.wchar_t_width, 16);
    }

    #[test]
    fn extracts_instruction_debug_locations() {
        let context = Context::create();
        let module = parse_module(&context, DEBUG_IR);
        let function = module.get_function("f").expect("function");
        let add = function
            .get_first_basic_block()
            .expect("block")
            .get_instructions()
            .nth(2)
            .expect("add");

        let location = instruction_location(add, "f");

        assert_eq!(location.filename().as_deref(), Some("/tmp/loc_test.c"));
        assert_eq!(location.start_line(), Some(1));
    }

    #[test]
    fn extracts_function_debug_locations_from_first_tagged_instruction() {
        let context = Context::create();
        let module = parse_module(&context, DEBUG_IR);
        let function = module.get_function("f").expect("function");
        let location = function_location(function);

        assert_eq!(location.filename().as_deref(), Some("/tmp/loc_test.c"));
        assert_eq!(location.start_line(), Some(1));
    }

    #[test]
    fn falls_back_to_module_source_filename_without_debug_info() {
        let context = Context::create();
        let module = parse_module(
            &context,
            r#"
                source_filename = "/tmp/no_debug.c"

                define i32 @f() {
                entry:
                  ret i32 0
                }
            "#,
        );
        let function = module.get_function("f").expect("function");
        let instruction = function
            .get_first_basic_block()
            .expect("block")
            .get_first_instruction()
            .expect("instruction");

        let function_location = function_location(function);
        let instruction_location = instruction_location(instruction, "f");

        assert_eq!(
            function_location.filename().as_deref(),
            Some("/tmp/no_debug.c")
        );
        assert_eq!(function_location.start_line(), Some(1));
        assert_eq!(
            instruction_location.filename().as_deref(),
            Some("/tmp/no_debug.c")
        );
        assert_eq!(instruction_location.start_line(), Some(1));
    }
}
