// ---
// description = "Nested structs lower as nested aggregate types"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct inner {
  int x;
};

struct outer {
  struct inner in;
};

int main(void) {
  struct outer value = {{9}};
  __CPROVER_assert(value.in.x == 9, "nested struct");
  return 0;
}
