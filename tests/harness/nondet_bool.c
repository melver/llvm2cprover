// ---
// description = "Nondeterministic C _Bool (LLVM i1) produces 0 or 1 under zext"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);
extern _Bool __CPROVER_nondet_bool(void);

int main(void) {
  int arr[2] = {10, 20};
  int x = __CPROVER_nondet_bool();
  int idx = __CPROVER_nondet_bool() ? 1 : 0;
  __CPROVER_assert(x == 0 || x == 1, "nondet _Bool zero-extends to 0 or 1");
  __CPROVER_assert(arr[x] == 10 || arr[x] == 20, "nondet _Bool index stays in bounds");
  __CPROVER_assert(arr[idx] == 10 || arr[idx] == 20, "nondet _Bool ternary index stays in bounds");
  return 0;
}
