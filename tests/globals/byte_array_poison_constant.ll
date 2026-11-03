; ---
; description = "[N x i8] constant arrays with poison/undef/constant-expression lanes lower without crashing"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; An `[N x i8]` array with non-literal lanes such as `poison` (e.g. from
; compiler-generated switch lookup tables) is a `ConstantArray` rather than a
; `ConstantDataSequential`, so `LLVMGetAsString` (`ArrayValue::as_const_string`)
; must not be called on it.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@switch.table = private unnamed_addr constant [4 x i8] [i8 40, i8 poison, i8 poison, i8 16], align 4
@.msg = private unnamed_addr constant [2 x i8] [i8 65, i8 poison], align 1
@g = global i8 0
@mixed = private unnamed_addr constant [3 x i8] [i8 7, i8 undef, i8 ptrtoint (ptr @g to i8)], align 1

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %p0 = getelementptr [4 x i8], ptr @switch.table, i64 0, i64 0
  %v0 = load i8, ptr %p0
  %p3 = getelementptr [4 x i8], ptr @switch.table, i64 0, i64 3
  %v3 = load i8, ptr %p3
  %ok0 = icmp eq i8 %v0, 40
  %ok3 = icmp eq i8 %v3, 16
  %pm = getelementptr [3 x i8], ptr @mixed, i64 0, i64 0
  %vm = load i8, ptr %pm
  %okm = icmp eq i8 %vm, 7
  %ok03 = and i1 %ok0, %ok3
  %ok = and i1 %ok03, %okm
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
