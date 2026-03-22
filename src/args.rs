use std::{
    ffi::{c_void, CStr, CString},
    mem,
};

use anyhow::{bail, Result};
use crate::invoke::Invocation;
use crate::structs::{StructType, StructValue};
use enum_as_inner::EnumAsInner;

/// A runtime value held while preparing or executing a call.
///
/// Each argument is represented twice in the internal stack: once for
/// ownership/lifetime and once for the pointer that libffi reads from.
/// Users do not construct `ArgVal` directly in most cases; it is produced
/// by the [`ToArg`] and [`ToMutArg`] trait implementations.
#[derive(EnumAsInner, Debug, Clone)]
pub enum ArgVal {
    Pointer(*mut c_void),
    StructValue(StructValue),
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
/// These are produced automatically by [`DynCaller::define_function`](crate::DynCaller::define_function)
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
    /// Flat struct passed by value.
    Struct(StructType),
    /// Typed output pointer (`&mut T`). Use `*T` in the descriptor string.
    Pointer(Box<ArgType>),
    /// No value (only valid as a return type).
    Void,

}

impl ArgType {
    /// If this type is a struct (either `{...}` or `*{...}`), return the
    /// underlying [`StructType`] that describes the fields.
    ///
    /// Returns `None` for every other type.
    ///
    /// This is the ergonomic way for scripting-language adapters to inspect
    /// struct field types without manually matching both `Struct` and
    /// `Pointer(Struct(...))`:
    ///
    /// ```
    /// # use dyncall::{DynCaller, ArgType};
    /// # fn example(fdef: &dyncall::FuncDef, i: usize) {
    /// if let Some(st) = fdef.get_arg_type(i).struct_type() {
    ///     for j in 0..st.field_count() {
    ///         println!("  field {}: {:?}", j, st.field_type(j).unwrap());
    ///     }
    /// }
    /// # }
    /// ```
    pub fn struct_type(&self) -> Option<&StructType> {
        match self {
            ArgType::Struct(st) => Some(st),
            ArgType::Pointer(inner) => inner.struct_type(),
            _ => None,
        }
    }

    /// Returns `true` if this type is a struct passed by value (`{...}`).
    pub fn is_struct_by_value(&self) -> bool {
        matches!(self, ArgType::Struct(_))
    }

    /// Returns `true` if this type is a pointer to a struct (`*{...}`).
    pub fn is_struct_ptr(&self) -> bool {
        matches!(self, ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)))
    }
}
impl ArgVal {
    pub(crate) fn payload_ptr(&self) -> *mut c_void {
        use ArgVal::*;
        match self {
            Pointer(ref val) => val as *const _ as *mut c_void,
            StructValue(val) => val.as_ptr(),
            U64(ref val) => val as *const _ as *mut c_void,
            I32(ref val) => val as *const _ as *mut c_void,
            U32(ref val) => val as *const _ as *mut c_void,
            I16(ref val) => val as *const _ as *mut c_void,
            U16(ref val) => val as *const _ as *mut c_void,
            F32(ref val) => val as *const _ as *mut c_void,
            F64(ref val) => val as *const _ as *mut c_void,
            I64(ref val) => val as *const _ as *mut c_void,
            Char(ref val) => val as *const _ as *mut c_void,
            _ => panic!("Unsupported ArgVal variant for payload_ptr"),
        }
    }
}
/// Trait for types that can be passed as input arguments to a dynamic call.
///
/// Implemented for: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
/// `f32`, `f64`, `String`, `CStr`, [`ArgVal`].
pub trait ToArg {
    fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void>;
}

/// Trait for types that can be passed as output (pointer) arguments to a dynamic call.
///
/// The callee writes its result through the pointer. After the call returns,
/// the value is available in the original variable.
///
/// Implemented for: `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
/// `f32`, `f64`, `String`, [`ArgVal`].
pub trait ToMutArg {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void>;
}

impl ToMutArg for String {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void> {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::OCString(_ldef) => {
                let buffer = self.as_mut_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::RustString(self));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                Ok(pbuff.payload_ptr())
            }
            ArgType::OByteBuffer(_ldef) => {
                let buffer = self.as_mut_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::RustString(self));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                Ok(pbuff.payload_ptr())
            }
            _ => bail!("Expected OCString or OByteBuffer for mutable String argument"),
        }
    }
}
impl ToArg for String {
    fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = func.func_def.arg_types[arg_idx].clone();

        match &arg_type {
            ArgType::CString => {
                let cstr = CString::new(self.as_str()).unwrap();
                let buffer = cstr.as_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::CString(cstr));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                Ok(pbuff.payload_ptr())
            }
            _ if func.func_def.coerce => {
                let cstr = CString::new(self.as_str())
                    .map_err(|e| anyhow::anyhow!("Invalid string for coercion: {}", e))?;
                crate::coerce::push_coerced_str(func, cstr, &arg_type)
            }
            _ => bail!("Type mismatch: expected CString, got String for {:?}", arg_type),
        }
    }
}
impl ToMutArg for Vec<u8> {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void> {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::OByteBuffer(_ldef) => {
                let buffer = self.as_mut_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::ByteBuffer(self));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                Ok(pbuff.payload_ptr())
            }
            _ => bail!("Expected OByteBuffer for mutable byte buffer argument"),
        }
    }
}
impl ToArg for StructValue {
    fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::Struct(_) => {
                func.arg_vals.push(ArgVal::StructValue(self.clone()));
                func.arg_vals.push(ArgVal::StructValue(self.clone()));
                let pp = &func.arg_vals[func.arg_vals.len() - 1];
                Ok(pp.payload_ptr())
            }
            _ => bail!("Expected struct-by-value argument"),
        }
    }
}
impl ToArg for CStr {
    fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = func.func_def.arg_types[arg_idx].clone();

        match &arg_type {
            ArgType::CString => {
                let ptr = self.as_ptr();
                log::trace!("CStr to_arg: {:x}", ptr as u64);
                let p = unsafe { mem::transmute::<*const i8, *mut c_void>(ptr) };
                func.arg_vals.push(ArgVal::Pointer(p));
                func.arg_vals.push(ArgVal::Pointer(p));
                let pp = &func.arg_vals[func.arg_vals.len() - 1];
                log::trace!("CStr to_arg ArgVal ptr: {:p}", pp);
                let ppp = if let ArgVal::Pointer(ref p) = pp { p } else { panic!("Expected Pointer ArgVal") };
                log::trace!("CStr to_arg final ptr: {:p}", ppp);
                Ok(ppp as *const *mut c_void as *mut c_void)
            }
            _ if func.func_def.coerce => {
                let cstr = self.to_owned();
                crate::coerce::push_coerced_str(func, cstr, &arg_type)
            }
            _ => bail!("Type mismatch: expected CString, got CStr for {:?}", arg_type),
        }
    }
}

/// Implements `ToArg` for integer-like primitive types.
///
/// `$rust_ty`       — the Rust source type (e.g. `i32`)
/// `$arg_type_pat`  — the `ArgType` variant to match (e.g. `I32`)
/// `$arg_val_ctor`  — the `ArgVal` constructor to use  (e.g. `I32`)
macro_rules! impl_to_arg_int {
    ($rust_ty:ty, $arg_type_pat:ident, $arg_val_ctor:ident) => {
        impl ToArg for $rust_ty {
            fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
                let arg_idx = func.arg_ptrs.len();
                let declared = func.func_def.arg_types[arg_idx].clone();
                if matches!(declared, ArgType::$arg_type_pat) {
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    let pp = &func.arg_vals[func.arg_vals.len() - 1];
                    Ok(pp.payload_ptr())
                } else if func.func_def.coerce {
                    crate::coerce::push_coerced_int(func, *self as i64, &declared)
                } else {
                    bail!("Type mismatch: expected {:?}, got {}", declared, stringify!($rust_ty))
                }
            }
        }
    };
}

/// Implements `ToArg` for floating-point primitive types.
macro_rules! impl_to_arg_float {
    ($rust_ty:ty, $arg_type_pat:ident, $arg_val_ctor:ident) => {
        impl ToArg for $rust_ty {
            fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
                let arg_idx = func.arg_ptrs.len();
                let declared = func.func_def.arg_types[arg_idx].clone();
                if matches!(declared, ArgType::$arg_type_pat) {
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    let pp = &func.arg_vals[func.arg_vals.len() - 1];
                    Ok(pp.payload_ptr())
                } else if func.func_def.coerce {
                    crate::coerce::push_coerced_float(func, *self as f64, &declared)
                } else {
                    bail!("Type mismatch: expected {:?}, got {}", declared, stringify!($rust_ty))
                }
            }
        }
    };
}

impl_to_arg_int!(u64, U64,  U64);
impl_to_arg_int!(i64, I64,  I64);
impl_to_arg_int!(u32, U32,  U32);
impl_to_arg_int!(i32, I32,  I32);
impl_to_arg_int!(u16, U16,  U16);
impl_to_arg_int!(i16, I16,  I16);
impl_to_arg_int!(u8,  Char, Char);
impl_to_arg_int!(i8,  Char, Char);
impl_to_arg_float!(f64, F64, F64);
impl_to_arg_float!(f32, F32, F32);
impl ToMutArg for ArgVal {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void> {
        func.arg_vals.push(self.clone());
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        Ok(pp.payload_ptr())
    }
}
impl ToArg for ArgVal {
    fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
        func.arg_vals.push(self.clone());
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        Ok(pp.payload_ptr())
    }
}

/// Implements `ToMutArg` for primitive types that are passed as `*mut T` pointers.
macro_rules! impl_to_mut_arg_primitive {
    ($ty:ty) => {
        impl ToMutArg for $ty {
            fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void> {
                func.arg_vals.push(ArgVal::Pointer(self as *mut $ty as *mut c_void));
                func.arg_vals.push(ArgVal::Pointer(self as *mut $ty as *mut c_void));
                let pp = &func.arg_vals[func.arg_vals.len() - 1];
                Ok(pp.payload_ptr())
            }
        }
    };
}

impl_to_mut_arg_primitive!(i8);
impl_to_mut_arg_primitive!(u8);
impl_to_mut_arg_primitive!(i16);
impl_to_mut_arg_primitive!(u16);
impl_to_mut_arg_primitive!(i32);
impl_to_mut_arg_primitive!(u32);
impl_to_mut_arg_primitive!(i64);
impl_to_mut_arg_primitive!(u64);
impl_to_mut_arg_primitive!(usize);
impl_to_mut_arg_primitive!(f32);
impl_to_mut_arg_primitive!(f64);

impl ToMutArg for StructValue {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void> {
        let arg_idx = func.arg_ptrs.len();
        let arg_type = &func.func_def.arg_types[arg_idx];

        match arg_type {
            ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)) => {
                let ptr = self.as_mut_ptr();
                func.arg_vals.push(ArgVal::Pointer(ptr));
                func.arg_vals.push(ArgVal::Pointer(ptr));
                let pp = &func.arg_vals[func.arg_vals.len() - 1];
                Ok(pp.payload_ptr())
            }
            _ => bail!("Expected pointer-to-struct argument"),
        }
    }
}

