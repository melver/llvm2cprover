; ---
; description = "symbolic llvm.umul.with.overflow respects equality-constrained operands"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg1 = private unnamed_addr constant [20 x i8] c"overflow flag clear\00", align 1
@.msg2 = private unnamed_addr constant [16 x i8] c"product matches\00", align 1

declare i64 @__CPROVER_nondet_ulong()
declare void @__CPROVER_assume(i32)
declare void @__CPROVER_assert(i32, ptr)
declare { i64, i1 } @llvm.umul.with.overflow.i64(i64, i64)

define i32 @main() {
entry:
  %x = call i64 @__CPROVER_nondet_ulong()
  %y = call i64 @__CPROVER_nondet_ulong()

  %xok = icmp eq i64 %x, 9223372036854
  %xok_i32 = zext i1 %xok to i32
  call void @__CPROVER_assume(i32 %xok_i32)

  %yok = icmp eq i64 %y, 1048800
  %yok_i32 = zext i1 %yok to i32
  call void @__CPROVER_assume(i32 %yok_i32)

  %pair = call { i64, i1 } @llvm.umul.with.overflow.i64(i64 %x, i64 %y)
  %prod = extractvalue { i64, i1 } %pair, 0
  %ov = extractvalue { i64, i1 } %pair, 1

  %ov_clear = xor i1 %ov, true
  %ov_i32 = zext i1 %ov_clear to i32
  call void @__CPROVER_assert(i32 %ov_i32, ptr @.msg1)

  %ok = icmp eq i64 %prod, 9673472592252475200
  %ok_i32 = zext i1 %ok to i32
  call void @__CPROVER_assert(i32 %ok_i32, ptr @.msg2)

  ret i32 0
}
