// ---
// description = "CBMC pointer builtins lower to their expressions instead of bodiless calls"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---
//
// CBMC's C typechecker turns these into expressions; through LLVM they arrive as calls and
// must be lowered by the translator, or CBMC sees bodiless functions returning nondet.

#include <stdlib.h>

extern void __CPROVER_assert(_Bool, const char *);
extern _Bool __CPROVER_same_object(const void *, const void *);
extern _Bool __CPROVER_r_ok(const void *, unsigned long);
extern unsigned long __CPROVER_OBJECT_SIZE(const void *);
extern unsigned long __CPROVER_POINTER_OBJECT(const void *);
extern long __CPROVER_POINTER_OFFSET(const void *);
extern _Bool __CPROVER_DYNAMIC_OBJECT(const void *);

int a[4], b[4];

int main(void) {
  __CPROVER_assert(__CPROVER_same_object(&a[0], &a[3]), "same object");
  __CPROVER_assert(!__CPROVER_same_object(&a[0], &b[0]), "different objects");
  __CPROVER_assert(__CPROVER_POINTER_OBJECT(a) != __CPROVER_POINTER_OBJECT(b), "object ids");
  __CPROVER_assert(__CPROVER_OBJECT_SIZE(a) == sizeof(a), "object size");
  __CPROVER_assert(__CPROVER_POINTER_OFFSET(&a[2]) == 2 * sizeof(int), "pointer offset");
  __CPROVER_assert(__CPROVER_r_ok(a, sizeof(a)), "readable");
  __CPROVER_assert(!__CPROVER_r_ok(&a[1], sizeof(a)), "not readable past the end");
  int *p = malloc(sizeof(int));
  __CPROVER_assert(!p || __CPROVER_DYNAMIC_OBJECT(p), "heap object");
  __CPROVER_assert(!__CPROVER_DYNAMIC_OBJECT(a), "static object");
  free(p);
  return 0;
}
