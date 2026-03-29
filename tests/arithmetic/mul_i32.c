// ---
// description = "Integer multiplication lowers and verifies"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  __CPROVER_assert(6 * 7 == 42, "mul");
  return 0;
}
