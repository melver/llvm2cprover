// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "--fp-groups also restricts indirect invoke: a group-b target in a group-a call fails closed"
// expected = "VERIFICATION FAILED"
// entry = "main"
// translator_flags = ["--entry", "main", "--fp-groups", "tests/functions/fp_groups.groups"]
// ---
//
// The destructor makes the indirect call an `invoke` (it may unwind past `g`), which is how
// Rust function-pointer and vtable calls appear under panic=unwind.

struct G {
  ~G();
};
G::~G() {}

extern "C" int a_one(int x) { return x + 1; }
extern "C" int b_one(int x) { return x * 100; }

extern "C" int a_apply(int (*f)(int), int x) {
  G g;
  return f(x);
}

extern "C" int main() {
  int (*volatile keep)(int) = a_one;
  (void)keep;
  return a_apply(b_one, 1);
}
