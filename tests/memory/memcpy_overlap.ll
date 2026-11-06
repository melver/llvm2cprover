; ---
; description = "llvm.memcpy with partially overlapping source and destination fails the overlap check"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; LLVM LangRef: llvm.memcpy's source and destination "must either be equal or
; non-overlapping". Only the equal case is allowed; this copy overlaps by 4 bytes.

declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)

define i32 @main() {
entry:
  %a = alloca [4 x i32]
  %a1 = getelementptr i32, ptr %a, i64 1
  call void @llvm.memcpy.p0.p0.i64(ptr %a1, ptr %a, i64 8, i1 false)
  ret i32 0
}
