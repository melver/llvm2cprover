// ---
// description = "CBMC runtime functions with library semantics stay ordinary calls"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---
//
// __CPROVER_havoc_object is implemented by CBMC itself, not by the translator. If it were
// dropped or treated as a bodiless no-op, `g` would stay 0 and verification would succeed.

extern void __CPROVER_assert(_Bool, const char *);
extern void __CPROVER_havoc_object(void *);

int g;

int main(void) {
  __CPROVER_havoc_object(&g);
  __CPROVER_assert(g == 0, "havocked");
  return 0;
}
