; ---
; description = "llvm.is.constant reports manifest LLVM constants"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg_true = private unnamed_addr constant [11 x i8] c"const true\00"
@.msg_false = private unnamed_addr constant [12 x i8] c"const false\00"

declare i1 @llvm.is.constant.i32(i32)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %value = add i32 20, 22
  %const = call i1 @llvm.is.constant.i32(i32 42)
  %not_const = call i1 @llvm.is.constant.i32(i32 %value)
  call void @__CPROVER_assert(i1 %const, ptr @.msg_true)
  %ok = xor i1 %not_const, true
  call void @__CPROVER_assert(i1 %ok, ptr @.msg_false)
  ret i32 0
}
