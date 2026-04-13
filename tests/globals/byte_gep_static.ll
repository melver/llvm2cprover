; ---
; description = "Byte GEPs into a one-element struct-array global resolve to in-bounds byte offsets"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Clang with opaque pointers lowers 'static T obj[1]' as a global of type [1 x T]
; and accesses struct fields via constant expressions of the form
;   getelementptr i8, ptr @obj, i64 <byte_offset>
; The translator must treat i8-element-type GEPs as byte arithmetic rather than
; struct-element indexing; otherwise the index is interpreted as an element count
; and CBMC emits an OOB check that always fails for a 1-element array.

%struct.two = type { i64, i64 }

@s = internal global [1 x %struct.two] zeroinitializer, align 8

@.msg = private unnamed_addr constant [14 x i8] c"byte gep read\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  store i64 42, ptr getelementptr inbounds (i8, ptr @s, i64 8), align 8
  %val = load i64, ptr getelementptr inbounds (i8, ptr @s, i64 8), align 8
  %ok = icmp eq i64 %val, 42
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
