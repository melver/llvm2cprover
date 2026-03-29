// ---
// description = "a small Rust harness can call __CPROVER_assert directly"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "harness"
// ---

extern "C" {
    fn __CPROVER_assert(cond: bool, descr: *const i8);
}

#[no_mangle]
pub unsafe extern "C" fn harness() -> i32 {
    __CPROVER_assert(true, b"rust assert passes\0".as_ptr().cast());
    0
}
