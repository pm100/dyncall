use std::ffi;
use std::path::Path;
use std::ptr;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use anyhow::{anyhow, bail};

use libffi::low::{self, *};

use libffi::raw::ffi_abi_FFI_DEFAULT_ABI;

use crate::args::{ArgType, LengthDef};
use crate::dylib::DynamicLibrary;
use crate::invoke::Invocation;
use crate::structs::{StructType, StructValue};
static DYNCALLER: LazyLock<Mutex<DynCaller>> = LazyLock::new(|| Mutex::new(DynCaller::new()));

/// A singleton that manages loaded dynamic libraries.
///
/// `DynCaller` caches library handles so each library is only opened once.
/// It is accessed through a global `LazyLock<Mutex<DynCaller>>` internally;
/// you interact with it only via the static method [`DynCaller::define_function`].
pub struct DynCaller {
    libs: HashMap<String, DynamicLibrary>,
}

/// A compiled, reusable definition of a foreign function.
///
/// Created by [`DynCaller::define_function`]. A `FuncDef` is cheap to
/// clone and can be used to create multiple independent [`Invocation`]s.
#[derive(Clone)]
pub struct FuncDef {
    pub(crate) cif: ffi_cif,
    pub(crate) entry_point: unsafe extern "C" fn(),
    pub(crate) ffi_arg_types: Vec<*mut ffi_type>,
    pub(crate) ffi_return_type: ffi_type,
    _ffi_owned_types: Rc<Vec<OwnedStructFfiType>>,
    pub(crate) arg_types: Vec<ArgType>,
    pub(crate) return_type: ArgType,
    pub(crate) coerce: bool,
    pub(crate) capture_errno: bool,
}
impl FuncDef {
    /// Create an [`Invocation`] ready to accept arguments for this function.
    ///
    /// Call this once per invocation; the `Invocation` is consumed by the call.
    pub fn prep(&self) -> Invocation<'_> {
        Invocation {
            func_def: self,
            arg_ptrs: Vec::with_capacity(self.arg_types.len()),
            arg_vals: Vec::with_capacity(self.arg_types.len() * 4),
            last_errno: None,
        }
    }
    /// Returns the number of declared arguments.
    ///
    /// Useful as an upper bound when iterating over caller-supplied values to
    /// validate that the right number of arguments will be pushed before
    /// calling [`Invocation::call`].
    pub fn get_arg_count(&self) -> usize {
        self.arg_types.len()
    }
    /// Returns the declared [`ArgType`] for the argument at `index`.
    ///
    /// Useful for scripting-language runtimes that need to inspect the expected
    /// type before deciding which value to supply — for example, converting a
    /// dynamic script value to the right Rust type before calling
    /// [`Invocation::push_arg`].
    pub fn get_arg_type(&self, index: usize) -> &ArgType {
        &self.arg_types[index]
    }
    /// Returns the declared return [`ArgType`].
    pub fn get_return_type(&self) -> &ArgType {
        &self.return_type
    }

    /// Returns `true` if type coercion is enabled for this function.
    pub fn is_coerce(&self) -> bool {
        self.coerce
    }

    /// Create an empty [`StructValue`] for the declared argument at `index`.
    ///
    /// This is intended for arguments declared as `{...}` or `*{...}`.
    pub fn create_struct(&self, index: usize) -> Result<StructValue> {
        StructValue::new(self.get_arg_type(index))
    }
}

struct OwnedStructFfiType {
    ffi_type: Box<ffi_type>,
    _elements: Box<[*mut ffi_type]>,
}

impl OwnedStructFfiType {
    fn new(mut elements: Vec<*mut ffi_type>) -> Self {
        elements.push(ptr::null_mut());
        let mut elements = elements.into_boxed_slice();
        let ffi_type = Box::new(ffi_type {
            type_: low::type_tag::STRUCT,
            elements: elements.as_mut_ptr(),
            ..Default::default()
        });
        Self {
            ffi_type,
            _elements: elements,
        }
    }

    fn as_mut_ptr(&mut self) -> *mut ffi_type {
        self.ffi_type.as_mut()
    }
}

struct FfiTypeStore {
    owned_structs: Vec<OwnedStructFfiType>,
}

impl FfiTypeStore {
    fn new() -> Self {
        Self {
            owned_structs: Vec::new(),
        }
    }

    fn ffi_type_for(&mut self, arg_type: &ArgType) -> Result<*mut ffi_type> {
        Ok(match arg_type {
            ArgType::Char => &raw mut types::uint8,
            ArgType::U16 => &raw mut types::uint16,
            ArgType::I16 => &raw mut types::sint16,
            ArgType::U32 => &raw mut types::uint32,
            ArgType::I32 => &raw mut types::sint32,
            ArgType::U64 => &raw mut types::uint64,
            ArgType::I64 => &raw mut types::sint64,
            ArgType::F32 => &raw mut types::float,
            ArgType::F64 => &raw mut types::double,
            ArgType::OpaquePointer
            | ArgType::CString
            | ArgType::OCString(_)
            | ArgType::ByteBuffer
            | ArgType::OByteBuffer(_)
            | ArgType::Pointer(_) => &raw mut types::pointer,
            ArgType::Struct(struct_type) => {
                let elements = struct_type
                    .fields
                    .iter()
                    .map(|field| self.ffi_type_for(&field.arg_type))
                    .collect::<Result<Vec<_>>>()?;
                let mut owned = OwnedStructFfiType::new(elements);
                let ptr = owned.as_mut_ptr();
                self.owned_structs.push(owned);
                ptr
            }
            ArgType::Void => &raw mut types::void,
        })
    }

    fn into_arc(self) -> Rc<Vec<OwnedStructFfiType>> {
        Rc::new(self.owned_structs)
    }
}

struct Flags {
    has_fixed_args: bool,
    fixed_args: u8,
    coerce: bool,
    capture_errno: bool,
}
impl DynCaller {
    fn new() -> Self {
        DynCaller {
            libs: HashMap::new(),
        }
    }

    fn get_lib(&mut self, lib_name: &str) -> Result<DynamicLibrary> {
        if self.libs.contains_key(lib_name) {
            return Ok(self.libs.get(lib_name).unwrap().clone());
        }

        let lib = DynamicLibrary::open(Some(Path::new(lib_name)))?;

        self.libs.insert(lib_name.to_string(), lib);
        Ok(self.libs.get(lib_name).unwrap().clone())
    }

    fn get_entry_point(
        &mut self,
        lib_name: &str,
        entry_point_name: &str,
    ) -> Result<*mut ffi::c_void> {
        let lib = self.get_lib(lib_name)?;
        let ep = unsafe { lib.symbol(entry_point_name)? };
        Ok(ep)
    }
    /// Parse a function descriptor string, load the library, and return a [`FuncDef`].
    ///
    /// # Descriptor format
    ///
    /// ```text
    /// "library|function|arg1,arg2,...|return_type|flags"
    /// ```
    ///
    /// All five `|`-separated fields are required (the last two may be empty).
    ///
    /// See the [crate-level documentation](crate) for the full list of type tokens and flags.
    ///
    /// # Errors
    ///
    /// Returns an error if the descriptor is malformed, the library cannot be
    /// loaded, or the symbol cannot be found.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use dyncall::DynCaller;
    /// // printf(const char *fmt, ...) → int
    /// let def = DynCaller::define_function(
    ///     "msvcrt.dll|printf|cstr,i32|i32|fixargs=1"
    /// ).unwrap();
    /// ```
    pub fn define_function(funcdef: &str) -> Result<FuncDef> {
        let funcdef = funcdef.split("|").collect::<Vec<&str>>();
        if funcdef.len() != 5 {
            bail!("Invalid function definition format. Expected 'lib_name|entry_point_name|arg1,arg2,arg3|return_type|flags'");
        };
        let lib_name = funcdef[0];
        let entry_point_name = funcdef[1];
        let args_str = funcdef[2];
        let my_arg_types = parse_arg_types(args_str)?;
        let mut ffi_store = FfiTypeStore::new();
        let ffi_arg_types = my_arg_types
            .iter()
            .map(|arg_type| ffi_store.ffi_type_for(arg_type))
            .collect::<Result<Vec<_>>>()?;
        let return_type_str = funcdef[3];
        let my_ret = parse_arg_type(return_type_str)?;
        if matches!(my_ret, ArgType::Struct(_)) {
            bail!("Struct return values are not supported; use *{{...}} to return a pointer to a struct");
        }
        let ffi_ret = ffi_store.ffi_type_for(&my_ret)?;
        let arg_count = my_arg_types.len();
        let ep = DYNCALLER
            .lock()
            .unwrap()
            .get_entry_point(lib_name, entry_point_name)?;
        let entry_point = unsafe { std::mem::transmute::<*mut ffi::c_void, unsafe extern "C" fn()>(ep) };
        let flag_str = funcdef[4];
        let mut func = FuncDef {
            cif: ffi_cif::default(),
            entry_point,
            ffi_arg_types,
            ffi_return_type: unsafe { *ffi_ret },
            _ffi_owned_types: ffi_store.into_arc(),
            arg_types: my_arg_types,
            return_type: my_ret,
            coerce: false,
            capture_errno: false,
        };
        let flags = Self::parse_flags(flag_str)?;
        func.coerce = flags.coerce;
        func.capture_errno = flags.capture_errno;
        unsafe {
            if flags.has_fixed_args {
                prep_cif_var(
                    &mut func.cif,
                    ffi_abi_FFI_DEFAULT_ABI,
                    flags.fixed_args as usize,
                    arg_count as usize,
                    &mut func.ffi_return_type,
                    func.ffi_arg_types.as_ptr() as *mut *mut ffi_type,
                )
                .map_err(|e| anyhow!(format!("{:?}", e)))?;
            } else {
                prep_cif(
                    &mut func.cif,
                    ffi_abi_FFI_DEFAULT_ABI,
                    arg_count as usize,
                    &mut func.ffi_return_type,
                    func.ffi_arg_types.as_ptr() as *mut *mut ffi_type,
                )
                .map_err(|e| anyhow!(format!("{:?}", e)))?;
            }
        };

        Ok(func)
    }
    fn parse_flags(flag_str: &str) -> Result<Flags> {
        let mut flags = Flags {
            has_fixed_args: false,
            fixed_args: 0,
            coerce: false,
            capture_errno: false,
        };
        for flag in flag_str.split(',') {
            let flag = flag.trim();
            if flag == "coerce" {
                flags.coerce = true;
            } else if flag == "errno" {
                flags.capture_errno = true;
            } else if let Some(fixed_count) = flag.strip_prefix("fixargs=") {
                flags.has_fixed_args = true;
                flags.fixed_args = fixed_count.parse::<u8>()?;
            } else if !flag.is_empty() {
                bail!("Unknown flag: {}", flag);
            }
        }
        Ok(flags)
    }
}

fn parse_def(def: &str) -> (&str, Option<&str>) {

    let parts = def.split('=').collect::<Vec<&str>>();
    if parts.len() == 2 {
        return (parts[0], Some(parts[1]));
    }
    (parts[0], None)
}
fn parse_arg_types(args_str: &str) -> Result<Vec<ArgType>> {
    if args_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    split_top_level(args_str, ',')?
        .into_iter()
        .map(parse_arg_type)
        .collect()
}

fn parse_arg_type(at: &str) -> Result<ArgType> {
    let at = at.trim();
    if let Some(rest) = at.strip_prefix('*') {
        return Ok(ArgType::Pointer(Box::new(parse_arg_type(rest)?)));
    }
    if at.starts_with('{') {
        return Ok(ArgType::Struct(parse_struct_type(at)?));
    }

    let (parsed_arg, qualifier) = parse_def(at);
    Ok(match parsed_arg.trim() {
        "u8" | "i8" => ArgType::Char,
        "u16" => ArgType::U16,
        "i16" => ArgType::I16,
        "u32" => ArgType::U32,
        "i32" => ArgType::I32,
        "u64" => ArgType::U64,
        "i64" => ArgType::I64,
        "f32" => ArgType::F32,
        "f64" => ArgType::F64,
        "ptr" => ArgType::OpaquePointer,
        "cstr" => ArgType::CString,
        "ocstr" => ArgType::OCString(parse_length_def(qualifier)?),
        "obuff" => ArgType::OByteBuffer(parse_length_def(qualifier)?),
        "buff" => ArgType::ByteBuffer,
        "void" => ArgType::Void,
        _ => bail!("unknown type {}", at),
    })
}

fn parse_struct_type(definition: &str) -> Result<StructType> {
    if !definition.ends_with('}') {
        bail!("Struct type {} is missing a closing brace", definition);
    }
    let inner = &definition[1..definition.len() - 1];
    let field_types = split_top_level(inner, ',')?
        .into_iter()
        .map(parse_arg_type)
        .collect::<Result<Vec<_>>>()?;
    StructType::new(field_types)
}

fn parse_length_def(qualifier: Option<&str>) -> Result<LengthDef> {
    if let Some(qual) = qualifier {
        if let Some(rest) = qual.strip_prefix("arg") {
            return Ok(LengthDef::Arg(rest.parse()?));
        }
        return Ok(LengthDef::Fixed(qual.parse()?));
    }
    Ok(LengthDef::None)
}

fn split_top_level(input: &str, separator: char) -> Result<Vec<&str>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    bail!("Unmatched closing brace in {}", input);
                }
                depth -= 1;
            }
            _ if ch == separator && depth == 0 => {
                parts.push(input[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    if depth != 0 {
        bail!("Unclosed struct type in {}", input);
    }

    parts.push(input[start..].trim());
    Ok(parts)
}
