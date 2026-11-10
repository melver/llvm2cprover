; ---
; description = "llvm.experimental.cttz.elts of an all-false mask with is_zero_poison is unconstrained"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

declare i32 @llvm.experimental.cttz.elts.i32.v4i1(<4 x i1>, i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %p = call i32 @llvm.experimental.cttz.elts.i32.v4i1(<4 x i1> zeroinitializer, i1 true)
  %ok = icmp eq i32 %p, 4
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
