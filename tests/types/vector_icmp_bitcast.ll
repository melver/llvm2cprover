; ---
; description = "Vector icmp lowers lane-wise and can be bitcast to a scalar mask"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [17 x i8] c"vector icmp mask\00"

define i32 @main() {
entry:
  %cmp = icmp ne <4 x i8> <i8 64, i8 -91, i8 90, i8 -80>, <i8 64, i8 -91, i8 90, i8 -80>
  %bits = bitcast <4 x i1> %cmp to i4
  %ok = icmp eq i4 %bits, 0
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
