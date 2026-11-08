// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors

//! Lowering tests: inline IR through `SymbolBuilder`, checking the lowered GOTO bodies.

use cprover_bindings::goto_program::{SymbolValues, Type};
use inkwell::context::Context;

use crate::translator::symbols::SymbolBuilder;
use crate::translator::symbols::test_util::*;
use crate::translator::{Diagnostic, DiagnosticKind, UnsupportedFeature};

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
    .expect_err("unsound mode must not replace stateful inline asm results with nondeterminism");

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
fn lowers_relational_pointer_comparisons_via_size_t() {
    let symbols = build_symbols_from_ir(
        br#"
        define i1 @eq_only(ptr %a, ptr %b) {
        entry:
          %cmp = icmp eq ptr %a, %b
          ret i1 %cmp
        }

        define i1 @rel(ptr %a, ptr %b) {
        entry:
          %u = icmp ult ptr %a, %b
          %s = icmp slt ptr %a, %b
          %r = and i1 %u, %s
          ret i1 %r
        }
    "#,
    );

    let eq_debug = stmt_debug(&symbols, "eq_only");
    assert!(!eq_debug.contains("SizeT"));

    let rel_debug = stmt_debug(&symbols, "rel");
    assert!(rel_debug.contains("SizeT"));
    assert!(rel_debug.contains("SSizeT"));
    // Unsigned predicates compare in-bounds same-object offsets, so symex can fold them.
    assert!(rel_debug.contains("PointerOffset"));
    assert!(rel_debug.contains("ObjectSize"));
}

#[test]
fn rejects_non_integer_minmax_and_three_way_cmp_intrinsics() {
    for ir in [
        &br#"
            declare ptr @llvm.umin.p0(ptr, ptr)
            define ptr @f(ptr %a, ptr %b) {
              %r = call ptr @llvm.umin.p0(ptr %a, ptr %b)
              ret ptr %r
            }
        "#[..],
        &br#"
            declare i8 @llvm.ucmp.i8.p0(ptr, ptr)
            define i8 @f(ptr %a, ptr %b) {
              %r = call i8 @llvm.ucmp.i8.p0(ptr %a, ptr %b)
              ret i8 %r
            }
        "#[..],
    ] {
        let error = build_symbols_result_from_ir(ir)
            .expect_err("pointer min/max or three-way cmp must fail closed");
        let diagnostic = error
            .downcast_ref::<Diagnostic>()
            .expect("expected structured diagnostic");
        assert_eq!(
            diagnostic.kind,
            DiagnosticKind::UnsupportedFeature(UnsupportedFeature::Intrinsic)
        );
    }
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
fn preserves_cprover_assert_messages() {
    let symbols = build_symbols_from_ir(
        br#"
        @.str = private unnamed_addr constant [14 x i8] c"direct string\00"
        @.str.1 = private unnamed_addr constant [11 x i8] c"gep string\00"
        @.alias = private alias [14 x i8], ptr @.str
        @.gep_alias = private alias i8, ptr getelementptr inbounds ([11 x i8], ptr @.str.1, i64 0, i64 0)
        @.not_bytes = private unnamed_addr constant [2 x i32] [i32 65, i32 0]

        declare void @__CPROVER_assert(i1, ptr)

        define void @f(i1 %cond, ptr %dynamic) {
        entry:
          call void @__CPROVER_assert(i1 %cond, ptr @.str)
          call void @__CPROVER_assert(i1 %cond, ptr getelementptr inbounds ([11 x i8], ptr @.str.1, i64 0, i64 0))
          call void @__CPROVER_assert(i1 %cond, ptr @.alias)
          call void @__CPROVER_assert(i1 %cond, ptr @.gep_alias)
          call void @__CPROVER_assert(i1 %cond, ptr @.not_bytes)
          call void @__CPROVER_assert(i1 %cond, ptr %dynamic)
          ret void
        }
    "#,
    );

    let debug = stmt_debug(&symbols, "f");
    assert_eq!(debug.matches("msg: \"direct string\"").count(), 2);
    assert_eq!(debug.matches("msg: \"gep string\"").count(), 2);
    assert_eq!(debug.matches("msg: \"__CPROVER_assert\"").count(), 2);
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
fn lowers_noalias_scope_decl_as_nop() {
    let symbols = build_symbols_from_ir(
        br#"
        declare void @llvm.experimental.noalias.scope.decl(metadata)

        define void @f() {
        entry:
          tail call void @llvm.experimental.noalias.scope.decl(metadata !0)
          ret void
        }

        !0 = !{!1}
        !1 = distinct !{!1, !2, !"callee: %p"}
        !2 = distinct !{!2, !"callee"}
    "#,
    );

    let debug = stmt_debug(&symbols, "f");
    assert!(!debug.contains("noalias"));
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
    // LLVM permits `dst == src`; C `memcpy` does not. Only partial overlap is asserted, and
    // the copy is CBMC's `memmove`, without a branch on `dst != src`.
    assert!(debug.contains("memmove"));
    assert!(debug.contains("memcpy src/dst overlap"));
    assert!(!debug.contains("Ifthenelse"));
}

#[test]
fn single_use_compare_is_the_branch_condition() {
    let symbols = build_symbols_from_ir(
        br#"
        define i32 @f(ptr %p) {
        entry:
          %isnull = icmp eq ptr %p, null
          br i1 %isnull, label %a, label %b
        a:
          ret i32 0
        b:
          ret i32 1
        }
    "#,
    );
    // CBMC narrows `p`'s value set only on a guard that is the comparison itself.
    let debug = stmt_debug(&symbols, "f");
    assert!(debug.contains("Ifthenelse"));
    assert!(!debug.contains("isnull"));
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
fn types_allocation_reloaded_through_stack_slot() {
    // `-O0` shape: the result goes through a stack slot; only the reloaded pointer is used
    // as `%struct.S`. The call result gets that pointee, so `sizeof(S)` is annotated.
    let symbols = build_symbols_from_ir(
        br#"
        %struct.S = type { i64, ptr }
        declare ptr @malloc(i64)

        define ptr @f() {
        entry:
          %slot = alloca ptr
          %p = call ptr @malloc(i64 16)
          store ptr %p, ptr %slot
          %q = load ptr, ptr %slot
          %field = getelementptr inbounds %struct.S, ptr %q, i32 0, i32 1
          store ptr null, ptr %field
          ret ptr %q
        }
    "#,
    );
    let debug = stmt_debug(&symbols, "f");
    assert!(debug.contains("size_of_annotation: Some(StructTag(\"tag-struct.S\"))"));
}

#[test]
fn leaves_packed_struct_allocation_untyped() {
    // A packed struct may have unaligned members, which CBMC mishandles on straddling stores.
    let symbols = build_symbols_from_ir(
        br#"
        %struct.P = type <{ i8, i64 }>
        declare ptr @malloc(i64)

        define ptr @f() {
        entry:
          %p = call ptr @malloc(i64 9)
          %field = getelementptr inbounds %struct.P, ptr %p, i32 0, i32 1
          store i64 0, ptr %field
          ret ptr %p
        }
    "#,
    );
    assert!(!stmt_debug(&symbols, "f").contains("size_of_annotation: Some"));
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

#[test]
fn lowers_rust_panic_calls_to_assert_false() {
    let symbols = build_symbols_from_ir(
        br#"
        declare void @_RNvNtCs1234_4core9panicking18panic_bounds_check(i64, i64, ptr)
        declare void @_RNvNtNtCs1234_4core9panicking11panic_const23panic_const_div_by_zero(ptr)
        declare void @_RNvNtNtCs1234_4core5slice5index16slice_index_fail(i64, i64, i64, ptr)
        declare void @_ZN4core9panicking5panic17h0123456789abcdefE(ptr, i64, ptr)
        declare void @_ZN4core5slice5index16slice_index_fail17h0123456789abcdefE(i64, i64, i64, ptr)
        declare void @_RNvNtCskADBYZwJmSU_4core6result13unwrap_failed(ptr, i64, ptr, ptr, ptr)
        declare void @_RNvNtCskADBYZwJmSU_4core6option13unwrap_failed(ptr)
        declare void @_ZN4core6option13expect_failed17h0123456789abcdefE(ptr, i64, ptr)

        define void @f() {
        entry:
          call void @_RNvNtCs1234_4core9panicking18panic_bounds_check(i64 0, i64 0, ptr null)
          call void @_RNvNtNtCs1234_4core9panicking11panic_const23panic_const_div_by_zero(ptr null)
          call void @_RNvNtNtCs1234_4core5slice5index16slice_index_fail(i64 0, i64 0, i64 0, ptr null)
          call void @_ZN4core9panicking5panic17h0123456789abcdefE(ptr null, i64 0, ptr null)
          call void @_ZN4core5slice5index16slice_index_fail17h0123456789abcdefE(i64 0, i64 0, i64 0, ptr null)
          call void @_RNvNtCskADBYZwJmSU_4core6result13unwrap_failed(ptr null, i64 0, ptr null, ptr null, ptr null)
          call void @_RNvNtCskADBYZwJmSU_4core6option13unwrap_failed(ptr null)
          call void @_ZN4core6option13expect_failed17h0123456789abcdefE(ptr null, i64 0, ptr null)
          ret void
        }
    "#,
    );

    let debug = stmt_debug(&symbols, "f");
    assert_eq!(debug.matches("Assert {").count(), 8);
    assert!(debug.contains("Rust panic"));
    assert!(!debug.contains("FunctionCall"));
}

#[test]
fn lowers_byval_parameter_to_prologue_copy() {
    let symbols = build_symbols_from_ir(
        br#"
        %struct.S = type { i8, i64 }

        define void @callee(ptr byval(%struct.S) %p) {
        entry:
          store i8 0, ptr %p, align 8
          ret void
        }
    "#,
    );

    let struct_type = Type::struct_tag("struct.S");
    assert_eq!(
        symbols.lookup("callee::p").expect("formal param").typ,
        struct_type.clone().to_pointer()
    );
    assert_eq!(
        symbols.lookup("callee::byval0").expect("byval local").typ,
        struct_type
    );
    let debug = stmt_debug(&symbols, "callee");
    assert!(debug.contains("callee::byval0"));
}

#[test]
fn clamps_scalar_and_vector_shift_distances() {
    let symbols = build_symbols_from_ir(
        br#"
        define i32 @scalar_shifts(i32 %x, i32 %n) {
        entry:
          %a = shl i32 %x, %n
          %b = lshr i32 %a, %n
          %c = ashr i32 %b, %n
          ret i32 %c
        }

        define <2 x i32> @vector_shifts(<2 x i32> %x, <2 x i32> %n) {
        entry:
          %a = shl <2 x i32> %x, %n
          %b = lshr <2 x i32> %a, %n
          %c = ashr <2 x i32> %b, %n
          ret <2 x i32> %c
        }
    "#,
    );

    let scalar_debug = stmt_debug(&symbols, "scalar_shifts");
    assert!(scalar_debug.contains("Shl"));
    assert!(scalar_debug.contains("Lshr"));
    assert!(scalar_debug.contains("Ashr"));
    assert!(scalar_debug.contains("IntConstant(32)"));
    // An out-of-range distance is `poison`: a fresh local, not a `nondet` side effect.
    assert!(scalar_debug.contains("scalar_shifts::$poison::poison"));
    assert!(!scalar_debug.contains("Nondet"));

    let vector_debug = stmt_debug(&symbols, "vector_shifts");
    assert!(vector_debug.contains("Vector"));
    assert!(vector_debug.contains("Shl"));
    assert!(vector_debug.contains("Lshr"));
    assert!(vector_debug.contains("Ashr"));
    assert!(vector_debug.contains("vector_shifts::$poison::poison"));
}

#[test]
fn rejects_incompatible_cross_type_alias_call_signatures() {
    for ir in [
        &br#"
            @as_i64 = alias i64 (), ptr @returns_double
            define double @returns_double() {
            entry:
              ret double 1.5
            }
            define i64 @f() {
            entry:
              %r = call i64 @as_i64()
              ret i64 %r
            }
        "#[..],
        &br#"
            %struct.A = type { i64 }
            %struct.B = type { ptr }
            @as_a = alias %struct.A (%struct.A), ptr @id_b
            define %struct.B @id_b(%struct.B %x) {
            entry:
              ret %struct.B %x
            }
            define %struct.A @f(%struct.A %x) {
            entry:
              %r = call %struct.A @as_a(%struct.A %x)
              ret %struct.A %r
            }
        "#[..],
        &br#"
            @one_arg = alias void (i32), ptr @two_args
            define void @two_args(i32 %a, i32 %b) {
            entry:
              ret void
            }
            define void @f() {
            entry:
              call void @one_arg(i32 1)
              ret void
            }
        "#[..],
    ] {
        let err = build_symbols_result_from_ir(ir).expect_err("incompatible alias call must fail");
        let diag = err
            .downcast_ref::<Diagnostic>()
            .expect("structured diagnostic");
        assert_eq!(
            diag.kind,
            super::super::DiagnosticKind::UnsupportedFeature(UnsupportedFeature::Instruction)
        );
    }
}

#[test]
fn rejects_invalid_arity_and_operand_types_in_cprover_and_assume_builtins() {
    for ir in [
        &br#"
            declare void @__CPROVER_assume(i1, i32)
            define void @f() {
            entry:
              call void @__CPROVER_assume(i1 true, i32 0)
              ret void
            }
        "#[..],
        &br#"
            declare void @__CPROVER_assert(i1, ptr, i32)
            define void @f() {
            entry:
              call void @__CPROVER_assert(i1 true, ptr null, i32 0)
              ret void
            }
        "#[..],
        &br#"
            declare void @__CPROVER_assert(i1, i32)
            define void @f() {
            entry:
              call void @__CPROVER_assert(i1 true, i32 0)
              ret void
            }
        "#[..],
        &br#"
            declare void @__CPROVER_assume(double)
            define void @f() {
            entry:
              call void @__CPROVER_assume(double 1.0)
              ret void
            }
        "#[..],
        &br#"
            declare i32 @__CPROVER_nondet_int(i32)
            define i32 @f() {
            entry:
              %r = call i32 @__CPROVER_nondet_int(i32 0)
              ret i32 %r
            }
        "#[..],
        &br#"
            declare i64 @__CPROVER_OBJECT_SIZE(ptr, i64)
            define i64 @f(ptr %p) {
            entry:
              %r = call i64 @__CPROVER_OBJECT_SIZE(ptr %p, i64 8)
              ret i64 %r
            }
        "#[..],
        &br#"
            declare i64 @__CPROVER_OBJECT_SIZE(i64)
            define i64 @f(i64 %x) {
            entry:
              %r = call i64 @__CPROVER_OBJECT_SIZE(i64 %x)
              ret i64 %r
            }
        "#[..],
        &br#"
            declare i1 @__CPROVER_r_ok(ptr, double)
            define i1 @f(ptr %p) {
            entry:
              %r = call i1 @__CPROVER_r_ok(ptr %p, double 4.0)
              ret i1 %r
            }
        "#[..],
    ] {
        let err = build_symbols_result_from_ir(ir).expect_err("malformed builtin call must fail");
        let diag = err
            .downcast_ref::<Diagnostic>()
            .expect("structured diagnostic");
        assert_eq!(
            diag.kind,
            super::super::DiagnosticKind::UnsupportedFeature(UnsupportedFeature::Intrinsic)
        );
    }
}
