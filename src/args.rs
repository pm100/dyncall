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
    I8(i8),
    /// An owned string, returned for `cstr` return types.
    Str(String),
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
    /// Unsigned single byte (`u8` / `bool`).
    Char,
    /// Signed single byte (`i8`).
    I8,

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
            I8(ref val) => val as *const _ as *mut c_void,
            // Safety: callers (ToArg/ToMutArg impls) must never push CString,
            // RustString, ByteBuffer, or None into arg_vals directly.
            _ => unreachable!("ArgVal variant {:?} has no fixed-size FFI payload", self),
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
        let arg_type = &func.func_def.inner.arg_types[arg_idx];

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
        let arg_type = func.func_def.inner.arg_types[arg_idx].clone();

        match &arg_type {
            ArgType::CString => {
                let cstr = CString::new(self.as_str())
                    .map_err(|e| anyhow::anyhow!("String contains an interior nul byte: {}", e))?;
                let buffer = cstr.as_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::CString(cstr));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                Ok(pbuff.payload_ptr())
            }
            _ if func.func_def.inner.coerce => {
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
        let arg_type = &func.func_def.inner.arg_types[arg_idx];

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
        let arg_type = &func.func_def.inner.arg_types[arg_idx];

        match arg_type {
            ArgType::Struct(declared) => {
                if !declared.same_layout(self.struct_type()) {
                    bail!("StructValue layout does not match the declared struct type");
                }
                if self.pushed_field_count() != self.field_count() {
                    bail!(
                        "struct argument is incomplete: {} of {} fields pushed",
                        self.pushed_field_count(),
                        self.field_count()
                    );
                }
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
        let arg_type = func.func_def.inner.arg_types[arg_idx].clone();

        match &arg_type {
            ArgType::CString => {
                // Copy so the invocation owns the bytes: a borrowed pointer
                // could dangle if the caller drops the CStr before `call()`.
                let cstr = self.to_owned();
                let buffer = cstr.as_ptr() as *mut c_void;
                func.arg_vals.push(ArgVal::Pointer(buffer));
                func.arg_vals.push(ArgVal::CString(cstr));
                let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
                Ok(pbuff.payload_ptr())
            }
            _ if func.func_def.inner.coerce => {
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
                let declared = func.func_def.inner.arg_types[arg_idx].clone();
                if matches!(declared, ArgType::$arg_type_pat) {
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    let pp = &func.arg_vals[func.arg_vals.len() - 1];
                    Ok(pp.payload_ptr())
                } else if func.func_def.inner.coerce {
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
                let declared = func.func_def.inner.arg_types[arg_idx].clone();
                if matches!(declared, ArgType::$arg_type_pat) {
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    func.arg_vals.push(ArgVal::$arg_val_ctor(*self as _));
                    let pp = &func.arg_vals[func.arg_vals.len() - 1];
                    Ok(pp.payload_ptr())
                } else if func.func_def.inner.coerce {
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
impl_to_arg_int!(i8,  I8,   I8);
impl_to_arg_float!(f64, F64, F64);
impl_to_arg_float!(f32, F32, F32);
/// Push an [`ArgVal`] as the next argument, checking it against the declared
/// type. In `coerce` mode numeric values are converted to the declared type.
fn push_argval(val: &ArgVal, func: &mut Invocation) -> Result<*mut c_void> {
    match val {
        ArgVal::None => bail!("ArgVal::None cannot be passed as a function argument"),
        ArgVal::CString(_) => bail!("ArgVal::CString cannot be passed directly; use a String or CStr"),
        ArgVal::RustString(_) | ArgVal::ByteBuffer(_) | ArgVal::Str(_) => {
            bail!("ArgVal::Str / internal variants cannot be passed as a direct argument; use a String")
        }
        _ => {}
    }
    let declared = func.func_def.inner.arg_types[func.arg_ptrs.len()].clone();
    let matches_declared = matches!(
        (&declared, val),
        (ArgType::Char, ArgVal::Char(_))
        | (ArgType::I8, ArgVal::I8(_))
        | (ArgType::I16, ArgVal::I16(_))
        | (ArgType::U16, ArgVal::U16(_))
        | (ArgType::I32, ArgVal::I32(_))
        | (ArgType::U32, ArgVal::U32(_))
        | (ArgType::I64, ArgVal::I64(_))
        | (ArgType::U64, ArgVal::U64(_))
        | (ArgType::F32, ArgVal::F32(_))
        | (ArgType::F64, ArgVal::F64(_))
        | (ArgType::Struct(_), ArgVal::StructValue(_))
        | (
            ArgType::OpaquePointer
            | ArgType::CString
            | ArgType::OCString(_)
            | ArgType::ByteBuffer
            | ArgType::OByteBuffer(_)
            | ArgType::Pointer(_),
            ArgVal::Pointer(_),
        )
    );
    let is_pointer_decl = matches!(
        declared,
        ArgType::OpaquePointer
            | ArgType::CString
            | ArgType::OCString(_)
            | ArgType::ByteBuffer
            | ArgType::OByteBuffer(_)
            | ArgType::Pointer(_)
    );
    if !matches_declared && is_pointer_decl {
        // A pointer-sized integer (e.g. a pointer returned as u64) is accepted
        // for a pointer parameter and passed as that address.
        let addr = match val {
            ArgVal::U64(v) if mem::size_of::<usize>() == 8 => Some(*v as usize),
            ArgVal::I64(v) if mem::size_of::<usize>() == 8 => Some(*v as usize),
            ArgVal::U32(v) if mem::size_of::<usize>() == 4 => Some(*v as usize),
            ArgVal::I32(v) if mem::size_of::<usize>() == 4 => Some(*v as usize),
            _ => None,
        };
        if let Some(addr) = addr {
            let p = ArgVal::Pointer(addr as *mut c_void);
            func.arg_vals.push(p.clone());
            func.arg_vals.push(p);
            let pp = &func.arg_vals[func.arg_vals.len() - 1];
            return Ok(pp.payload_ptr());
        }
    }
    if !matches_declared {
        if func.func_def.inner.coerce {
            match val {
                ArgVal::Char(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::I8(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::I16(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::U16(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::I32(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::U32(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::I64(v) => return crate::coerce::push_coerced_int(func, *v, &declared),
                ArgVal::U64(v) => return crate::coerce::push_coerced_int(func, *v as i64, &declared),
                ArgVal::F32(v) => return crate::coerce::push_coerced_float(func, *v as f64, &declared),
                ArgVal::F64(v) => return crate::coerce::push_coerced_float(func, *v, &declared),
                _ => {}
            }
        }
        bail!("Type mismatch: expected {:?}, got {:?}", declared, val);
    }
    func.arg_vals.push(val.clone());
    func.arg_vals.push(val.clone());
    let pp = &func.arg_vals[func.arg_vals.len() - 1];
    Ok(pp.payload_ptr())
}

impl ToMutArg for ArgVal {
    fn to_mut_arg(&mut self, func: &mut Invocation) -> Result<*mut c_void> {
        push_argval(self, func)
    }
}
impl ToArg for ArgVal {
    fn to_arg(&self, func: &mut Invocation) -> Result<*mut c_void> {
        push_argval(self, func)
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
        let arg_type = &func.func_def.inner.arg_types[arg_idx];

        match arg_type {
            ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)) => {
                // Unpushed fields stay zero-initialised, so output-only structs
                // may be passed unpopulated; the layout must still match.
                if !inner.struct_type().is_some_and(|t| t.same_layout(self.struct_type())) {
                    bail!("StructValue layout does not match the declared struct type");
                }
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

