; ---
; description = "a struct whose field is a non-byte-aligned integer (i24) must keep its LLVM alloc size (4), not its store size (3)"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; todo = "struct padding cursor advances by the field's ABI size while the GOTO field only spans its bit width; sizeof({i24}) becomes 3"
; ---
;
; Clang models a C union containing a 22-bit bitfield as `{ i24 }`. Its
; LLVM alloc size is 4 (i24 aligns like i32), so clang copies it with
; `memcpy(…, 4)` and reads the bitfield with `load i32`. The translator
; lowers the field as a 24-bit bitvector and emits no trailing padding, so
; CBMC sees a 3-byte object and reports "pointer outside object bounds" on
; the i32 load, and "memcpy destination region writeable" on the copy.
; Found by Csmith (seeds 80, 469).

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

%union.U = type { i24 }

@g = internal global %union.U { i24 5 }, align 4

declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly captures(none), ptr noalias readonly captures(none), i64, i1 immarg)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %u = alloca %union.U, align 4
  call void @llvm.memcpy.p0.p0.i64(ptr align 4 %u, ptr align 4 @g, i64 4, i1 false)
  %v = load i32, ptr %u, align 4
  %bits = and i32 %v, 4194303
  %ok = icmp eq i32 %bits, 5
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
