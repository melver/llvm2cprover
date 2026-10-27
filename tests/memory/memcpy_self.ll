; ---
; description = "llvm.memcpy with identical source and destination is a no-op, not an overlap failure"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; LLVM LangRef: llvm.memcpy's source and destination "must either be equal or
; non-overlapping". Clang emits exactly this for struct self-assignment
; (`*p = *p;`), which Csmith generates routinely. Lowering to C `memcpy`
; inherits its stricter no-overlap precondition and CBMC reports
; "memcpy src/dst overlap".

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

%struct.S = type { i32, i64 }

@g = global [2 x %struct.S] [%struct.S { i32 1, i64 2 }, %struct.S { i32 3, i64 4 }], align 8

declare void @llvm.memcpy.p0.p0.i64(ptr noalias nocapture writeonly, ptr noalias nocapture readonly, i64, i1 immarg)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %p = getelementptr inbounds [2 x %struct.S], ptr @g, i64 0, i64 1
  call void @llvm.memcpy.p0.p0.i64(ptr align 8 %p, ptr align 8 %p, i64 16, i1 false)
  %f = getelementptr inbounds %struct.S, ptr %p, i32 0, i32 1
  %v = load i64, ptr %f, align 8
  %ok = icmp eq i64 %v, 4
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
