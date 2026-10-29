; ---
; description = "unreachable ends the path: an infeasible edge into it must not merge state into later blocks"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--unwind", "3", "--unwinding-assertions"]
; ---

declare void @__CPROVER_assert(i1, ptr)
declare void @__CPROVER_assume(i1)
declare i32 @__CPROVER_nondet_int()
declare i8 @__CPROVER_nondet_uchar()
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)

@.msg0 = private unnamed_addr constant [5 x i8] c"d[0]\00"
@.msg1 = private unnamed_addr constant [5 x i8] c"d[1]\00"

define void @copy(ptr %src0, ptr %dst0, i32 %width, i32 %height) {
entry:
  %empty = icmp slt i32 %height, 1
  br i1 %empty, label %exit, label %pre

pre:
  %ok = icmp sgt i32 %width, -1
  %n = zext i32 %width to i64
  br i1 %ok, label %loop, label %panic

loop:
  %src = phi ptr [ %src0, %pre ], [ %src.next, %loop ]
  %dst = phi ptr [ %dst0, %pre ], [ %dst.next, %loop ]
  %h = phi i32 [ %height, %pre ], [ %h.next, %loop ]
  %h.next = add nsw i32 %h, -1
  call void @llvm.memcpy.p0.p0.i64(ptr %dst, ptr %src, i64 %n, i1 false)
  %src.next = getelementptr inbounds i8, ptr %src, i64 2
  %dst.next = getelementptr inbounds i8, ptr %dst, i64 2
  %last = icmp ult i32 %h, 2
  br i1 %last, label %exit, label %loop

panic:
  unreachable

exit:
  ret void
}

define i32 @main() {
entry:
  %s = alloca [4 x i8]
  %d = alloca [4 x i8]
  %s0 = call i8 @__CPROVER_nondet_uchar()
  %s1 = call i8 @__CPROVER_nondet_uchar()
  %s2 = call i8 @__CPROVER_nondet_uchar()
  %s3 = call i8 @__CPROVER_nondet_uchar()
  %s.p1 = getelementptr inbounds i8, ptr %s, i64 1
  %s.p2 = getelementptr inbounds i8, ptr %s, i64 2
  %s.p3 = getelementptr inbounds i8, ptr %s, i64 3
  store i8 %s0, ptr %s
  store i8 %s1, ptr %s.p1
  store i8 %s2, ptr %s.p2
  store i8 %s3, ptr %s.p3
  %d.p1 = getelementptr inbounds i8, ptr %d, i64 1
  %d.p2 = getelementptr inbounds i8, ptr %d, i64 2
  %d.p3 = getelementptr inbounds i8, ptr %d, i64 3
  store i8 0, ptr %d
  store i8 0, ptr %d.p1
  store i8 0, ptr %d.p2
  store i8 0, ptr %d.p3

  ; width in [1, 2], height in [1, 2], both strides 2: rows never overlap.
  %w = call i32 @__CPROVER_nondet_int()
  %h = call i32 @__CPROVER_nondet_int()
  %w.lo = icmp sge i32 %w, 1
  %w.hi = icmp sle i32 %w, 2
  %h.lo = icmp sge i32 %h, 1
  %h.hi = icmp sle i32 %h, 2
  call void @__CPROVER_assume(i1 %w.lo)
  call void @__CPROVER_assume(i1 %w.hi)
  call void @__CPROVER_assume(i1 %h.lo)
  call void @__CPROVER_assume(i1 %h.hi)

  call void @copy(ptr %s, ptr %d, i32 %w, i32 %h)

  ; d[0] = s[0] always; d[1] = s[1] iff width == 2, else untouched (0).
  %r0 = load i8, ptr %d
  %ok0 = icmp eq i8 %r0, %s0
  call void @__CPROVER_assert(i1 %ok0, ptr @.msg0)
  %r1 = load i8, ptr %d.p1
  %wide = icmp eq i32 %w, 2
  %want1 = select i1 %wide, i8 %s1, i8 0
  %ok1 = icmp eq i8 %r1, %want1
  call void @__CPROVER_assert(i1 %ok1, ptr @.msg1)
  ret i32 0
}
