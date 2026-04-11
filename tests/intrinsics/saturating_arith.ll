; ---
; description = "llvm saturating integer arithmetic lowers through overflow checks"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [11 x i8] c"saturating\00"

declare void @__CPROVER_assert(i1, ptr)

declare i8 @llvm.uadd.sat.i8(i8, i8)
declare i8 @llvm.usub.sat.i8(i8, i8)
declare i8 @llvm.sadd.sat.i8(i8, i8)
declare i8 @llvm.ssub.sat.i8(i8, i8)
declare i64 @llvm.uadd.sat.i64(i64, i64)

define i32 @main() {
entry:
  %uadd = call i8 @llvm.uadd.sat.i8(i8 250, i8 10)
  %usub = call i8 @llvm.usub.sat.i8(i8 3, i8 9)
  %sadd_pos = call i8 @llvm.sadd.sat.i8(i8 120, i8 20)
  %sadd_neg = call i8 @llvm.sadd.sat.i8(i8 -120, i8 -20)
  %ssub_pos = call i8 @llvm.ssub.sat.i8(i8 120, i8 -20)
  %ssub_neg = call i8 @llvm.ssub.sat.i8(i8 -120, i8 20)
  %uadd64 = call i64 @llvm.uadd.sat.i64(i64 -2, i64 17)

  %ok1 = icmp eq i8 %uadd, -1
  %ok2 = icmp eq i8 %usub, 0
  %ok3 = icmp eq i8 %sadd_pos, 127
  %ok4 = icmp eq i8 %sadd_neg, -128
  %ok5 = icmp eq i8 %ssub_pos, 127
  %ok6 = icmp eq i8 %ssub_neg, -128
  %ok7 = icmp eq i64 %uadd64, -1

  %all1 = and i1 %ok1, %ok2
  %all2 = and i1 %all1, %ok3
  %all3 = and i1 %all2, %ok4
  %all4 = and i1 %all3, %ok5
  %all5 = and i1 %all4, %ok6
  %all6 = and i1 %all5, %ok7
  call void @__CPROVER_assert(i1 %all6, ptr @.msg)
  ret i32 0
}
