#[repr(C)]
pub struct PairU32 {
    first: u32,
    second: u32,
}

#[no_mangle]
pub extern "C" fn sum_pair(value: PairU32) -> u32 {
    value.first + value.second
}

#[no_mangle]
pub extern "C" fn sum_pair_ptr(value: *const PairU32) -> u32 {
    unsafe { (*value).first + (*value).second }
}

/// Returns the low 32 bits of the pointer without dereferencing it.
#[no_mangle]
pub extern "C" fn ptr_addr_low(p: *const u8, _n: i32) -> u32 {
    p as usize as u32
}

#[no_mangle]
pub extern "C" fn neg_one_i8() -> i8 {
    -1
}

#[no_mangle]
pub extern "C" fn bump_pair(value: *mut PairU32) -> u32 {
    unsafe {
        (*value).first += 1;
        (*value).second += 2;
        (*value).first + (*value).second
    }
}
