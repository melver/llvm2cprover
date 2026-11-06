; ---
; description = "A heap object behind a null check reads back what memcpy stored, so dead branches on it are pruned"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--no-malloc-may-fail"]
; ---
;
; The allocation may fail and @init's memcpy operands are unknown. Symex prunes
; %dead only if the null check's GOTO guard is the comparison itself (so NULL
; leaves %d's value set) and memcpy adds no `dst != src` branch. Pruning does not
; change the verdict; unit tests pin the branch-free lowering.

declare zeroext i1 @nondet_bool()
declare ptr @calloc(i64, i64)
declare void @__CPROVER_assert(i1, ptr)
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)

define ptr @safe_calloc() noinline {
entry:
  %ok = call zeroext i1 @nondet_bool()
  br i1 %ok, label %alloc, label %done
alloc:
  %p = call ptr @calloc(i64 1, i64 8)
  br label %done
done:
  %r = phi ptr [ %p, %alloc ], [ null, %entry ]
  ret ptr %r
}

define void @init(ptr %dst, ptr %src) noinline {
  call void @llvm.memcpy.p0.p0.i64(ptr %dst, ptr %src, i64 8, i1 false)
  ret void
}

define i32 @main() {
entry:
  %v = alloca i64
  store i64 0, ptr %v
  %d = call ptr @safe_calloc()
  %isnull = icmp eq ptr %d, null
  br i1 %isnull, label %exit, label %use
use:
  call void @init(ptr %d, ptr %v)
  %x = load i64, ptr %d
  %live = icmp ne i64 %x, 0
  br i1 %live, label %dead, label %exit
dead:
  call void @__CPROVER_assert(i1 false, ptr null)
  br label %exit
exit:
  ret i32 0
}
