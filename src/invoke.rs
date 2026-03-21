use std::{ffi::c_void, ptr};

use anyhow::{bail, Result};
use libc::strlen;
use libffi::raw::{
    ffi_call, FFI_TYPE_DOUBLE, FFI_TYPE_FLOAT, FFI_TYPE_POINTER, FFI_TYPE_SINT16, FFI_TYPE_SINT32,
    FFI_TYPE_SINT64, FFI_TYPE_SINT8, FFI_TYPE_UINT16, FFI_TYPE_UINT32, FFI_TYPE_UINT64,
    FFI_TYPE_UINT8, FFI_TYPE_VOID,
};

use crate::StructValue;
use crate::{
    args::{ArgType, LengthDef, ToArg, ToMutArg},
    ArgVal, FuncDef,
};

/// Read the C `errno` value for the current thread.
/// Must be called immediately after a foreign function returns.
///
/// On Windows, returns the Win32 error code from `GetLastError()`.
/// C `errno` cannot be read portably because it is stored per-CRT instance
/// and the called function may use a different CRT than this crate.
/// For most file I/O errors the Win32 and errno values coincide
/// (e.g. `ERROR_FILE_NOT_FOUND` = 2 = `ENOENT`), but they differ for some
/// errors (e.g. `EACCES` = 13 vs `ERROR_ACCESS_DENIED` = 5).
#[inline(always)]
fn read_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(0)
}
/// A single prepared call to a foreign function.
///
/// Obtained from [`FuncDef::prep`]. Arguments are pushed in declaration order
/// with [`push_arg`](Invocation::push_arg) (input) or
/// [`push_mut_arg`](Invocation::push_mut_arg) (output). Then the call is
/// executed with [`call`](Invocation::call).
///
/// After the call, the C `errno` value set by the foreign function is
/// available via [`last_errno`](Invocation::last_errno). It is captured
/// immediately after the foreign function returns, before any other code
/// can clobber it.
///
/// An `Invocation` is single-use: after calling, the argument lists are
/// cleared. Call [`FuncDef::prep`] again for the next invocation.
pub struct Invocation<'a> {
    pub(crate) func_def: &'a FuncDef,
    pub(crate) arg_ptrs: Vec<*mut c_void>,
    pub(crate) arg_vals: Vec<ArgVal>,
    pub(crate) last_errno: Option<i32>,
}
impl<'a> Invocation<'a> {
    /// Push an input argument.
    ///
    /// Arguments must be pushed in the same order as declared in the [`FuncDef`].
    /// Returns an error if more arguments are pushed than the function declares,
    /// if the argument type does not match the declared type (strict mode),
    /// or if coercion fails (coerce mode).
    pub fn push_arg<T>(&mut self, value: &T) -> Result<()>
    where
        T: ToArg + ?Sized,
    {
        self.check_arg_count()?;
        let val_count = self.arg_vals.len();
        let argp = value.to_arg(self)?;
        self.finish_push(argp, val_count)
    }

    /// Push an output argument.
    ///
    /// The callee writes its result through a pointer to `value`. After
    /// [`call`](Invocation::call) returns, `value` contains the result written
    /// by the callee. Returns an error if more arguments are pushed than the
    /// function declares.
    pub fn push_mut_arg<T>(&mut self, value: &mut T) -> Result<()>
    where
        T: ToMutArg + ?Sized,
    {
        self.check_arg_count()?;
        let val_count = self.arg_vals.len();
        let argp = value.to_mut_arg(self)?;
        self.finish_push(argp, val_count)
    }

    fn check_arg_count(&self) -> Result<()> {
        if self.arg_ptrs.len() >= self.func_def.arg_types.len() {
            bail!(
                "too many arguments: function takes {}, already pushed {}",
                self.func_def.arg_types.len(),
                self.arg_ptrs.len()
            );
        }
        Ok(())
    }

    fn finish_push(&mut self, argp: *mut c_void, prev_val_count: usize) -> Result<()> {
        if self.arg_vals.len() - prev_val_count != 2 {
            bail!("ToArg/ToMutArg impl must push exactly 2 ArgVal entries");
        }
        self.arg_ptrs.push(argp);
        Ok(())
    }

    /// Returns the declared [`ArgType`] for the argument at `index`.
    ///
    /// Useful for scripting-language runtimes that need to inspect the expected
    /// type before deciding which value to supply — for example, converting a
    /// dynamic script value to the right Rust type before calling
    /// [`push_arg`](Invocation::push_arg).
    pub fn get_arg_type(&self, index: usize) -> &ArgType {
        &self.func_def.arg_types[index]
    }

    /// Returns the number of declared arguments.
    ///
    /// Useful as an upper bound when iterating over caller-supplied values to
    /// validate that the right number of arguments will be pushed before
    /// calling [`call`](Invocation::call).
    pub fn get_arg_count(&self) -> usize {
        self.func_def.arg_types.len()
    }

    /// Push the next argument, coercing `value` from `f64` to the declared type.
    ///
    /// Useful for language runtimes that represent all numbers as `f64`
    /// (e.g. BASIC, Lox).  The declared type drives the conversion — integers
    /// are truncated via `as` cast, floats are narrowed.
    pub fn push_arg_f64(&mut self, value: f64) -> Result<()> {
        self.check_arg_count()?;
        let declared = self.func_def.arg_types[self.arg_ptrs.len()].clone();
        let val_count = self.arg_vals.len();
        let argp = crate::coerce::push_coerced_float(self, value, &declared)?;
        self.finish_push(argp, val_count)
    }

    /// Push the next argument, coercing `value` from `i64` to the declared type.
    ///
    /// Useful for language runtimes that represent all numbers as `i64`
    /// (e.g. Forth).  The declared type drives the conversion.
    pub fn push_arg_i64(&mut self, value: i64) -> Result<()> {
        self.check_arg_count()?;
        let declared = self.func_def.arg_types[self.arg_ptrs.len()].clone();
        let val_count = self.arg_vals.len();
        let argp = crate::coerce::push_coerced_int(self, value, &declared)?;
        self.finish_push(argp, val_count)
    }

    /// Create an empty [`StructValue`] for the declared argument at `index`.
    pub fn create_struct(&self, index: usize) -> anyhow::Result<StructValue> {
        self.func_def.create_struct(index)
    }
    /// Execute the call and return the result as an [`ArgVal`].
    ///
    /// Returns an error if fewer arguments have been pushed than the function
    /// declares. The variant of the returned [`ArgVal`] matches the return type
    /// declared in the [`FuncDef`]. Use the `as_*` accessors generated by
    /// [`EnumAsInner`](enum_as_inner) to extract the concrete value.
    pub fn call(&mut self) -> Result<ArgVal> {
        if self.arg_ptrs.len() < self.func_def.arg_types.len() {
            bail!(
                "too few arguments: expected {}, got {}",
                self.func_def.arg_types.len(),
                self.arg_ptrs.len()
            );
        }
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
            FFI_TYPE_VOID => ArgVal::None,
            _ => panic!("Unsupported return type"),
        };

        let addr = match result {
            ArgVal::None => ptr::null_mut(),
            _ => result.payload_ptr(),
        };
        log::trace!("call2 self={:?}", self.arg_ptrs);
        unsafe {
            ffi_call(
                &mut cif,
                Some(self.func_def.entry_point),
                addr, //as *mut c_void,
                self.arg_ptrs.as_mut_ptr(),
            );
        }
        self.last_errno = if self.func_def.capture_errno {
            Some(read_errno())
        } else {
            None
        };

        self.post_process_args();
        self.arg_ptrs.clear();
        self.arg_vals.clear();
        Ok(result)
    }

    /// Returns the platform error code captured immediately after the last call,
    /// or `None` if the `errno` flag was not set in the function descriptor.
    ///
    /// The value is saved right after the foreign function returns, before any
    /// other code can overwrite it. Enable capture by adding `errno` to the
    /// flags field of the descriptor string:
    ///
    /// ```text
    /// "libc.so.6|fopen|cstr,cstr|ptr|errno"
    /// ```
    ///
    /// **Platform behaviour:**
    /// - **Linux / macOS**: returns the C `errno` value (e.g. `ENOENT` = 2).
    /// - **Windows**: returns the Win32 error code from `GetLastError()`.
    ///   C `errno` cannot be read reliably because it is stored per-CRT instance
    ///   and the callee may use a different CRT. For common file I/O errors the
    ///   values coincide (`ERROR_FILE_NOT_FOUND` = 2 = `ENOENT`), but they differ
    ///   for others (`EACCES` = 13 vs `ERROR_ACCESS_DENIED` = 5).
    pub fn last_errno(&self) -> Option<i32> {
        self.last_errno
    }

    fn resolve_length(&self, ldef: &LengthDef) -> Option<usize> {
        match ldef {
            LengthDef::Arg(argnum) => {
                let len = match self.arg_vals.get((*argnum * 2 + 1) as usize) {
                    Some(ArgVal::U32(v)) => *v as usize,
                    Some(ArgVal::I32(v)) => *v as usize,
                    Some(ArgVal::U64(v)) => *v as usize,
                    Some(ArgVal::I64(v)) => *v as usize,
                    _ => panic!("Invalid length arg type for output buffer"),
                };
                Some(len)
            }
            LengthDef::Fixed(len) => Some(*len),
            LengthDef::None => None,
        }
    }

    fn pre_process_ocstring(&mut self, arg_idx: usize, ldef: &LengthDef) {
        let Some(len) = self.resolve_length(ldef) else { return };
        if let ArgVal::RustString(str) = &mut self.arg_vals[arg_idx * 2 + 1] {
            let s = unsafe { &mut **str };
            s.reserve(len + 1); // +1 for null terminator
            log::trace!("pre_process_ocstring reserved len={}", len);
            self.arg_vals[arg_idx * 2] = ArgVal::Pointer(s.as_mut_ptr() as *mut c_void);
        };
    }

    fn pre_process_obytebuffer(&mut self, arg_idx: usize, ldef: &LengthDef) {
        let Some(len) = self.resolve_length(ldef) else { return };
        match &mut self.arg_vals[arg_idx * 2 + 1] {
            ArgVal::RustString(str) => {
                let s = unsafe { &mut **str };
                s.reserve(len);
                log::trace!("pre_process_obytebuffer reserved len={}", len);
                self.arg_vals[arg_idx * 2] = ArgVal::Pointer(s.as_mut_ptr() as *mut c_void);
            }
            ArgVal::ByteBuffer(buffer) => {
                let buf = unsafe { &mut **buffer };
                buf.resize(len, 0);
                log::trace!("pre_process_obytebuffer resized len={}", len);
                self.arg_vals[arg_idx * 2] = ArgVal::Pointer(buf.as_mut_ptr() as *mut c_void);
            }
            _ => {}
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
                ArgType::OCString(_ldef) => {
                    if let ArgVal::Pointer(p) = self.arg_vals[i * 2] {
                        //  let &mut str = *self.arg_vals[val_idx].as_rust_string_mut().unwrap();
                        let arg_str = self.arg_vals.get_mut(i * 2 + 1).unwrap();
                        let _foo = arg_str.as_rust_string_mut().unwrap();
                        if let ArgVal::RustString(str) = arg_str {
                            unsafe {
                                let len = strlen(p as *const i8);
                                let str = &mut **str;
                                log::trace!("fgets post_process_args len={}", len);
                                str.as_mut_vec().set_len(len);
                                // let rep_str = String::from_raw_parts(*p as *mut u8, len, cap);
                            }
                        }
                    }
                }
                ArgType::CString => {}
                ArgType::OByteBuffer(_ldef) => {
                    if let Some(ArgVal::ByteBuffer(_)) = self.arg_vals.get(i * 2 + 1) {}
                }
                // Add more mutable types as needed
                _ => {}
            }
        }
    }
}
