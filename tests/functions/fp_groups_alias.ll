; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
; ---
; description = "--fp-groups: direct calls through GlobalAlias (including cross-type MergeFunctions aliases) lower as direct calls"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; translator_flags = ["--entry", "main", "--fp-groups", "tests/functions/fp_groups.groups"]
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@b_alias = unnamed_addr alias i32 (i32), ptr @b_one
@get_user_data = unnamed_addr alias ptr (ptr), ptr @get_tag_len
@id_int = unnamed_addr alias i64 (i64), ptr @id_ptr

declare void @__CPROVER_assert(i1, ptr)

define ptr @id_ptr(ptr %p) {
entry:
  br i1 false, label %dead, label %exit
dead:
  %unused = load i8, ptr %p, align 1
  br label %exit
exit:
  ret ptr %p
}

define i32 @a_one(i32 %x) {
entry:
  %r = add i32 %x, 1
  ret i32 %r
}

define i32 @b_one(i32 %x) {
entry:
  %r = mul i32 %x, 10
  ret i32 %r
}

define i64 @get_tag_len(ptr %p) {
entry:
  %v = load i64, ptr %p, align 8
  ret i64 %v
}

define i32 @a_apply(ptr %f, i32 %x) {
entry:
  %ind = call i32 %f(i32 %x)
  %dir = call i32 @b_alias(i32 %ind)
  ret i32 %dir
}

define i32 @main() {
entry:
  %slot = alloca ptr, align 8
  %val = inttoptr i64 42 to ptr
  store ptr %val, ptr %slot, align 8
  %got = call ptr @get_user_data(ptr %slot)
  %ok_ptr = icmp eq ptr %got, %val
  call void @__CPROVER_assert(i1 %ok_ptr, ptr null)

  %res = call i32 @a_apply(ptr @a_one, i32 5)
  %ok_res = icmp eq i32 %res, 60
  call void @__CPROVER_assert(i1 %ok_res, ptr null)

  %seed = add i64 100, 23
  %roundtrip = call i64 @id_int(i64 %seed)
  %ok_int = icmp eq i64 %roundtrip, 123
  call void @__CPROVER_assert(i1 %ok_int, ptr null)
  ret i32 0
}
