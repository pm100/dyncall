use std::ffi;
use std::path::Path;
use std::ptr;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;
use anyhow::{anyhow, bail};

use libffi::low::{self, *};

use libffi::raw::ffi_abi_FFI_DEFAULT_ABI;

use crate::args::{ArgType, LengthDef};
use crate::dylib::DynamicLibrary;
use crate::invoke::Invocation;
use crate::structs::{StructType, StructValue};
static DYNCALLER: LazyLock<Mutex<DynCaller>> = LazyLock::new(|| Mutex::new(DynCaller::new()));
/// Named types registered with [`DynCaller::define_type`]. Kept apart from `DYNCALLER` so that
/// parsing a descriptor (which looks names up here) never needs the library-cache lock.
static TYPES: LazyLock<Mutex<HashMap<String, ArgType>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A singleton that manages loaded dynamic libraries.
///
/// `DynCaller` caches library handles so each library is only opened once.
/// It is accessed through a global `LazyLock<Mutex<DynCaller>>` internally;
/// you interact with it via the static methods [`DynCaller::define_function`]
/// and [`DynCaller::set_default_coerce`].
pub struct DynCaller {
    libs: HashMap<String, DynamicLibrary>,
    default_coerce: bool,
}

/// A compiled, reusable definition of a foreign function.
///
/// Created by [`DynCaller::define_function`]. A `FuncDef` is a cheap,
/// reference-counted handle: cloning shares the same prepared call interface,
/// and it is `Send + Sync`, so it can be stored in shared interpreter state and
/// used to create multiple independent [`Invocation`]s from any thread.
#[derive(Clone)]
pub struct FuncDef {
    pub(crate) inner: Arc<FuncDefInner>,
}

/// The immutable, heap-pinned state behind a [`FuncDef`].
///
/// Everything the `ffi_cif` points at (argument type array, owned struct
/// types) lives in this allocation, so the pointers stay valid for as long as
/// any `FuncDef` handle exists.
pub(crate) struct FuncDefInner {
    pub(crate) cif: ffi_cif,
    pub(crate) entry_point: unsafe extern "C" fn(),
    /// Owns the array `cif.arg_types` points at; must outlive the cif.
    #[allow(dead_code)]
    pub(crate) ffi_arg_types: Vec<*mut ffi_type>,
    /// Points at a libffi-owned static type (never a copy).
    pub(crate) ffi_return_type: *mut ffi_type,
    _ffi_owned_types: Vec<OwnedStructFfiType>,
    pub(crate) arg_types: Vec<ArgType>,
    pub(crate) return_type: ArgType,
    pub(crate) coerce: bool,
    pub(crate) capture_errno: bool,
}

// SAFETY: after `define_function` finishes, `FuncDefInner` is never mutated.
// The raw pointers refer either to libffi's immutable static type descriptors
// or to heap data owned by this same struct, and `ffi_call` only reads the cif.
unsafe impl Send for FuncDefInner {}
unsafe impl Sync for FuncDefInner {}

impl FuncDef {
    /// Create an [`Invocation`] ready to accept arguments for this function.
    ///
    /// Create one `Invocation` per call. An `Invocation` can technically be
    /// reused after `call`, but its output slots are only meaningful for a
    /// single call.
    pub fn prep(&self) -> Invocation<'_> {
        Invocation {
            func_def: self,
            arg_ptrs: Vec::with_capacity(self.inner.arg_types.len()),
            arg_vals: Vec::with_capacity(self.inner.arg_types.len() * 4),
            last_errno: None,
            script_output_slots: Vec::new(),
            script_struct_slots: Vec::new(),
        }
    }
    /// Returns the number of declared arguments.
    ///
    /// Useful as an upper bound when iterating over caller-supplied values to
    /// validate that the right number of arguments will be pushed before
    /// calling [`Invocation::call`].
    pub fn get_arg_count(&self) -> usize {
        self.inner.arg_types.len()
    }
    /// Returns the declared [`ArgType`] for the argument at `index`.
    ///
    /// Useful for scripting-language runtimes that need to inspect the expected
    /// type before deciding which value to supply — for example, converting a
    /// dynamic script value to the right Rust type before calling
    /// [`Invocation::push_arg`].
    ///
    /// # Panics
    /// Panics if `index >= self.get_arg_count()`. Use [`arg_type`](FuncDef::arg_type)
    /// for a non-panicking lookup.
    pub fn get_arg_type(&self, index: usize) -> &ArgType {
        &self.inner.arg_types[index]
    }

    /// Non-panicking variant of [`get_arg_type`](FuncDef::get_arg_type): returns
    /// `None` if `index` is out of range.
    pub fn arg_type(&self, index: usize) -> Option<&ArgType> {
        self.inner.arg_types.get(index)
    }
    /// Returns the declared return [`ArgType`].
    pub fn get_return_type(&self) -> &ArgType {
        &self.inner.return_type
    }

    /// Returns `true` if type coercion is enabled for this function.
    pub fn is_coerce(&self) -> bool {
        self.inner.coerce
    }

    /// Create an empty [`StructValue`] for the declared argument at `index`.
    ///
    /// This is intended for arguments declared as `{...}` or `*{...}`.
    pub fn create_struct(&self, index: usize) -> Result<StructValue> {
        let arg_type = self.arg_type(index).ok_or_else(|| {
            anyhow!(
                "argument index {} is out of range (function takes {})",
                index,
                self.get_arg_count()
            )
        })?;
        StructValue::new(arg_type)
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
            ArgType::I8 => &raw mut types::sint8,
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

    fn into_owned(self) -> Vec<OwnedStructFfiType> {
        self.owned_structs
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
            default_coerce: false,
        }
    }

    fn get_lib(&mut self, lib_name: &str) -> Result<DynamicLibrary> {
        if !self.libs.contains_key(lib_name) {
            let lib = DynamicLibrary::open(Some(Path::new(lib_name)))
                .map_err(|e| anyhow!("failed to load library '{}': {}", lib_name, e))?;
            self.libs.insert(lib_name.to_string(), lib);
        }
        Ok(self.libs[lib_name].clone())
    }

    fn get_entry_point(
        &mut self,
        lib_name: &str,
        entry_point_name: &str,
    ) -> Result<*mut ffi::c_void> {
        let lib = self.get_lib(lib_name)?;
        let ep = unsafe { lib.symbol(entry_point_name) }
            .map_err(|e| anyhow!("in library '{}': {}", lib_name, e))?;
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
        let flags = Self::parse_flags(funcdef[4])?;
        validate_signature(&my_arg_types, &flags)?;
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
            .unwrap_or_else(|p| p.into_inner())
            .get_entry_point(lib_name, entry_point_name)?;
        let entry_point = unsafe { std::mem::transmute::<*mut ffi::c_void, unsafe extern "C" fn()>(ep) };
        let coerce =
            flags.coerce || DYNCALLER.lock().unwrap_or_else(|p| p.into_inner()).default_coerce;
        let mut cif = ffi_cif::default();
        // The cif keeps raw pointers to `ffi_arg_types`' heap buffer, the struct
        // types in `ffi_store` (each boxed) and the libffi static `ffi_ret`.
        // All of those stay put when moved into the `Arc` below.
        unsafe {
            if flags.has_fixed_args {
                prep_cif_var(
                    &mut cif,
                    ffi_abi_FFI_DEFAULT_ABI,
                    flags.fixed_args as usize,
                    arg_count as usize,
                    ffi_ret,
                    ffi_arg_types.as_ptr() as *mut *mut ffi_type,
                )
                .map_err(|e| anyhow!(format!("{:?}", e)))?;
            } else {
                prep_cif(
                    &mut cif,
                    ffi_abi_FFI_DEFAULT_ABI,
                    arg_count as usize,
                    ffi_ret,
                    ffi_arg_types.as_ptr() as *mut *mut ffi_type,
                )
                .map_err(|e| anyhow!(format!("{:?}", e)))?;
            }
        };

        Ok(FuncDef {
            inner: Arc::new(FuncDefInner {
                cif,
                entry_point,
                ffi_arg_types,
                ffi_return_type: ffi_ret,
                _ffi_owned_types: ffi_store.into_owned(),
                arg_types: my_arg_types,
                return_type: my_ret,
                coerce,
                capture_errno: flags.capture_errno,
            }),
        })
    }
    /// Register a named type that descriptors can use anywhere a type token is accepted.
    ///
    /// `definition` is any type in descriptor syntax (a struct, a pointer, a scalar, or
    /// another alias); it is parsed immediately, so later redefinitions of the names it
    /// mentions do not affect it. Aliases are process-wide, like the library cache.
    /// Redefining an existing alias replaces it for functions defined afterwards;
    /// already-defined [`FuncDef`]s keep the type they were built with.
    ///
    /// ```rust
    /// use dyncall::DynCaller;
    /// DynCaller::define_type("Point", "{i32,i32}").unwrap();
    /// // `*Point` is a pointer to the struct, `Point` the struct by value.
    /// # #[cfg(target_os = "windows")] const LIBC: &str = "msvcrt.dll";
    /// # #[cfg(target_os = "macos")] const LIBC: &str = "libSystem.B.dylib";
    /// # #[cfg(target_os = "linux")] const LIBC: &str = "libc.so.6";
    /// let def = DynCaller::define_function(&format!("{LIBC}|memcmp|*Point,*Point,usize|i32|")).unwrap();
    /// assert_eq!(def.get_arg_count(), 3);
    /// ```
    ///
    /// # Errors
    ///
    /// The name must be an identifier (`[A-Za-z_][A-Za-z0-9_]*`) and must not be one of the
    /// built-in type tokens (`i32`, `ptr`, `cstr`, ...); the definition must parse.
    pub fn define_type(name: &str, definition: &str) -> Result<()> {
        let valid = name
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()));
        if name.is_empty() || !valid {
            bail!("invalid type name '{}': must be an identifier", name);
        }
        if is_builtin_type_name(name) {
            bail!("'{}' is a built-in type and cannot be redefined", name);
        }
        let parsed = parse_arg_type(definition)?;
        if matches!(parsed, ArgType::Void) {
            bail!("a type alias cannot be void");
        }
        TYPES.lock().unwrap_or_else(|p| p.into_inner()).insert(name.to_string(), parsed);
        Ok(())
    }

    /// Set or clear the process-wide default coerce flag.
    ///
    /// When enabled, every subsequent call to [`DynCaller::define_function`]
    /// behaves as if the descriptor included `coerce` in its flags — even when
    /// the descriptor itself omits it.  A descriptor that already includes
    /// `coerce` is unaffected (the flag is OR-d in).
    ///
    /// This is intended for scripting-language adapters whose runtime value
    /// model is always loosely typed (e.g. everything is an `f64`).  Call this
    /// once during interpreter startup instead of appending `coerce` to every
    /// descriptor string your users write.
    ///
    /// ```rust
    /// use dyncall::DynCaller;
    /// DynCaller::set_default_coerce(true);
    /// // All define_function calls from here on have coerce enabled
    /// ```
    pub fn set_default_coerce(enabled: bool) {
        DYNCALLER.lock().unwrap_or_else(|p| p.into_inner()).default_coerce = enabled;
    }

    /// Returns the current process-wide default coerce setting.
    pub fn default_coerce() -> bool {
        DYNCALLER.lock().unwrap_or_else(|p| p.into_inner()).default_coerce
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

fn is_integer_type(t: &ArgType) -> bool {
    matches!(
        t,
        ArgType::Char
            | ArgType::I8
            | ArgType::I16
            | ArgType::U16
            | ArgType::I32
            | ArgType::U32
            | ArgType::I64
            | ArgType::U64
    )
}

/// Reject descriptors that would be unsafe or meaningless to call.
fn validate_signature(args: &[ArgType], flags: &Flags) -> Result<()> {
    for (i, arg) in args.iter().enumerate() {
        match arg {
            ArgType::OCString(LengthDef::None) => {
                bail!("argument {}: ocstr needs a buffer length (ocstr=N or ocstr=argK)", i)
            }
            ArgType::OCString(LengthDef::Arg(k)) | ArgType::OByteBuffer(LengthDef::Arg(k)) => {
                let k = *k as usize;
                match args.get(k) {
                    None => bail!(
                        "argument {}: length argument arg{} is out of range ({} arguments)",
                        i,
                        k,
                        args.len()
                    ),
                    Some(_) if k == i => bail!("argument {}: length argument refers to itself", i),
                    Some(t) if !is_integer_type(t) => {
                        bail!("argument {}: length argument arg{} is not an integer type", i, k)
                    }
                    Some(_) => {}
                }
            }
            _ => {}
        }
    }
    if flags.has_fixed_args {
        let fixed = flags.fixed_args as usize;
        if fixed > args.len() {
            bail!("fixargs={} exceeds the {} declared arguments", fixed, args.len());
        }
        // C promotes these when passed through `...`; declaring them as variadic
        // arguments would pass the wrong width.
        for (i, arg) in args.iter().enumerate().skip(fixed) {
            if matches!(arg, ArgType::F32 | ArgType::Char | ArgType::I8 | ArgType::I16 | ArgType::U16) {
                bail!(
                    "variadic argument {} is declared as {:?}, which C promotes; use f64 for floats or i32/u32 for small integers",
                    i,
                    arg
                );
            }
        }
    }
    Ok(())
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
        "u8" | "bool" => ArgType::Char,
        "i8" => ArgType::I8,
        "u16" => ArgType::U16,
        "i16" => ArgType::I16,
        "u32" => ArgType::U32,
        "i32" => ArgType::I32,
        "u64" => ArgType::U64,
        "i64" => ArgType::I64,
        // Portable C types whose width depends on the target.
        "usize" | "size_t" => {
            if cfg!(target_pointer_width = "64") { ArgType::U64 } else { ArgType::U32 }
        }
        "isize" | "ssize_t" => {
            if cfg!(target_pointer_width = "64") { ArgType::I64 } else { ArgType::I32 }
        }
        // C `long`: 32-bit on Windows and 32-bit targets, 64-bit on LP64 Unix.
        "clong" => {
            if cfg!(any(windows, target_pointer_width = "32")) { ArgType::I32 } else { ArgType::I64 }
        }
        "culong" => {
            if cfg!(any(windows, target_pointer_width = "32")) { ArgType::U32 } else { ArgType::U64 }
        }
        "f32" => ArgType::F32,
        "f64" => ArgType::F64,
        "ptr" => ArgType::OpaquePointer,
        "cstr" => ArgType::CString,
        "ocstr" => ArgType::OCString(parse_length_def(qualifier)?),
        "obuff" => ArgType::OByteBuffer(parse_length_def(qualifier)?),
        "buff" => ArgType::ByteBuffer,
        "void" => ArgType::Void,
        name => match TYPES.lock().unwrap_or_else(|p| p.into_inner()).get(name) {
            Some(t) if qualifier.is_none() => t.clone(),
            _ => bail!("unknown type {}", at),
        },
    })
}

/// Built-in type tokens (and `ocstr`/`obuff`/`buff` qualifier bases) that aliases may not shadow.
fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "bool" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "usize" | "size_t" | "isize"
            | "ssize_t" | "clong" | "culong" | "f32" | "f64" | "ptr" | "cstr" | "ocstr" | "obuff" | "buff" | "void"
    )
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
