use std::ffi::c_void;

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

pub trait ToStructField {
    fn write_field(&self, expected: &ArgType, dst: &mut [u8]) -> Result<()>;
}

pub trait FromStructField: Sized {
    fn read_field(expected: &ArgType, src: &[u8]) -> Result<Self>;
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
