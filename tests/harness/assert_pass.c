// ---
// description = "A passing harness assertion is reported as successful"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  __CPROVER_assert(1, "pass");
  return 0;
}
