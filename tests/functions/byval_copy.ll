; ---
; description = "byval parameters are callee-owned copies; writes must not reach the caller's object"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; LLVM LangRef: "The byval argument attribute ... indicates that the pointer
; parameter should really be passed by value to the function. ... the callee
; receives a copy". Clang (x86_64, -O0) elides the caller-side copy when the
; argument is an lvalue, so it passes the global's address directly, e.g.
; `func_23(g_29[0], ...)` becomes `call @func_23(ptr byval(%struct.S0) @g_29)`.
; The translator currently passes the raw pointer, so the callee's
; `p.f0 = 0` is observable in `@g` afterwards. Found by Csmith (seed 400).

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

%struct.S = type { i8, i64 }

@g = internal global %struct.S { i8 77, i64 5 }, align 8

declare void @__CPROVER_assert(i1, ptr)

define internal void @callee(ptr noundef byval(%struct.S) align 8 %p) {
entry:
  %f0 = getelementptr inbounds %struct.S, ptr %p, i32 0, i32 0
  store i8 0, ptr %f0, align 8
  ret void
}

define i32 @main() {
entry:
  call void @callee(ptr noundef byval(%struct.S) align 8 @g)
  %v = load i8, ptr @g, align 8
  %ok = icmp eq i8 %v, 77
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
