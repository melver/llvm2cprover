// ---
// description = "Rust panic ABI calls lower to assertions that pass when in bounds and non-zero"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "harness"
// ---

extern "C" {
    fn __CPROVER_assert(cond: bool, descr: *const i8);
}

fn slice_div(values: &[i32], start: usize, end: usize, idx: usize, divisor: i32) -> i32 {
    let sub = &values[start..end];
    sub[idx] / divisor
}

#[no_mangle]
pub unsafe extern "C" fn harness() -> i32 {
    let arr = [10, 20, 30, 40];
    let q = slice_div(&arr, 1, 3, 1, 3);
    assert!(q == 10);
    __CPROVER_assert(q == 10, b"rust panic paths stay unreachable\0".as_ptr().cast());
    0
}
