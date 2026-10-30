; ---
; description = "llvm.ucmp / llvm.scmp lower to -1/0/1 in the result width, per lane for vectors"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)
declare i32 @__CPROVER_nondet_int()
declare i64 @__CPROVER_nondet_ulong()
declare i32 @llvm.ucmp.i32.i64(i64, i64)
declare i8 @llvm.scmp.i8.i32(i32, i32)
declare i2 @llvm.ucmp.i2.i8(i8, i8)
declare i128 @llvm.scmp.i128.i1(i1, i1)
declare <2 x i8> @llvm.ucmp.v2i8.v2i32(<2 x i32>, <2 x i32>)
declare <2 x i8> @llvm.scmp.v2i8.v2i32(<2 x i32>, <2 x i32>)

@.ucmp = private unnamed_addr constant [5 x i8] c"ucmp\00"
@.scmp = private unnamed_addr constant [5 x i8] c"scmp\00"
@.narrow = private unnamed_addr constant [7 x i8] c"narrow\00"
@.wide = private unnamed_addr constant [5 x i8] c"wide\00"
@.vector = private unnamed_addr constant [7 x i8] c"vector\00"

define i32 @main() {
entry:
  ; ucmp agrees with ult/ugt for all inputs, and exactly one of -1/0/1 holds.
  %a = call i64 @__CPROVER_nondet_ulong()
  %b = call i64 @__CPROVER_nondet_ulong()
  %u = call i32 @llvm.ucmp.i32.i64(i64 %a, i64 %b)
  %ult = icmp ult i64 %a, %b
  %ugt = icmp ugt i64 %a, %b
  %u.neg = icmp eq i32 %u, -1
  %u.pos = icmp eq i32 %u, 1
  %u.zero = icmp eq i32 %u, 0
  %u.ok1 = icmp eq i1 %u.neg, %ult
  %u.ok2 = icmp eq i1 %u.pos, %ugt
  %u.any = or i1 %u.neg, %u.pos
  %u.ok3 = xor i1 %u.any, %u.zero
  %u.ok12 = and i1 %u.ok1, %u.ok2
  %u.ok = and i1 %u.ok12, %u.ok3
  call void @__CPROVER_assert(i1 %u.ok, ptr @.ucmp)

  ; scmp agrees with slt/sgt for all inputs; the unsigned view would disagree
  ; whenever the signs differ.
  %c = call i32 @__CPROVER_nondet_int()
  %d = call i32 @__CPROVER_nondet_int()
  %s = call i8 @llvm.scmp.i8.i32(i32 %c, i32 %d)
  %slt = icmp slt i32 %c, %d
  %sgt = icmp sgt i32 %c, %d
  %s.neg = icmp eq i8 %s, -1
  %s.pos = icmp eq i8 %s, 1
  %s.ok1 = icmp eq i1 %s.neg, %slt
  %s.ok2 = icmp eq i1 %s.pos, %sgt
  %s.ok = and i1 %s.ok1, %s.ok2
  call void @__CPROVER_assert(i1 %s.ok, ptr @.scmp)

  ; Narrowest legal result: i2 holds -1 as 0b11.
  %n = call i2 @llvm.ucmp.i2.i8(i8 0, i8 255)
  %n.ok = icmp eq i2 %n, -1
  call void @__CPROVER_assert(i1 %n.ok, ptr @.narrow)

  ; i1 operands: true is -1 signed, so scmp(true, false) = -1, widened to i128.
  %w = call i128 @llvm.scmp.i128.i1(i1 true, i1 false)
  %w.ok = icmp eq i128 %w, -1
  call void @__CPROVER_assert(i1 %w.ok, ptr @.wide)

  ; Vector overloads compare lane by lane.
  %vu = call <2 x i8> @llvm.ucmp.v2i8.v2i32(<2 x i32> <i32 1, i32 -1>, <2 x i32> <i32 2, i32 1>)
  %vs = call <2 x i8> @llvm.scmp.v2i8.v2i32(<2 x i32> <i32 1, i32 -1>, <2 x i32> <i32 2, i32 1>)
  %vu.ok = icmp eq <2 x i8> %vu, <i8 -1, i8 1>
  %vs.ok = icmp eq <2 x i8> %vs, <i8 -1, i8 -1>
  %v.ok = and <2 x i1> %vu.ok, %vs.ok
  %v.ok0 = extractelement <2 x i1> %v.ok, i64 0
  %v.ok1 = extractelement <2 x i1> %v.ok, i64 1
  %v.ok01 = and i1 %v.ok0, %v.ok1
  call void @__CPROVER_assert(i1 %v.ok01, ptr @.vector)
  ret i32 0
}
