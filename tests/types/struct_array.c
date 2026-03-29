// ---
// description = "Structs containing arrays lower as aggregate fields"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct holder {
  int values[2];
};

int main(void) {
  struct holder value = {
      {4, 7}
  };
  __CPROVER_assert(value.values[1] == 7, "struct array");
  return 0;
}
