// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026, Marco Elver <me@marcoelver.com>
// ---
// description = "Floating-point casts between float, double, and integers lower correctly"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  double wide = 3.0;
  int i = (int)wide;
  float narrow = (float)i;
  unsigned u = (unsigned)narrow;
  __CPROVER_assert(i == 3, "double to int");
  __CPROVER_assert(narrow == 3.0f, "int to float");
  __CPROVER_assert(u == 3u, "float to unsigned");
  return 0;
}
