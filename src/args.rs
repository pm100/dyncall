use std::{
    ffi::{c_void, CStr, CString},
    mem,
};

use enum_as_inner::EnumAsInner;
use libffi::{low::types, raw::ffi_type};

use crate::{invoke::Invocation, FuncDef};

#[derive(EnumAsInner, Debug, Clone)]
pub enum ArgVal {
    Pointer(*mut c_void),
    U64(u64),
    F64(f64),
    I64(i64),
    I32(i32),
    U32(u32),
    I16(i16),
    U16(u16),
    F32(f32),
    Char(u8),
    CString(CString),
    RustString(*mut String),
    None,
}
#[derive(Clone, Debug)]
pub enum LengthDef {
    None,
    Arg(u8),
    Fixed(usize),
}
#[derive(Clone, Debug)]
pub enum ArgType {
    // scalar types
    U64,
    F64,
    I64,
    I32,
    U32,
    I16,
    U16,
    F32,
    Char,

    // complex types
    CString,                // classic c string
    OCString(LengthDef),    // classic output c string (fgets, fscanf("%s"),...)
    ByteBuffer,             // buffer for writing raw bytes
    OByteBuffer(LengthDef), // Buffer for reading raw bytes
    OpaquePointer,          // void * , FILE*, HANDLE ....
    Pointer(Box<ArgType>),  // &i32, &f64 ....
    Void,

    // specials
    Stdin,
    Stdout,
    Stderr,
}
impl ArgVal {
    pub(crate) fn payload_ptr(&self) -> *mut c_void {
        // extracts a pointer to the data instide an ArgVal
        use ArgVal::*;
        match self {
            Pointer(ref val) => val as *const _ as *mut c_void,
            U64(ref val) => val as *const _ as *mut c_void,
            I32(ref val) => val as *const _ as *mut c_void,
            U32(ref val) => val as *const _ as *mut c_void,
            I16(ref val) => val as *const _ as *mut c_void,
            U16(ref val) => val as *const _ as *mut c_void,
            F32(ref val) => val as *const _ as *mut c_void,
            F64(ref val) => val as *const _ as *mut c_void,
            I64(ref val) => val as *const _ as *mut c_void,
            Char(ref val) => val as *const _ as *mut c_void,
            CString(ref val) => val.as_ptr() as *mut c_void,
            RustString(val) => {
                let v = *val;
                let s = unsafe { &*v };
                s.as_ptr() as *mut c_void
            }
            _ => panic!("Unsupported ArgVal variant for payload_ptr"),
            // ...
        }
    }
}
pub trait ToArg {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void;
}
pub trait ToMutArg {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void;
}
impl ToArg for CString {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        let ptr = self.as_ptr();
        let p = unsafe { std::mem::transmute::<*const i8, *mut c_void>(ptr) };
        func.arg_vals.push(ArgVal::Pointer(p));
        let penum = &func.arg_vals[func.arg_vals.len() - 1];

        penum.payload_ptr()
    }
}

impl ToMutArg for String {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::OCString(ldef) => {
                match ldef {
                    LengthDef::Fixed(len) => {
                        if self.len() + 1 < *len {
                            self.reserve(*len - self.len() - 1);
                        }
                    }
                    LengthDef::Arg(_arg_idx) => {}
                    LengthDef::None => {}
                }
                let mut buffer = self.as_mut_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::RustString(self));
                let pp = &func.arg_vals[func.arg_vals.len() - 2];
                let ppp = pp.payload_ptr();

                ppp
            }
            _ => unreachable!("Expected pointer type for mutable string argument"),
        }
    }
}
impl ToArg for String {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::CString => {
                let cstr = CString::new(self.as_str()).unwrap();
                func.arg_vals.push(ArgVal::CString(cstr.clone()));
                let pp = &func.arg_vals[func.arg_vals.len() - 1];
                let ppp = pp.as_c_string().unwrap().as_ptr() as *mut c_void;

                func.arg_vals.push(ArgVal::Pointer(ppp));
                // func.arg_vals.push(ArgVal::Pointer(p));
                let penum = &func.arg_vals[func.arg_vals.len() - 1];

                penum.payload_ptr()
            }
            ArgType::OCString(_) => {
                let mut buffer = vec![0u8; self.len() + 1];
                func.arg_vals
                    .push(ArgVal::Pointer(buffer.as_mut_ptr() as *mut c_void));
                let pp = &func.arg_vals[func.arg_vals.len() - 1];
                let ppp = pp.payload_ptr();

                // Store the buffer to keep it alive
                func.arg_vals
                    .push(ArgVal::Pointer(buffer.as_mut_ptr() as *mut c_void));

                ppp
            }
            _ => unreachable!("Expected pointer type for string argument"),
        }
    }
}
impl ToArg for CStr {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        let ptr = self.as_ptr();
        println!("CStr to_arg: {:x}", ptr as u64);
        let p = unsafe { mem::transmute::<*const i8, *mut c_void>(ptr) };

        func.arg_vals.push(ArgVal::Pointer(p));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        println!("CStr to_arg ArgVal ptr: {:p}", pp);
        let ppp = if let ArgVal::Pointer(ref p) = pp {
            p
        } else {
            panic!("Expected Pointer ArgVal")
        };
        println!("CStr to_arg final ptr: {:p}", ppp);
        unsafe { mem::transmute::<&*mut c_void, *mut c_void>(ppp) }
    }
}

impl ToArg for u64 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::U64(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for i64 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::I64(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for u32 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::U32(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for i32 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::I32(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToMutArg for ArgVal {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for ArgVal {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
