; ---
; description = "shift by >= bit width is poison, not UB; a select that discards it must not trip CBMC's undefined-shift check"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; todo = "shl/lshr/ashr lowered to bare GOTO shifts; CBMC's default undefined-shift-check fires on speculated poison"
; ---
;
; LLVM LangRef: for shl/lshr/ashr, "If op2 is (statically or dynamically)
; equal to or larger than the number of bits in op1, this instruction returns
; a poison value". Poison is only UB once it reaches a side effect. At -O2
; LLVM routinely hoists the shift out of its `if (amount < width)` guard and
; keeps the guard as a `select`, which is exactly what clang emits for Csmith's
; safe_lshift_func_* wrappers (seeds 162, 244, 259, 264, 312, 413, 496). CBMC
; treats the GOTO `<<`/`>>` as C semantics and reports "shift distance too
; large" on a value the program never observes.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@amount = global i32 40, align 4

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %n = load i32, ptr @amount, align 4
  %big = icmp ugt i32 %n, 31
  %shl = shl nuw i32 1, %n
  %lshr = lshr i32 255, %n
  %ashr = ashr i32 -256, %n
  %a = select i1 %big, i32 1, i32 %shl
  %b = select i1 %big, i32 255, i32 %lshr
  %c = select i1 %big, i32 -256, i32 %ashr
  %sum = add i32 %a, %b
  %sum2 = add i32 %sum, %c
  %ok = icmp eq i32 %sum2, 0
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
