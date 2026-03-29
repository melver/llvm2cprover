// ---
// description = "Lifetime markers are dropped as semantic no-ops"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int value = 3;
  __CPROVER_assert(value == 3, "lifetime");
  return 0;
}
