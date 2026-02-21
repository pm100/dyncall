use std::{ffi::c_void, ptr};

use libc::strlen;
use libffi::raw::{
    ffi_abi_FFI_DEFAULT_ABI, ffi_call, FFI_TYPE_DOUBLE, FFI_TYPE_FLOAT, FFI_TYPE_POINTER,
    FFI_TYPE_SINT16, FFI_TYPE_SINT32, FFI_TYPE_SINT64, FFI_TYPE_SINT8, FFI_TYPE_UINT16,
    FFI_TYPE_UINT32, FFI_TYPE_UINT64, FFI_TYPE_UINT8,
};

use crate::{
    args::{ArgType, LengthDef, ToArg, ToMutArg},
    ArgVal, FuncDef,
};

pub struct Invocation<'a> {
    pub(crate) func_def: &'a FuncDef,
    pub(crate) arg_ptrs: Vec<*mut c_void>,
    pub(crate) arg_vals: Vec<ArgVal>,
}
impl<'a> Invocation<'a> {
    pub fn push_arg<T>(&mut self, value: &T)
    where
        T: ToArg + ?Sized,
    {
        let val_count = self.arg_vals.len();
        // to_arg must push 2 values onto the arg_vals stack
        let argp = value.to_arg(self);
        assert!(self.arg_vals.len() - val_count == 2);

        self.arg_ptrs.push(argp);
    }
    pub fn push_mut_arg<T>(&mut self, value: &mut T)
    where
        T: ToMutArg + ?Sized,
    {
        let val_count = self.arg_vals.len();
        // to_mut_arg must push 2 values onto the arg_vals stack
        let argp = value.to_mut_arg(self);
        assert!(self.arg_vals.len() - val_count == 2);
        self.arg_ptrs.push(argp);
    }

    pub fn get_arg_type(&self, index: usize) -> &ArgType {
        &self.func_def.arg_types[index]
    }

    pub fn get_arg_count(&self) -> usize {
        self.func_def.arg_types.len()
    }
    pub fn call_and_return(&mut self, return_ptr: *mut c_void) {
        let mut cif = self.func_def.cif;
        self.pre_process_args();
        // let result = match self.func_def.ffi_return_type.type_ as u32 {
        //     FFI_TYPE_POINTER => ArgVal::Pointer(ptr::null_mut()),
        //     FFI_TYPE_UINT64 => ArgVal::U64(0),
        //     FFI_TYPE_SINT64 => ArgVal::I64(0),
        //     FFI_TYPE_UINT32 => ArgVal::U32(0),
        //     FFI_TYPE_SINT32 => ArgVal::I32(0),
        //     FFI_TYPE_SINT16 => ArgVal::I16(0),
        //     FFI_TYPE_UINT16 => ArgVal::U16(0),
        //     FFI_TYPE_UINT8 => ArgVal::Char(0),
        //     FFI_TYPE_SINT8 => ArgVal::Char(0),
        //     FFI_TYPE_FLOAT => ArgVal::F32(0.0),
        //     FFI_TYPE_DOUBLE => ArgVal::F64(0.0),
        //     _ => panic!("Unsupported return type"),
        // };

        // let addr = result.payload_ptr();
        println!("call2 self={:?}", self.arg_ptrs);
        unsafe {
            ffi_call(
                &mut cif,
                Some(self.func_def.entry_point),
                return_ptr, //as *mut c_void,
                self.arg_ptrs.as_mut_ptr(),
            );
        }

        self.post_process_args();
        self.arg_ptrs.clear();
        self.arg_vals.clear();
    }
    pub fn call(&mut self) -> ArgVal {
        let mut cif = self.func_def.cif;
        self.pre_process_args();
        let result = match self.func_def.ffi_return_type.type_ as u32 {
            FFI_TYPE_POINTER => ArgVal::Pointer(ptr::null_mut()),
            FFI_TYPE_UINT64 => ArgVal::U64(0),
            FFI_TYPE_SINT64 => ArgVal::I64(0),
            FFI_TYPE_UINT32 => ArgVal::U32(0),
            FFI_TYPE_SINT32 => ArgVal::I32(0),
            FFI_TYPE_SINT16 => ArgVal::I16(0),
            FFI_TYPE_UINT16 => ArgVal::U16(0),
            FFI_TYPE_UINT8 => ArgVal::Char(0),
            FFI_TYPE_SINT8 => ArgVal::Char(0),
            FFI_TYPE_FLOAT => ArgVal::F32(0.0),
            FFI_TYPE_DOUBLE => ArgVal::F64(0.0),
            _ => panic!("Unsupported return type"),
        };

        let addr = result.payload_ptr();
        println!("call2 self={:?}", self.arg_ptrs);
        unsafe {
            ffi_call(
                &mut cif,
                Some(self.func_def.entry_point),
                addr, //as *mut c_void,
                self.arg_ptrs.as_mut_ptr(),
            );
        }

        self.post_process_args();
        self.arg_ptrs.clear();
        self.arg_vals.clear();
        result
    }

    fn pre_process_ocstring(&mut self, arg_idx: usize, ldef: &LengthDef) {
        let len = match ldef {
            LengthDef::Arg(argnum) => {
                // TODO get length from other arg
                let len = match self.arg_vals.get((*argnum * 2 + 1) as usize) {
                    Some(ArgVal::U32(v)) => *v as usize,
                    Some(ArgVal::I32(v)) => *v as usize,
                    Some(ArgVal::U64(v)) => *v as usize,
                    Some(ArgVal::I64(v)) => *v as usize,
                    _ => panic!("Invalid length arg type for OCString"),
                };
                len
            }
            LengthDef::Fixed(len) => *len,
            LengthDef::None => return,
        };
        if let ArgVal::RustString(str) = &mut self.arg_vals[arg_idx * 2 + 1] {
            let s = unsafe { &mut **str };
            s.reserve(len + 1); // +1 for null terminator
            println!("pre_process_ocstring reserved len={}", len);
            self.arg_vals[arg_idx * 2] = ArgVal::Pointer(s.as_mut_ptr() as *mut c_void);
        };
    }

    fn pre_process_obytebuffer(&mut self, arg_idx: usize, ldef: &LengthDef) {
        let len = match ldef {
            LengthDef::Arg(argnum) => {
                // TODO get length from other arg
                let len = match self.arg_vals.get((*argnum * 2 + 1) as usize) {
                    Some(ArgVal::U32(v)) => *v as usize,
                    Some(ArgVal::I32(v)) => *v as usize,
                    Some(ArgVal::U64(v)) => *v as usize,
                    Some(ArgVal::I64(v)) => *v as usize,
                    _ => panic!("Invalid length arg type for OByteBuffer"),
                };
                len
            }
            LengthDef::Fixed(len) => *len,
            LengthDef::None => return,
        };
        if let ArgVal::RustString(str) = &mut self.arg_vals[arg_idx * 2 + 1] {
            let s = unsafe { &mut **str };
            s.reserve(len);
            println!("pre_process_obytebuffer reserved len={}", len);
            self.arg_vals[arg_idx * 2] = ArgVal::Pointer(s.as_mut_ptr() as *mut c_void);
        };
    }
    fn pre_process_args(&mut self) {
        // for mutable args, prepare buffers etc.

        for (i, arg_type) in self.func_def.arg_types.iter().enumerate() {
            match arg_type {
                ArgType::OCString(ldef) => {
                    self.pre_process_ocstring(i, ldef);
                }
                ArgType::OByteBuffer(ldef) => {
                    self.pre_process_obytebuffer(i, ldef);
                }
                _ => {}
            }
        }
    }
    fn post_process_args(&mut self) {
        for (i, arg_type) in self.func_def.arg_types.iter().enumerate() {
            match arg_type {
                ArgType::OCString(ldef) => {
                    if let ArgVal::Pointer(p) = self.arg_vals[i * 2] {
                        //  let &mut str = *self.arg_vals[val_idx].as_rust_string_mut().unwrap();
                        let arg_str = self.arg_vals.get_mut(i * 2 + 1).unwrap();
                        let foo = arg_str.as_rust_string_mut().unwrap();
                        if let ArgVal::RustString(str) = arg_str {
                            unsafe {
                                let len = strlen(p as *const i8);
                                let str = &mut **str;
                                println!("fgets post_process_args len={}", len);
                                str.as_mut_vec().set_len(len);
                                // let rep_str = String::from_raw_parts(*p as *mut u8, len, cap);
                            }
                        }
                    }
                }
                ArgType::CString => {}
                ArgType::OByteBuffer(_ldef) => {
                    if let ArgVal::Pointer(p) = self.arg_vals[i * 2] {
                        // let arg_str = self.arg_vals.get_mut(i * 2 + 1).unwrap();
                        // let foo = arg_str.as_rust_string_mut().unwrap();
                        // if let ArgVal::RustString(str) = arg_str {
                        //     unsafe {
                        //         let cap = str.capacity();
                        //         let str = &mut **str;
                        //         str.as_mut_vec().set_len(cap);
                        //     }
                        // }
                    }
                }
                // Add more mutable types as needed
                _ => {}
            }
        }
    }
}
