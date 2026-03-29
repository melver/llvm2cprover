// ---
// description = "Array indexing lowers through GEP and load/store"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int data[3] = {1, 2, 3};
  data[1] = data[0] + data[2];
  __CPROVER_assert(data[1] == 4, "array gep");
  return 0;
}
