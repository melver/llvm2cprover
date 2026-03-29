// ---
// description = "Nested struct and array indexing lowers through chained GEPs"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct Inner {
  int values[2];
};

struct Outer {
  struct Inner inner;
};

int main(void) {
  struct Outer outer = {{{5, 7}}};
  outer.inner.values[0] = outer.inner.values[1] - 2;
  __CPROVER_assert(outer.inner.values[0] == 5, "nested gep");
  return 0;
}
