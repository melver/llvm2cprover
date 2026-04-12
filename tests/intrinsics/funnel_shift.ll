; ---
; description = "llvm funnel shifts lower as modulo-width rotates and cross-lane shifts"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [10 x i8] c"fshl/fshr\00"

declare void @__CPROVER_assert(i1, ptr)
declare i32 @llvm.fshl.i32(i32, i32, i32)
declare i32 @llvm.fshr.i32(i32, i32, i32)

define i32 @main() {
entry:
  %rotl = call i32 @llvm.fshl.i32(i32 1, i32 1, i32 1)
  %rotl_ok = icmp eq i32 %rotl, 2

  %cross_l = call i32 @llvm.fshl.i32(i32 0, i32 -2147483648, i32 1)
  %cross_l_ok = icmp eq i32 %cross_l, 1

  %rotr = call i32 @llvm.fshr.i32(i32 1, i32 1, i32 1)
  %rotr_ok = icmp eq i32 %rotr, -2147483648

  %cross_r = call i32 @llvm.fshr.i32(i32 0, i32 1, i32 1)
  %cross_r_ok = icmp eq i32 %cross_r, 0

  %wrap = call i32 @llvm.fshl.i32(i32 7, i32 7, i32 33)
  %wrap_ok = icmp eq i32 %wrap, 14

  %zero_l = call i32 @llvm.fshl.i32(i32 1, i32 2, i32 32)
  %zero_l_ok = icmp eq i32 %zero_l, 1

  %zero_r = call i32 @llvm.fshr.i32(i32 1, i32 2, i32 32)
  %zero_r_ok = icmp eq i32 %zero_r, 2

  %ok0 = and i1 %rotl_ok, %cross_l_ok
  %ok1 = and i1 %rotr_ok, %cross_r_ok
  %ok2 = and i1 %wrap_ok, %zero_l_ok
  %ok3 = and i1 %zero_r_ok, %ok2
  %ok4 = and i1 %ok0, %ok1
  %ok = and i1 %ok4, %ok3
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
