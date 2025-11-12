use std::{
    ffi::{c_void, CStr},
    mem,
};

pub struct Marshaller {
    obuffer: Vec<u8>,
    pointers: Vec<usize>,
}

pub enum FieldType {
    U16,
    U32,
    U64,
    I16,
    I32,
    I64,
    F32,
    F64,
    String,
    Char, // one byte
}
impl Marshaller {
    pub fn new() -> Self {
        Marshaller {
            obuffer: Vec::new(),
            pointers: Vec::new(),
        }
    }

    pub fn push<T>(&mut self, value: T) {
        let size = mem::size_of::<T>();
        let ptr = &value as *const T as *const u8;
        let slice = unsafe { std::slice::from_raw_parts(ptr, size) };
        self.pointers.push(self.obuffer.len());
        self.obuffer.extend_from_slice(slice);
    }

    // pub fn push_cstr(&mut self, value: &CStr) {
    //     let bytes = value.to_bytes_with_nul();
    //     //  let bytes = cstring.to_bytes_with_nul();
    //    // let size = bytes.len();
    //    // let ptr = bytes.as_ptr();
    //     let slice = unsafe { std::slice::from_raw_parts(ptr, size) };
    //     self.pointers.push(self.obuffer.len());
    //     self.obuffer.extend_from_slice(slice);
    // }

    // fn pop<T>(&mut self) -> T {
    //     let size = mem::size_of::<T>();
    //     let slice = self.obuffer.split_off(self.obuffer.len() - size);
    //     let ptr = slice.as_ptr() as *const T;
    //     unsafe { *ptr }
    // }

    pub fn build_buffer(&mut self) -> (Vec<u8>, Vec<*mut c_void>) {
        //self.obuffer.clone()
        let mut pvec: Vec<*mut c_void> = Vec::new();
        for p in &self.pointers {
            let ptr = unsafe { self.obuffer.as_mut_ptr().offset(*p as isize) };
            pvec.push(ptr as *mut c_void);
        }
        (self.obuffer.clone(), pvec)
    }
}
