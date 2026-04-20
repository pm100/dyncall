use std::ffi::{c_char, c_void, CStr};

use anyhow::{bail, Result};

use crate::args::ArgType;

#[derive(Clone, Debug)]
pub struct StructField {
    pub arg_type: ArgType,
    pub offset: usize,
    pub size: usize,
}

#[derive(Clone, Debug)]
pub struct StructType {
    pub(crate) fields: Vec<StructField>,
    pub(crate) size: usize,
    pub(crate) alignment: usize,
}

#[derive(Clone, Debug)]
pub struct StructValue {
    layout: StructType,
    bytes: Vec<u8>,
    next_field: usize,
}

/// Write a Rust value into a struct field byte slice.
///
/// Implemented for all scalar types that can appear as struct fields:
/// `u8`, `i8` (as `Char`), `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
/// `f32`, `f64`, `*mut c_void`.
///
/// Used by [`StructValue::push_field`].
pub trait ToStructField {
    fn write_field(&self, expected: &ArgType, dst: &mut [u8]) -> Result<()>;
}

/// Read a Rust value from a struct field byte slice.
///
/// Implemented for all scalar types that can appear as struct fields:
/// `u8`, `i8` (as `Char`), `u16`, `i16`, `u32`, `i32`, `u64`, `i64`,
/// `f32`, `f64`, `*mut c_void`, `String` (follows a `char *` pointer).
///
/// Used by [`StructValue::read_field`].
pub trait FromStructField: Sized {
    fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self>;
}

/// Coerced read: converts any numeric field to `Self` regardless of the
/// declared field type. Useful for language runtimes that use a single
/// numeric representation (e.g. `f64` for BASIC/Lox, `i64` for Forth).
pub trait CoerceFromField: Sized {
    fn coerce_from_field(src_type: &ArgType, src: &[u8]) -> Result<Self>;
}

/// Coerced write: writes `self` into a field slot of any numeric type,
/// converting via `as` cast. Useful as the mirror of [`CoerceFromField`].
pub trait CoerceIntoField {
    fn coerce_into_field(&self, dst_type: &ArgType, dst: &mut [u8]) -> Result<()>;
}

impl StructType {
    pub fn new(field_types: Vec<ArgType>) -> Result<Self> {
        if field_types.is_empty() {
            bail!("Structs must declare at least one field");
        }

        let mut fields = Vec::with_capacity(field_types.len());
        let mut offset = 0usize;
        let mut alignment = 1usize;

        for field_type in field_types {
            let (field_size, field_alignment) = scalar_layout(&field_type).ok_or_else(|| {
                anyhow::anyhow!("Unsupported struct field type: {:?}", field_type)
            })?;
            offset = align_up(offset, field_alignment);
            fields.push(StructField {
                arg_type: field_type,
                offset,
                size: field_size,
            });
            offset += field_size;
            alignment = alignment.max(field_alignment);
        }

        Ok(Self {
            fields,
            size: align_up(offset, alignment),
            alignment,
        })
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn field_type(&self, index: usize) -> Option<&ArgType> {
        self.fields.get(index).map(|field| &field.arg_type)
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn alignment(&self) -> usize {
        self.alignment
    }

    pub(crate) fn field(&self, index: usize) -> Option<&StructField> {
        self.fields.get(index)
    }
}

impl StructValue {
    pub fn new(arg_type: &ArgType) -> Result<Self> {
        let struct_type = struct_type_from_arg_type(arg_type)
            .ok_or_else(|| anyhow::anyhow!("Type is not a struct or pointer-to-struct"))?;
        Ok(Self::from_struct_type(struct_type))
    }

    pub fn from_struct_type(struct_type: &StructType) -> Self {
        Self {
            layout: struct_type.clone(),
            bytes: vec![0u8; struct_type.size],
            next_field: 0,
        }
    }

    pub fn push_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ToStructField + ?Sized,
    {
        let field = self
            .layout
            .field(self.next_field)
            .ok_or_else(|| anyhow::anyhow!("All struct fields are already populated"))?;
        let start = field.offset;
        let end = field.offset + field.size;
        value.write_field(&field.arg_type, &mut self.bytes[start..end])?;
        self.next_field += 1;
        Ok(())
    }

    pub fn read_field<T>(&self, index: usize) -> Result<T>
    where
        T: FromStructField,
    {
        let field = self
            .layout
            .field(index)
            .ok_or_else(|| anyhow::anyhow!("Struct field index {} is out of range", index))?;
        let start = field.offset;
        let end = field.offset + field.size;
        T::read_field(&field.arg_type, &self.bytes[start..end])
    }

    /// Read field `index`, converting from whatever numeric type it was
    /// declared as to `T`. Avoids needing to know the exact declared type.
    pub fn read_field_coerced<T>(&self, index: usize) -> Result<T>
    where
        T: CoerceFromField,
    {
        let field = self
            .layout
            .field(index)
            .ok_or_else(|| anyhow::anyhow!("Struct field index {} is out of range", index))?;
        let start = field.offset;
        let end = field.offset + field.size;
        T::coerce_from_field(&field.arg_type, &self.bytes[start..end])
    }

    /// Push the next field, coercing `value` to the declared field type.
    /// Useful for runtimes that store all numbers as a single type (e.g. `f64`).
    pub fn push_field_coerced<T>(&mut self, value: &T) -> Result<()>
    where
        T: CoerceIntoField,
    {
        let field = self
            .layout
            .field(self.next_field)
            .ok_or_else(|| anyhow::anyhow!("All struct fields are already populated"))?;
        let start = field.offset;
        let end = field.offset + field.size;
        value.coerce_into_field(&field.arg_type, &mut self.bytes[start..end])?;
        self.next_field += 1;
        Ok(())
    }

    pub fn field_count(&self) -> usize {
        self.layout.field_count()
    }

    pub fn pushed_field_count(&self) -> usize {
        self.next_field
    }

    pub fn reset(&mut self) {
        self.bytes.fill(0);
        self.next_field = 0;
    }

    pub fn struct_type(&self) -> &StructType {
        &self.layout
    }

    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.bytes.as_ptr() as *mut c_void
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut c_void {
        self.bytes.as_mut_ptr() as *mut c_void
    }
}

pub(crate) fn struct_type_from_arg_type(arg_type: &ArgType) -> Option<&StructType> {
    match arg_type {
        ArgType::Struct(struct_type) => Some(struct_type),
        ArgType::Pointer(inner) => match inner.as_ref() {
            ArgType::Struct(struct_type) => Some(struct_type),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn scalar_layout(arg_type: &ArgType) -> Option<(usize, usize)> {
    match arg_type {
        ArgType::Char => Some((std::mem::size_of::<u8>(), std::mem::align_of::<u8>())),
        ArgType::U16 | ArgType::I16 => {
            Some((std::mem::size_of::<u16>(), std::mem::align_of::<u16>()))
        }
        ArgType::U32 | ArgType::I32 => {
            Some((std::mem::size_of::<u32>(), std::mem::align_of::<u32>()))
        }
        ArgType::U64 | ArgType::I64 => {
            Some((std::mem::size_of::<u64>(), std::mem::align_of::<u64>()))
        }
        ArgType::F32 => Some((std::mem::size_of::<f32>(), std::mem::align_of::<f32>())),
        ArgType::F64 => Some((std::mem::size_of::<f64>(), std::mem::align_of::<f64>())),
        ArgType::OpaquePointer | ArgType::CString => Some((
            std::mem::size_of::<*mut c_void>(),
            std::mem::align_of::<*mut c_void>(),
        )),
        _ => None,
    }
}

fn align_up(value: usize, alignment: usize) -> usize {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

macro_rules! impl_numeric_struct_field {
    ($ty:ty, $variant:pat, $size:expr) => {
        impl ToStructField for $ty {
            fn write_field(&self, expected: &ArgType, dst: &mut [u8]) -> Result<()> {
                match expected {
                    $variant => {
                        dst.copy_from_slice(&self.to_ne_bytes());
                        Ok(())
                    }
                    _ => bail!(
                        "Expected {:?} field, received {}",
                        expected,
                        stringify!($ty)
                    ),
                }
            }
        }

        impl FromStructField for $ty {
            fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self> {
                match expected {
                    $variant => Ok(<$ty>::from_ne_bytes(src[..$size].try_into().unwrap())),
                    _ => bail!(
                        "Expected {:?} field, requested {}",
                        expected,
                        stringify!($ty)
                    ),
                }
            }
        }
    };
}

impl_numeric_struct_field!(u16, ArgType::U16, std::mem::size_of::<u16>());
impl_numeric_struct_field!(i16, ArgType::I16, std::mem::size_of::<i16>());
impl_numeric_struct_field!(u32, ArgType::U32, std::mem::size_of::<u32>());
impl_numeric_struct_field!(i32, ArgType::I32, std::mem::size_of::<i32>());
impl_numeric_struct_field!(u64, ArgType::U64, std::mem::size_of::<u64>());
impl_numeric_struct_field!(i64, ArgType::I64, std::mem::size_of::<i64>());
impl_numeric_struct_field!(f32, ArgType::F32, std::mem::size_of::<f32>());
impl_numeric_struct_field!(f64, ArgType::F64, std::mem::size_of::<f64>());

impl ToStructField for u8 {
    fn write_field(&self, expected: &ArgType, dst: &mut [u8]) -> Result<()> {
        match expected {
            ArgType::Char => {
                dst[0] = *self;
                Ok(())
            }
            _ => bail!("Expected {:?} field, received u8", expected),
        }
    }
}

impl FromStructField for u8 {
    fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self> {
        match expected {
            ArgType::Char => Ok(src[0]),
            _ => bail!("Expected {:?} field, requested u8", expected),
        }
    }
}

impl ToStructField for i8 {
    fn write_field(&self, expected: &ArgType, dst: &mut [u8]) -> Result<()> {
        match expected {
            ArgType::Char => {
                dst.copy_from_slice(&self.to_ne_bytes());
                Ok(())
            }
            _ => bail!("Expected {:?} field, received i8", expected),
        }
    }
}

impl FromStructField for i8 {
    fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self> {
        match expected {
            ArgType::Char => Ok(i8::from_ne_bytes([src[0]])),
            _ => bail!("Expected {:?} field, requested i8", expected),
        }
    }
}

// ── Coerced field read/write ──────────────────────────────────────────────────

/// Read any numeric field from raw bytes as an `i64`, widening or reinterpreting
/// as necessary. Used by [`StructValue::read_field_coerced`].
fn read_any_numeric_as_i64(src_type: &ArgType, src: &[u8]) -> Result<i64> {
    Ok(match src_type {
        ArgType::Char => src[0] as i8 as i64,
        ArgType::I16  => i16::from_ne_bytes(src[..2].try_into().unwrap()) as i64,
        ArgType::U16  => u16::from_ne_bytes(src[..2].try_into().unwrap()) as i64,
        ArgType::I32  => i32::from_ne_bytes(src[..4].try_into().unwrap()) as i64,
        ArgType::U32  => u32::from_ne_bytes(src[..4].try_into().unwrap()) as i64,
        ArgType::I64  => i64::from_ne_bytes(src[..8].try_into().unwrap()),
        ArgType::U64  => u64::from_ne_bytes(src[..8].try_into().unwrap()) as i64,
        ArgType::F32  => f32::from_ne_bytes(src[..4].try_into().unwrap()) as i64,
        ArgType::F64  => f64::from_ne_bytes(src[..8].try_into().unwrap()) as i64,
        _ => bail!("read_field_coerced: unsupported field type {:?}", src_type),
    })
}

/// Write an `i64` into a field of any numeric type, truncating/converting as needed.
fn write_i64_into_any_numeric(value: i64, dst_type: &ArgType, dst: &mut [u8]) -> Result<()> {
    match dst_type {
        ArgType::Char => dst[0] = value as u8,
        ArgType::I16  => dst[..2].copy_from_slice(&(value as i16).to_ne_bytes()),
        ArgType::U16  => dst[..2].copy_from_slice(&(value as u16).to_ne_bytes()),
        ArgType::I32  => dst[..4].copy_from_slice(&(value as i32).to_ne_bytes()),
        ArgType::U32  => dst[..4].copy_from_slice(&(value as u32).to_ne_bytes()),
        ArgType::I64  => dst[..8].copy_from_slice(&value.to_ne_bytes()),
        ArgType::U64  => dst[..8].copy_from_slice(&(value as u64).to_ne_bytes()),
        ArgType::F32  => dst[..4].copy_from_slice(&(value as f32).to_ne_bytes()),
        ArgType::F64  => dst[..8].copy_from_slice(&(value as f64).to_ne_bytes()),
        _ => bail!("push_field_coerced: unsupported field type {:?}", dst_type),
    }
    Ok(())
}

/// Implements [`CoerceFromField`] for a numeric type, routing through `i64` as
/// the common intermediate representation.
macro_rules! impl_coerce_from_field {
    ($ty:ty) => {
        impl CoerceFromField for $ty {
            fn coerce_from_field(src_type: &ArgType, src: &[u8]) -> Result<Self> {
                Ok(read_any_numeric_as_i64(src_type, src)? as $ty)
            }
        }
    };
}

/// Implements [`CoerceIntoField`] for a numeric type, routing through `i64`.
macro_rules! impl_coerce_into_field {
    ($ty:ty) => {
        impl CoerceIntoField for $ty {
            fn coerce_into_field(&self, dst_type: &ArgType, dst: &mut [u8]) -> Result<()> {
                write_i64_into_any_numeric(*self as i64, dst_type, dst)
            }
        }
    };
}

// f64 gets special treatment to avoid precision loss on the read path.
impl CoerceFromField for f64 {
    fn coerce_from_field(src_type: &ArgType, src: &[u8]) -> Result<Self> {
        Ok(match src_type {
            ArgType::Char => src[0] as i8 as f64,
            ArgType::I16  => i16::from_ne_bytes(src[..2].try_into().unwrap()) as f64,
            ArgType::U16  => u16::from_ne_bytes(src[..2].try_into().unwrap()) as f64,
            ArgType::I32  => i32::from_ne_bytes(src[..4].try_into().unwrap()) as f64,
            ArgType::U32  => u32::from_ne_bytes(src[..4].try_into().unwrap()) as f64,
            ArgType::I64  => i64::from_ne_bytes(src[..8].try_into().unwrap()) as f64,
            ArgType::U64  => u64::from_ne_bytes(src[..8].try_into().unwrap()) as f64,
            ArgType::F32  => f32::from_ne_bytes(src[..4].try_into().unwrap()) as f64,
            ArgType::F64  => f64::from_ne_bytes(src[..8].try_into().unwrap()),
            _ => bail!("read_field_coerced: unsupported field type {:?}", src_type),
        })
    }
}

impl CoerceIntoField for f64 {
    fn coerce_into_field(&self, dst_type: &ArgType, dst: &mut [u8]) -> Result<()> {
        match dst_type {
            ArgType::F64 => { dst[..8].copy_from_slice(&self.to_ne_bytes()); Ok(()) }
            ArgType::F32 => { dst[..4].copy_from_slice(&(*self as f32).to_ne_bytes()); Ok(()) }
            _ => write_i64_into_any_numeric(*self as i64, dst_type, dst),
        }
    }
}

impl_coerce_from_field!(i8);
impl_coerce_from_field!(u8);
impl_coerce_from_field!(i16);
impl_coerce_from_field!(u16);
impl_coerce_from_field!(i32);
impl_coerce_from_field!(u32);
impl_coerce_from_field!(i64);
impl_coerce_from_field!(u64);
impl_coerce_from_field!(f32);

impl_coerce_into_field!(i8);
impl_coerce_into_field!(u8);
impl_coerce_into_field!(i16);
impl_coerce_into_field!(u16);
impl_coerce_into_field!(i32);
impl_coerce_into_field!(u32);
impl_coerce_into_field!(i64);
impl_coerce_into_field!(u64);
impl_coerce_into_field!(f32);

// ── Pointer / cstr field support ──────────────────────────────────────────────

const PTR_SIZE: usize = std::mem::size_of::<*mut c_void>();

impl ToStructField for *mut c_void {
    fn write_field(&self, expected: &ArgType, dst: &mut [u8]) -> Result<()> {
        match expected {
            ArgType::OpaquePointer | ArgType::CString => {
                let bytes = (*self as usize).to_ne_bytes();
                dst[..PTR_SIZE].copy_from_slice(&bytes);
                Ok(())
            }
            _ => bail!("Expected ptr/cstr field, received *mut c_void"),
        }
    }
}

impl FromStructField for *mut c_void {
    fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self> {
        match expected {
            ArgType::OpaquePointer | ArgType::CString => {
                let addr = usize::from_ne_bytes(src[..PTR_SIZE].try_into().unwrap());
                Ok(addr as *mut c_void)
            }
            _ => bail!("Expected ptr/cstr field, requested *mut c_void"),
        }
    }
}

/// Read a `cstr` struct field: load the pointer stored in the field bytes and
/// follow it to produce an owned `String`. Returns an error if the field is
/// not declared as `cstr`, or if the pointer is null.
impl FromStructField for String {
    fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self> {
        match expected {
            ArgType::CString => {
                let addr = usize::from_ne_bytes(src[..PTR_SIZE].try_into().unwrap());
                if addr == 0 {
                    bail!("cstr field is null");
                }
                let s = unsafe { CStr::from_ptr(addr as *const c_char) }
                    .to_string_lossy()
                    .into_owned();
                Ok(s)
            }
            _ => bail!("Expected cstr field, requested String"),
        }
    }
}

impl StructValue {
    /// Materialise a `StructValue` by copying `layout.size` bytes from `ptr`.
    ///
    /// # Safety
    /// `ptr` must point to at least `layout.size` valid, initialised bytes
    /// with the field layout described by `layout`.
    pub unsafe fn from_raw_ptr(ptr: *const c_void, layout: StructType) -> Self {
        let size = layout.size;
        let mut bytes = vec![0u8; size];
        std::ptr::copy_nonoverlapping(ptr as *const u8, bytes.as_mut_ptr(), size);
        Self { layout, bytes, next_field: 0 }
    }

    /// Build a [`StructValue`] from a slice of [`ScriptVal`]s.
    ///
    /// Each element is coerced to the declared field type.  Adapters convert
    /// their native number type (f64, i64, …) to `ScriptVal::Number` or
    /// `ScriptVal::Integer` before calling this.
    pub fn from_script_vals(arg_type: &ArgType, vals: &[ScriptVal]) -> Result<Self> {
        let mut sv = StructValue::new(arg_type)?;
        for val in vals {
            match val {
                ScriptVal::Number(n) => sv.push_field_coerced(n)?,
                ScriptVal::Integer(n) => sv.push_field_coerced(n)?,
                ScriptVal::Str(_) => sv.push_field_coerced(&0.0f64)?,
                ScriptVal::Pointer(p) => sv.push_field(p)?,
                ScriptVal::Nil => sv.push_field_coerced(&0i64)?,
            }
        }
        Ok(sv)
    }

    /// Read field `index` as a [`ScriptVal`].
    ///
    /// For `cstr` fields the stored pointer is followed to produce an owned
    /// `String`. Pointer fields return `ScriptVal::Pointer` (or `Nil` if null).
    /// All other numeric field types are widened to `f64` as `ScriptVal::Number`.
    pub fn script_read(&self, index: usize) -> Result<ScriptVal> {
        let field_type = self.layout.fields.get(index)
            .map(|f| &f.arg_type)
            .ok_or_else(|| anyhow::anyhow!("field index {} out of range", index))?;
        match field_type {
            ArgType::CString => {
                if let Ok(s) = self.read_field::<String>(index) {
                    return Ok(ScriptVal::Str(s));
                }
                Ok(ScriptVal::Nil)
            }
            ArgType::OpaquePointer => {
                let p = self.read_field::<*mut c_void>(index)?;
                Ok(ScriptVal::from(p))
            }
            _ => {
                let n = self.read_field_coerced::<f64>(index)?;
                Ok(ScriptVal::Number(n))
            }
        }
    }
}

/// A dynamically-typed value for use by scripting-language adapters.
///
/// Used with [`Invocation::push_script_val`] to push arguments and returned
/// by [`Invocation::call_scripted`] for return values and output-pointer
/// writebacks.  Also accepted by [`StructValue::from_script_vals`] and
/// returned by [`StructValue::script_read`].
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptVal {
    /// Numeric value — floats and small integers (BASIC, Lox, JS-style runtimes).
    /// All integer and float C types are widened to `f64` on read.
    Number(f64),
    /// Integer value without float precision loss (Forth, PostScript-style runtimes).
    /// Preferred over `Number` when the runtime uses `i64` as its stack type.
    Integer(i64),
    /// String value (`cstr` pointer followed to produce an owned `String`).
    Str(String),
    /// Opaque pointer (`void *`, `FILE *`, `HANDLE`, …).
    Pointer(*mut c_void),
    /// Null pointer, void return, or zero / absent value.
    Nil,
}

impl From<f64> for ScriptVal {
    fn from(v: f64) -> Self { ScriptVal::Number(v) }
}

impl From<i64> for ScriptVal {
    fn from(v: i64) -> Self { ScriptVal::Integer(v) }
}

impl From<String> for ScriptVal {
    fn from(v: String) -> Self { ScriptVal::Str(v) }
}

impl From<&str> for ScriptVal {
    fn from(v: &str) -> Self { ScriptVal::Str(v.to_string()) }
}

impl From<*mut c_void> for ScriptVal {
    fn from(v: *mut c_void) -> Self {
        if v.is_null() { ScriptVal::Nil } else { ScriptVal::Pointer(v) }
    }
}
