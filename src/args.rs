use std::{
    ffi::{c_void, CStr, CString},
    mem,
};

use enum_as_inner::EnumAsInner;
use crate::invoke::Invocation;

/// A runtime value held while preparing or executing a call.
///
/// Each argument is represented twice in the internal stack: once for
/// ownership/lifetime and once for the pointer that libffi reads from.
/// Users do not construct `ArgVal` directly in most cases; it is produced
/// by the [`ToArg`] and [`ToMutArg`] trait implementations.
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
    ByteBuffer(*mut Vec<u8>),
    None,
}

/// Describes how the length of an output buffer is determined.
#[derive(Clone, Debug)]
pub enum LengthDef {
    /// Length is not specified; the buffer must already be large enough.
    None,
    /// Length is taken from another argument at the given zero-based index.
    Arg(u8),
    /// Length is a fixed number of bytes.
    Fixed(usize),
}

/// Describes the type of a single argument or return value in a [`FuncDef`](crate::FuncDef).
///
/// These are produced automatically by [`DynCaller::define_function_by_str`](crate::DynCaller::define_function_by_str)
/// from the type tokens in the descriptor string (see crate-level docs).
#[derive(Clone, Debug)]
pub enum ArgType {
    // ── scalar input types ────────────────────────────────────────────────
    U64,
    F64,
    I64,
    I32,
    U32,
    I16,
    U16,
    F32,
    /// Single byte (`u8` / `i8`).
    Char,

    // ── complex types ─────────────────────────────────────────────────────
    /// Input null-terminated C string (`const char *`). Push with a `String`.
    CString,
    /// Output C string buffer written by the callee (e.g. `fgets`, `sscanf %s`).
    /// The optional `LengthDef` controls buffer pre-allocation.
    OCString(LengthDef),
    /// Input raw byte buffer.
    ByteBuffer,
    /// Output raw byte buffer written by the callee (e.g. `fread`).
    OByteBuffer(LengthDef),
    /// Opaque pointer (`void *`, `FILE *`, `HANDLE`, …).
    OpaquePointer,
    /// Typed output pointer (`&mut T`). Use `*T` in the descriptor string.
    Pointer(Box<ArgType>),
    /// No value (only valid as a return type).
    Void,

    // ── specials ──────────────────────────────────────────────────────────
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
            // CString(ref val) => {
            //     val.as_ptr() as *const _ as *mut c_void

            // }
            // RustString(val) => {
            //     let v = *val;
            //     let s = unsafe { &*v };
            //     s.as_ptr() as *mut c_void
            // }
            _ => panic!("Unsupported ArgVal variant for payload_ptr"),
            // ...
        }
    }
}
/// Trait for types that can be passed as input arguments to a dynamic call.
///
/// Implemented for: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
/// `f32`, `f64`, `String`, `CStr`, [`ArgVal`].
pub trait ToArg {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void;
}

/// Trait for types that can be passed as output (pointer) arguments to a dynamic call.
///
/// The callee writes its result through the pointer. After the call returns,
/// the value is available in the original variable.
///
/// Implemented for: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
/// `f32`, `f64`, `String`, [`ArgVal`].
pub trait ToMutArg {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void;
}
// impl ToArg for CString {
//     fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
//         let ptr = self.as_ptr();
//         let p = unsafe { std::mem::transmute::<*const i8, *mut c_void>(ptr) };
//         func.arg_vals.push(ArgVal::Pointer(p));
//         let penum = &func.arg_vals[func.arg_vals.len() - 1];

//         penum.payload_ptr()
//     }
// }

impl ToMutArg for String {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::OCString(_ldef) => {
                // size maybe adjusted just before call

                // store pointer to the string bytes
                let buffer = self.as_mut_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));

                // store pointer to the complete string for later retrieval
                func.arg_vals.push(ArgVal::RustString(self));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                pbuff.payload_ptr()
            }
            ArgType::OByteBuffer(_ldef) => {
                // size maybe adjusted just before call

                // store pointer to the buffer bytes
                let buffer = self.as_mut_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));

                // store pointer to the complete buffer for later retrieval

                func.arg_vals.push(ArgVal::RustString(self));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                pbuff.payload_ptr()
            }
            _ => unreachable!("Expected String type for mutable string argument"),
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
                let buffer = cstr.as_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::CString(cstr));

                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                // pbuff.payload_ptr()
                pbuff.payload_ptr()
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
        func.arg_vals.push(ArgVal::U64(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for i64 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::I64(*self));
        func.arg_vals.push(ArgVal::I64(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for u32 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::U32(*self));
        func.arg_vals.push(ArgVal::U32(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for i32 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::I32(*self));
        func.arg_vals.push(ArgVal::I32(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for i16 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::I16(*self));
        func.arg_vals.push(ArgVal::I16(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for u16 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::U16(*self));
        func.arg_vals.push(ArgVal::U16(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for f32 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::F32(*self));
        func.arg_vals.push(ArgVal::F32(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for f64 {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(ArgVal::F64(*self));
        func.arg_vals.push(ArgVal::F64(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToMutArg for ArgVal {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(self.clone());
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for ArgVal {
    fn to_arg(&self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals.push(self.clone());
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for i32 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i32 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i32 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToMutArg for u32 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u32 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u32 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for i64 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i64 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i64 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToMutArg for u64 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u64 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u64 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for f64 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut f64 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut f64 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for f32 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut f32 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut f32 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for i16 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i16 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i16 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for u16 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u16 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u16 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for i8 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i8 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut i8 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl ToMutArg for u8 {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> *mut c_void {
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u8 as *mut c_void));
        func.arg_vals
            .push(ArgVal::Pointer(self as *mut u8 as *mut c_void));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
