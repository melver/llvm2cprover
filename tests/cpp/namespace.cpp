// ---
// description = "simple namespace-qualified functions lower through C++ mangling"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern "C" void __CPROVER_assert(bool, const char*);

namespace math {
int add_one(int x) {
  return x + 1;
}
}  // namespace math

int main() {
  __CPROVER_assert(math::add_one(4) == 5, "namespace call works");
  return 0;
}
