; ---
; description = "Relational pointer icmp compares flat integer addresses without C same-object UB"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; LLVM LangRef defines pointer `icmp` as comparing pointer values as integers.
; Optimizer passes such as LoopVersioning/LoopVectorize emit cross-object range
; checks (`p0 >= p1 + n || p0 + n <= p1`) that are UB in C (`same object
; violation` under CBMC's pointer `<`/`<=`/`>`/`>=`, which also compare only
; `POINTER_OFFSET` and ignore `POINTER_OBJECT`).

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@g_a = internal unnamed_addr global i32 1, align 4
@g_b = internal unnamed_addr global i32 2, align 4
@arr = internal global [4 x i32] zeroinitializer, align 4

declare void @__CPROVER_assert(i1, ptr)
declare i1 @__CPROVER_nondet_bool()
declare i64 @__CPROVER_nondet_u64()

define i32 @main() {
entry:
  %local = alloca i64, align 8

  ; LoopVersioning disjointness idiom on distinct 4-byte globals (Csmith seed 30266).
  %a_end = getelementptr inbounds nuw i8, ptr @g_a, i64 4
  %b_end = getelementptr inbounds nuw i8, ptr @g_b, i64 4
  %after = icmp uge ptr @g_a, %b_end
  %before = icmp ule ptr %a_end, @g_b
  %disjoint_globals = or i1 %after, %before
  call void @__CPROVER_assert(i1 %disjoint_globals, ptr null)

  ; Disjointness between a stack allocation (different pointee type) and a global.
  %local_end = getelementptr inbounds nuw i8, ptr %local, i64 8
  %stack_after = icmp uge ptr %local, %a_end
  %stack_before = icmp ule ptr %local_end, @g_a
  %disjoint_stack = or i1 %stack_after, %stack_before
  call void @__CPROVER_assert(i1 %disjoint_stack, ptr null)

  ; Within-object ordering, including one-past-the-end.
  %p1 = getelementptr inbounds i32, ptr @arr, i64 1
  %p4 = getelementptr inbounds i32, ptr @arr, i64 4
  %in_lt = icmp ult ptr @arr, %p1
  %in_le = icmp ule ptr %p1, %p4
  %in_gt = icmp ugt ptr %p4, @arr
  %in_ok1 = and i1 %in_lt, %in_le
  %in_ok = and i1 %in_ok1, %in_gt
  call void @__CPROVER_assert(i1 %in_ok, ptr null)

  ; Unsigned and signed pointer predicates agree with `ptrtoint` + integer `icmp`,
  ; and stay consistent with pointer-sort equality (`ule == (ult || eq)`).
  %c1 = call i1 @__CPROVER_nondet_bool()
  %c2 = call i1 @__CPROVER_nondet_bool()
  %lhs = select i1 %c1, ptr @g_a, ptr %local
  %rhs = select i1 %c2, ptr @g_b, ptr %p1
  %lhs_i = ptrtoint ptr %lhs to i64
  %rhs_i = ptrtoint ptr %rhs to i64

  %p_eq = icmp eq ptr %lhs, %rhs
  %p_ne = icmp ne ptr %lhs, %rhs
  %p_ult = icmp ult ptr %lhs, %rhs
  %p_ule = icmp ule ptr %lhs, %rhs
  %i_ult = icmp ult i64 %lhs_i, %rhs_i
  %eq_ult = icmp eq i1 %p_ult, %i_ult
  %ult_or_eq = or i1 %p_ult, %p_eq
  %eq_ule_order = icmp eq i1 %p_ule, %ult_or_eq
  %ne_flip = xor i1 %p_eq, true
  %eq_ne = icmp eq i1 %p_ne, %ne_flip

  %p_slt = icmp slt ptr %lhs, %rhs
  %i_slt = icmp slt i64 %lhs_i, %rhs_i
  %eq_slt = icmp eq i1 %p_slt, %i_slt

  %p_sle = icmp sle ptr %lhs, %rhs
  %i_sle = icmp sle i64 %lhs_i, %rhs_i
  %eq_sle = icmp eq i1 %p_sle, %i_sle

  %p_sgt = icmp sgt ptr %lhs, %rhs
  %i_sgt = icmp sgt i64 %lhs_i, %rhs_i
  %eq_sgt = icmp eq i1 %p_sgt, %i_sgt

  %p_sge = icmp sge ptr %lhs, %rhs
  %i_sge = icmp sge i64 %lhs_i, %rhs_i
  %eq_sge = icmp eq i1 %p_sge, %i_sge

  %eq_1 = and i1 %eq_ult, %eq_ule_order
  %eq_2 = and i1 %eq_1, %eq_ne
  %eq_3 = and i1 %eq_2, %eq_slt
  %eq_4 = and i1 %eq_3, %eq_sle
  %eq_5 = and i1 %eq_4, %eq_sgt
  %eq_all = and i1 %eq_5, %eq_sge
  call void @__CPROVER_assert(i1 %eq_all, ptr null)

  ; Vector pointer relational `icmp` (`<2 x ptr>`).
  %vl0 = insertelement <2 x ptr> poison, ptr @arr, i32 0
  %vl = insertelement <2 x ptr> %vl0, ptr %lhs, i32 1
  %vr0 = insertelement <2 x ptr> poison, ptr %p1, i32 0
  %vr = insertelement <2 x ptr> %vr0, ptr %rhs, i32 1
  %vcmp = icmp slt <2 x ptr> %vl, %vr
  %vcmp0 = extractelement <2 x i1> %vcmp, i32 0
  %vcmp1 = extractelement <2 x i1> %vcmp, i32 1
  %veq1 = icmp eq i1 %vcmp1, %p_slt
  %vok = and i1 %vcmp0, %veq1
  call void @__CPROVER_assert(i1 %vok, ptr null)

  ; Empty-slice iterator `begin < end` on dangling or null pointers: defined in
  ; LLVM, so no pointer-relation validity check may fire.
  %dangling = select i1 %c1, ptr inttoptr (i64 4 to ptr), ptr null
  %more = icmp ult ptr %dangling, %dangling
  %done = xor i1 %more, true
  call void @__CPROVER_assert(i1 %done, ptr null)

  ; Integer addresses below 2^(63 - object_bits) (see pointer_icmp_forged_tag.ll
  ; for higher ones) and wild same-object offsets order like integers.
  %x = call i64 @__CPROVER_nondet_u64()
  %y = call i64 @__CPROVER_nondet_u64()
  %xm = and i64 %x, 140737488355327
  %ym = and i64 %y, 140737488355327
  %px = inttoptr i64 %xm to ptr
  %py = inttoptr i64 %ym to ptr
  %p_xy = icmp ult ptr %px, %py
  %i_xy = icmp ult i64 %xm, %ym
  %eq_xy = icmp eq i1 %p_xy, %i_xy
  call void @__CPROVER_assert(i1 %eq_xy, ptr null)
  %wild = getelementptr i8, ptr @arr, i64 %x
  %p_wild = icmp ult ptr @arr, %wild
  %arr_i = ptrtoint ptr @arr to i64
  %wild_i = ptrtoint ptr %wild to i64
  %i_wild = icmp ult i64 %arr_i, %wild_i
  %eq_wild = icmp eq i1 %p_wild, %i_wild
  call void @__CPROVER_assert(i1 %eq_wild, ptr null)

  ret i32 0
}
