; ---
; description = "constant-expression float bitcasts reinterpret the bits, like the instruction"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; LLVM cannot fold a bitcast of a global's address, so it stays a constant expression.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@g = global i32 0
@f = global float bitcast (i32 ptrtoint (ptr @g to i32) to float)

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %v = load float, ptr @f
  %bits = bitcast float %v to i32
  %addr = ptrtoint ptr @g to i32
  %ok = icmp eq i32 %bits, %addr
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
