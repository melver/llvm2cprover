// ---
// description = "CBMC entry selection can target one harness among several"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "harness_one"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int harness_one(void) {
  __CPROVER_assert(1, "first harness");
  return 0;
}

int harness_two(void) {
  __CPROVER_assert(0, "second harness");
  return 0;
}
