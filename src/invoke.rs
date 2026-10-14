use std::{ffi::c_void, ptr};

use anyhow::{bail, Result};
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
use crate::structs::ScriptVal;

/// Private storage for a boxed output slot created by [`Invocation::push_script_val`].
pub(crate) enum ScriptOutputSlot {
    Char(Box<u8>),
    I8(Box<i8>),
    I16(Box<i16>),
    U16(Box<u16>),
    I32(Box<i32>),
    U32(Box<u32>),
    I64(Box<i64>),
    U64(Box<u64>),
    F32(Box<f32>),
    F64(Box<f64>),
    Ptr(Box<*mut c_void>),
    // Boxed so the `String` header has a stable address while the callee runs.
    #[allow(clippy::box_collection)]
    OcString(Box<String>),
}

impl ScriptOutputSlot {
    fn to_script_val(&self) -> ScriptVal {
        match self {
            Self::Char(v) => ScriptVal::Integer(**v as i64),
            Self::I8(v) => ScriptVal::Integer(**v as i64),
            Self::I16(v) => ScriptVal::Integer(**v as i64),
            Self::U16(v) => ScriptVal::Integer(**v as i64),
            Self::I32(v) => ScriptVal::Integer(**v as i64),
            Self::U32(v) => ScriptVal::Integer(**v as i64),
            Self::I64(v) => ScriptVal::Integer(**v),
            Self::U64(v) => ScriptVal::Integer(**v as i64),
            Self::F32(v) => ScriptVal::Number(**v as f64),
            Self::F64(v) => ScriptVal::Number(**v),
            Self::Ptr(v) => ScriptVal::from(**v),
            Self::OcString(v) => ScriptVal::Str(*v.clone()),
        }
    }
}

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
/// Reset the platform error code before a call (see [`read_errno`]).
#[inline(always)]
fn clear_errno() {
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetLastError(code: u32);
        }
        SetLastError(0);
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe {
        *libc::__errno_location() = 0;
    }
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    unsafe {
        *libc::__error() = 0;
    }
}

/// Upper bound on a caller-requested output buffer (`ocstr=`/`obuff=`), so a
/// bad length value cannot make the process allocate unbounded memory.
const MAX_OUTPUT_BUFFER: usize = 256 * 1024 * 1024;

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
/// After calling, the argument lists are cleared. Call [`FuncDef::prep`] again
/// for the next invocation rather than reusing one.
pub struct Invocation<'a> {
    pub(crate) func_def: &'a FuncDef,
    pub(crate) arg_ptrs: Vec<*mut c_void>,
    pub(crate) arg_vals: Vec<ArgVal>,
    pub(crate) last_errno: Option<i32>,
    /// Output slots allocated by push_script_val; (arg_index, slot).
    pub(crate) script_output_slots: Vec<(usize, ScriptOutputSlot)>,
    /// Pointer-to-struct arguments owned by the invocation (see `push_script_struct`);
    /// (arg_index, struct). Boxed so the bytes keep a stable address while the callee runs.
    pub(crate) script_struct_slots: Vec<(usize, Box<StructValue>)>,
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
    ///
    /// The `Invocation` keeps a raw pointer to `value`, so `value` stays
    /// mutably borrowed until the invocation's last use (normally
    /// [`call`](Invocation::call)). The borrow checker therefore rejects code
    /// that drops or reads `value` before the call:
    ///
    /// ```compile_fail
    /// # use dyncall::DynCaller;
    /// # fn demo(def: &dyncall::FuncDef) {
    /// let mut inv = def.prep();
    /// {
    ///     let mut n = 0i32;
    ///     inv.push_mut_arg(&mut n).unwrap();
    /// } // `n` dropped here while `inv` still points at it
    /// inv.call().unwrap();
    /// # }
    /// ```
    pub fn push_mut_arg<T>(&mut self, value: &'a mut T) -> Result<()>
    where
        T: ToMutArg + ?Sized,
    {
        self.check_arg_count()?;
        let val_count = self.arg_vals.len();
        let argp = value.to_mut_arg(self)?;
        self.finish_push(argp, val_count)
    }

    fn check_arg_count(&self) -> Result<()> {
        if self.arg_ptrs.len() >= self.func_def.inner.arg_types.len() {
            bail!(
                "too many arguments: function takes {}, already pushed {}",
                self.func_def.inner.arg_types.len(),
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
        &self.func_def.inner.arg_types[index]
    }

    /// Returns the number of declared arguments.
    ///
    /// Useful as an upper bound when iterating over caller-supplied values to
    /// validate that the right number of arguments will be pushed before
    /// calling [`call`](Invocation::call).
    pub fn get_arg_count(&self) -> usize {
        self.func_def.inner.arg_types.len()
    }

    /// Push the next argument using a [`ScriptVal`].
    ///
    /// This is the primary API for scripting-language adapters. The declared
    /// [`ArgType`] drives all type conversion — you do not need to inspect it
    /// yourself.
    ///
    /// - Scalar input types (`i8`–`f64`): coerced from `Number`/`Integer`.
    /// - `cstr`: extracted from `Str`.
    /// - `ocstr`: buffer allocated from `Str` content; written back via
    ///   [`ScriptResult::outputs`] after [`call_scripted`](Invocation::call_scripted).
    /// - `ptr` (opaque pointer): extracted from `Pointer` or `Nil`.
    /// - `*T` (output pointer): boxed from the initial value; written back via
    ///   [`ScriptResult::outputs`] after [`call_scripted`](Invocation::call_scripted).
    /// - Struct args (`{…}` / `*{…}`): return an error — use
    ///   [`push_arg`](Invocation::push_arg) with a [`StructValue`] directly.
    pub fn push_script_val(&mut self, val: ScriptVal) -> Result<()> {
        self.check_arg_count()?;
        let arg_index = self.arg_ptrs.len();
        let declared = self.func_def.inner.arg_types[arg_index].clone();
        let val_count = self.arg_vals.len();

        let argp = match &declared {
            // ── output pointer *T ─────────────────────────────────────────
            // The C function receives a pointer to T and writes through it.
            // We box the initial value, store the box in script_output_slots,
            // and push ArgVal::Pointer(ptr_to_T) twice so pre_process / payload_ptr
            // work correctly.
            ArgType::Pointer(inner) if !matches!(inner.as_ref(), ArgType::Struct(_)) => {
                // Helper macro: box an initial value, push it, record the slot.
                macro_rules! push_out_slot {
                    ($ty:ty, $variant:ident, $init:expr) => {{
                        let mut b: Box<$ty> = Box::new($init);
                        let ptr_to_val = b.as_mut() as *mut $ty as *mut c_void;
                        self.arg_vals.push(ArgVal::Pointer(ptr_to_val));
                        self.arg_vals.push(ArgVal::Pointer(ptr_to_val));
                        // payload_ptr() of the second entry = &inner_ptr in arg_vals[val_count+1]
                        let argp = self.arg_vals[val_count + 1].payload_ptr();
                        self.script_output_slots.push((arg_index, ScriptOutputSlot::$variant(b)));
                        argp
                    }};
                }
                match inner.as_ref() {
                    ArgType::Char         => push_out_slot!(u8,  Char, script_val_as_u8(&val)),
                    ArgType::I8           => push_out_slot!(i8,  I8,   script_val_as_i64(&val) as i8),
                    ArgType::I16          => push_out_slot!(i16, I16,  script_val_as_i64(&val) as i16),
                    ArgType::U16          => push_out_slot!(u16, U16,  script_val_as_i64(&val) as u16),
                    ArgType::I32          => push_out_slot!(i32, I32,  script_val_as_i64(&val) as i32),
                    ArgType::U32          => push_out_slot!(u32, U32,  script_val_as_i64(&val) as u32),
                    ArgType::I64          => push_out_slot!(i64, I64,  script_val_as_i64(&val)),
                    ArgType::U64          => push_out_slot!(u64, U64,  script_val_as_i64(&val) as u64),
                    ArgType::F32          => push_out_slot!(f32, F32,  script_val_as_f64(&val) as f32),
                    ArgType::F64          => push_out_slot!(f64, F64,  script_val_as_f64(&val)),
                    ArgType::OpaquePointer => push_out_slot!(*mut c_void, Ptr, script_val_as_ptr(&val)),
                    other => bail!("push_script_val: unsupported output pointer inner type {:?}", other),
                }
            }
            // ── OCString output buffer ────────────────────────────────────
            // Pattern mirrors ToMutArg for String:
            //   arg_vals[i*2]   = Pointer(buf_ptr)  — updated by pre_process_ocstring
            //   arg_vals[i*2+1] = RustString(str_ptr) — used by post_process_args
            //   arg_ptrs[i]     = payload_ptr() of arg_vals[i*2]
            ArgType::OCString(_) => {
                let init = match &val {
                    ScriptVal::Str(s) => s.clone(),
                    _ => String::new(),
                };
                let mut b: Box<String> = Box::new(init);
                // Initial data pointer (pre_process will update after reserve).
                let buf_ptr = b.as_mut_ptr() as *mut c_void;
                // Pointer to the String struct itself (for post_process_args).
                let str_ptr: *mut String = &mut *b;
                self.arg_vals.push(ArgVal::Pointer(buf_ptr));
                self.arg_vals.push(ArgVal::RustString(str_ptr));
                let argp = self.arg_vals[val_count].payload_ptr();
                // Move b into the slot AFTER getting the raw pointers above.
                self.script_output_slots.push((arg_index, ScriptOutputSlot::OcString(b)));
                argp
            }
            // ── opaque pointer input ──────────────────────────────────────
            ArgType::OpaquePointer => {
                let p: *mut c_void = match &val {
                    ScriptVal::Pointer(p) => *p,
                    ScriptVal::Integer(n) => *n as usize as *mut c_void,
                    ScriptVal::Number(f) => *f as i64 as usize as *mut c_void,
                    ScriptVal::Nil => ptr::null_mut(),
                    other => bail!("push_script_val: cannot push {:?} for ptr arg", other),
                };
                self.arg_vals.push(ArgVal::Pointer(p));
                self.arg_vals.push(ArgVal::Pointer(p));
                self.arg_vals[val_count + 1].payload_ptr()
            }
            // ── cstr input ────────────────────────────────────────────────
            ArgType::CString => {
                let s = match val {
                    ScriptVal::Str(s) => s,
                    ScriptVal::Nil => String::new(),
                    other => bail!("push_script_val: expected Str for cstr arg, got {:?}", other),
                };
                // to_arg for String handles the CString allocation; returns payload_ptr.
                s.to_arg(self)?
            }
            // ── struct — not supported via ScriptVal ──────────────────────
            ArgType::Struct(_) | ArgType::Pointer(_) => {
                bail!("push_script_val: struct arguments must use push_script_struct (or push_arg with a StructValue)");
            }
            // ── scalar numeric ────────────────────────────────────────────
            _ => match &val {
                ScriptVal::Number(n) => crate::coerce::push_coerced_float(self, *n, &declared)?,
                ScriptVal::Integer(n) => crate::coerce::push_coerced_int(self, *n, &declared)?,
                ScriptVal::Nil => crate::coerce::push_coerced_int(self, 0, &declared)?,
                other => bail!("push_script_val: cannot push {:?} for scalar arg {:?}", other, declared),
            },
        };
        self.finish_push(argp, val_count)
    }

    /// Push the next argument, a struct (`{...}`) or pointer to a struct (`*{...}`), from a
    /// list of field values.
    ///
    /// The scripted counterpart of [`push_script_val`](Invocation::push_script_val) for
    /// struct arguments: the invocation builds and owns the [`StructValue`], so adapters need
    /// no separate storage that outlives the call and no borrow juggling. Fields are given
    /// in declaration order and coerced to the declared field types
    /// ([`StructValue::from_script_vals`]); fields left out stay zero for `*{...}`
    /// arguments, while a by-value struct must be given in full.
    ///
    /// For `*{...}` arguments the callee may modify the struct. After
    /// [`call_scripted`](Invocation::call_scripted) the final field values are in
    /// [`ScriptResult::struct_outputs`] under this argument's index.
    pub fn push_script_struct(&mut self, fields: &[ScriptVal]) -> Result<()> {
        self.check_arg_count()?;
        let arg_index = self.arg_ptrs.len();
        let declared = self.func_def.inner.arg_types[arg_index].clone();
        let is_ptr = match &declared {
            ArgType::Struct(_) => false,
            ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)) => true,
            other => bail!("push_script_struct: argument {} is declared as {:?}, not a struct", arg_index, other),
        };
        let sv = StructValue::from_script_vals(&declared, fields)?;
        if !is_ptr {
            return self.push_arg(&sv);
        }
        let mut slot = Box::new(sv);
        let ptr = slot.as_mut_ptr();
        let val_count = self.arg_vals.len();
        self.arg_vals.push(ArgVal::Pointer(ptr));
        self.arg_vals.push(ArgVal::Pointer(ptr));
        let argp = self.arg_vals[val_count + 1].payload_ptr();
        self.script_struct_slots.push((arg_index, slot));
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
        if self.arg_ptrs.len() < self.func_def.inner.arg_types.len() {
            bail!(
                "too few arguments: expected {}, got {}",
                self.func_def.inner.arg_types.len(),
                self.arg_ptrs.len()
            );
        }
        let mut cif = self.func_def.inner.cif;
        self.pre_process_args()?;
        let result = match unsafe { (*self.func_def.inner.ffi_return_type).type_ } as u32 {
            FFI_TYPE_POINTER => ArgVal::Pointer(ptr::null_mut()),
            FFI_TYPE_UINT64 => ArgVal::U64(0),
            FFI_TYPE_SINT64 => ArgVal::I64(0),
            FFI_TYPE_UINT32 => ArgVal::U32(0),
            FFI_TYPE_SINT32 => ArgVal::I32(0),
            FFI_TYPE_SINT16 => ArgVal::I16(0),
            FFI_TYPE_UINT16 => ArgVal::U16(0),
            FFI_TYPE_UINT8 => ArgVal::Char(0),
            FFI_TYPE_SINT8 => ArgVal::I8(0),
            FFI_TYPE_FLOAT => ArgVal::F32(0.0),
            FFI_TYPE_DOUBLE => ArgVal::F64(0.0),
            FFI_TYPE_VOID => ArgVal::None,
            t => bail!("Unsupported FFI return type code: {}", t),
        };

        let addr = match result {
            ArgVal::None => ptr::null_mut(),
            _ => result.payload_ptr(),
        };
        log::trace!("call2 self={:?}", self.arg_ptrs);
        if self.func_def.inner.capture_errno {
            // Reset first so a successful call cannot report a stale value.
            clear_errno();
        }
        unsafe {
            ffi_call(
                &mut cif,
                Some(self.func_def.inner.entry_point),
                addr, //as *mut c_void,
                self.arg_ptrs.as_mut_ptr(),
            );
        }
        self.last_errno = if self.func_def.inner.capture_errno {
            Some(read_errno())
        } else {
            None
        };

        self.post_process_args();
        self.arg_ptrs.clear();
        self.arg_vals.clear();

        // Post-process the return value based on the declared return type.
        let result = match &self.func_def.inner.return_type {
            // *{...} — callee returned a pointer to a struct; copy into StructValue.
            ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)) => {
                let ArgType::Struct(struct_type) = inner.as_ref() else { unreachable!() };
                let raw_ptr = if let ArgVal::Pointer(p) = result { p } else { ptr::null_mut() };
                if raw_ptr.is_null() {
                    ArgVal::Pointer(ptr::null_mut())
                } else {
                    let sv = unsafe {
                        crate::structs::StructValue::from_raw_ptr(raw_ptr as *const c_void, struct_type.clone())
                    };
                    ArgVal::StructValue(sv)
                }
            }
            // cstr — callee returned a `const char *`; dereference into a Rust String.
            // A NULL pointer is returned as `ArgVal::Pointer(null)` so it can be
            // told apart from an empty string.
            ArgType::CString => {
                let raw_ptr = if let ArgVal::Pointer(p) = result { p } else { ptr::null_mut() };
                if raw_ptr.is_null() {
                    ArgVal::Pointer(ptr::null_mut())
                } else {
                    let cstr = unsafe { std::ffi::CStr::from_ptr(raw_ptr as *const std::ffi::c_char) };
                    ArgVal::Str(cstr.to_string_lossy().into_owned())
                }
            }
            _ => result,
        };

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

    fn resolve_length(&self, ldef: &LengthDef) -> Result<Option<usize>> {
        let len: i128 = match ldef {
            LengthDef::Arg(argnum) => match self.arg_vals.get(*argnum as usize * 2 + 1) {
                Some(ArgVal::Char(v)) => *v as i128,
                Some(ArgVal::I8(v)) => *v as i128,
                Some(ArgVal::U16(v)) => *v as i128,
                Some(ArgVal::I16(v)) => *v as i128,
                Some(ArgVal::U32(v)) => *v as i128,
                Some(ArgVal::I32(v)) => *v as i128,
                Some(ArgVal::U64(v)) => *v as i128,
                Some(ArgVal::I64(v)) => *v as i128,
                _ => bail!("length arg {} is not an integer type", argnum),
            },
            LengthDef::Fixed(len) => *len as i128,
            LengthDef::None => return Ok(None),
        };
        if len < 0 {
            bail!("output buffer length is negative ({})", len);
        }
        if len > MAX_OUTPUT_BUFFER as i128 {
            bail!(
                "output buffer length {} exceeds the maximum of {} bytes",
                len,
                MAX_OUTPUT_BUFFER
            );
        }
        Ok(Some(len as usize))
    }

    fn pre_process_ocstring(&mut self, arg_idx: usize, ldef: &LengthDef) -> Result<()> {
        let Some(len) = self.resolve_length(ldef)? else {
            bail!("ocstr argument {} has no length; declare it as ocstr=N or ocstr=argK", arg_idx);
        };
        if let ArgVal::RustString(str) = &mut self.arg_vals[arg_idx * 2 + 1] {
            let s = unsafe { &mut **str };
            let used = s.len();
            s.reserve(len + 1); // +1 for null terminator
            let cap = s.capacity();
            let p = s.as_mut_ptr();
            // Zero the spare capacity so the buffer is always initialised and
            // NUL-terminated even if the callee writes nothing.
            unsafe { ptr::write_bytes(p.add(used), 0, cap - used) };
            log::trace!("pre_process_ocstring reserved len={}", len);
            self.arg_vals[arg_idx * 2] = ArgVal::Pointer(p as *mut c_void);
        };
        Ok(())
    }

    fn pre_process_obytebuffer(&mut self, arg_idx: usize, ldef: &LengthDef) -> Result<()> {
        let Some(len) = self.resolve_length(ldef)? else { return Ok(()) };
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
        Ok(())
    }

    fn pre_process_args(&mut self) -> Result<()> {
        for (i, arg_type) in self.func_def.inner.arg_types.iter().enumerate() {
            match arg_type {
                ArgType::OCString(ldef) => {
                    self.pre_process_ocstring(i, ldef)?;
                }
                ArgType::OByteBuffer(ldef) => {
                    self.pre_process_obytebuffer(i, ldef)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn post_process_args(&mut self) {
        for (i, arg_type) in self.func_def.inner.arg_types.iter().enumerate() {
            match arg_type {
                ArgType::OCString(_ldef) => {
                    if let ArgVal::Pointer(p) = self.arg_vals[i * 2] {
                        if let ArgVal::RustString(str) = self.arg_vals.get_mut(i * 2 + 1).unwrap() {
                            let str = unsafe { &mut **str };
                            // The buffer was zero-filled to its full capacity before
                            // the call, so scanning to the first NUL stays in bounds.
                            let bytes = unsafe {
                                std::slice::from_raw_parts(p as *const u8, str.capacity())
                            };
                            let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                            log::trace!("post_process_args ocstr len={}", len);
                            // Callee output may not be valid UTF-8.
                            let text = String::from_utf8_lossy(&bytes[..len]).into_owned();
                            *str = text;
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

// ── ScriptVal helpers ─────────────────────────────────────────────────────────

fn script_val_as_i64(v: &ScriptVal) -> i64 {
    match v {
        ScriptVal::Integer(n) => *n,
        ScriptVal::Number(f) => *f as i64,
        ScriptVal::Nil => 0,
        _ => 0,
    }
}

fn script_val_as_f64(v: &ScriptVal) -> f64 {
    match v {
        ScriptVal::Number(f) => *f,
        ScriptVal::Integer(n) => *n as f64,
        ScriptVal::Nil => 0.0,
        _ => 0.0,
    }
}

fn script_val_as_u8(v: &ScriptVal) -> u8 {
    script_val_as_i64(v) as u8
}

fn script_val_as_ptr(v: &ScriptVal) -> *mut c_void {
    match v {
        ScriptVal::Pointer(p) => *p,
        _ => ptr::null_mut(),
    }
}

// ── ScriptResult ──────────────────────────────────────────────────────────────

/// Result of a [`call_scripted`](Invocation::call_scripted) invocation.
///
/// Contains the function's return value and any values written back through
/// output-pointer arguments that were pushed with
/// [`push_script_val`](Invocation::push_script_val).
#[derive(Debug)]
pub struct ScriptResult {
    /// Return value of the foreign function.
    ///
    /// - Numeric return types → [`ScriptVal::Integer`] (for all integer types)
    ///   or [`ScriptVal::Number`] (for `f32`/`f64`).
    /// - `cstr` → [`ScriptVal::Str`].
    /// - `ptr` → [`ScriptVal::Pointer`] or [`ScriptVal::Nil`] (if null).
    /// - `void` → [`ScriptVal::Nil`].
    /// - `*{...}` → [`ScriptVal::Nil`]; the struct is in
    ///   [`return_struct`](ScriptResult::return_struct).
    /// - Struct-by-value return types → not supported by descriptors.
    pub return_val: ScriptVal,
    /// For a `*{...}` return type: a snapshot copy of the struct the callee
    /// returned a pointer to, or `None` if the pointer was NULL (or the return
    /// type is not a struct pointer). `return_val` is [`ScriptVal::Nil`] in
    /// this case.
    pub return_struct: Option<StructValue>,
    /// Written-back values for each output-pointer argument.
    ///
    /// Each entry is `(arg_index, value)` where `arg_index` is the zero-based
    /// position of the argument in the function's parameter list.  Entries
    /// appear in push order (ascending `arg_index`).
    pub outputs: Vec<(usize, ScriptVal)>,
    /// Final field values of every `*{...}` argument pushed with
    /// [`push_script_struct`](Invocation::push_script_struct), as `(arg_index, fields)` in
    /// push order. Integer fields are read as [`ScriptVal::Number`] (see
    /// [`StructValue::script_read`]).
    pub struct_outputs: Vec<(usize, Vec<ScriptVal>)>,
}

impl<'a> Invocation<'a> {
    /// Execute the call and return the result as a [`ScriptResult`].
    ///
    /// This is the high-level counterpart to [`call`](Invocation::call).
    /// It reads back all output-pointer slots that were pushed with
    /// [`push_script_val`](Invocation::push_script_val) and converts the
    /// return value to a [`ScriptVal`].
    ///
    /// # Errors
    /// - If fewer arguments have been pushed than declared.
    /// - If the function returns a struct (use [`call`](Invocation::call) for
    ///   struct returns).
    pub fn call_scripted(&mut self) -> Result<ScriptResult> {
        let return_type = self.func_def.inner.return_type.clone();

        let ret_arg_val = self.call()?;
        let mut return_struct = None;

        // Convert return ArgVal → ScriptVal
        let return_val = match &return_type {
            ArgType::F32 => {
                if let ArgVal::F32(v) = ret_arg_val { ScriptVal::Number(v as f64) }
                else { ScriptVal::Nil }
            }
            ArgType::F64 => {
                if let ArgVal::F64(v) = ret_arg_val { ScriptVal::Number(v) }
                else { ScriptVal::Nil }
            }
            ArgType::CString => {
                match ret_arg_val {
                    ArgVal::Str(s) => ScriptVal::Str(s),
                    _ => ScriptVal::Nil, // NULL
                }
            }
            ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)) => {
                if let ArgVal::StructValue(sv) = ret_arg_val {
                    return_struct = Some(sv);
                }
                ScriptVal::Nil
            }
            ArgType::OpaquePointer | ArgType::Pointer(_) => {
                if let ArgVal::Pointer(p) = ret_arg_val {
                    ScriptVal::from(p)
                } else {
                    ScriptVal::Nil
                }
            }
            ArgType::Struct(_) => {
                bail!("call_scripted: struct return types are not supported; use call()");
            }
            _ => {
                // All integer-ish types
                match ret_arg_val {
                    ArgVal::Char(v)  => ScriptVal::Integer(v as i64),
                    ArgVal::I8(v)    => ScriptVal::Integer(v as i64),
                    ArgVal::I16(v)   => ScriptVal::Integer(v as i64),
                    ArgVal::U16(v)   => ScriptVal::Integer(v as i64),
                    ArgVal::I32(v)   => ScriptVal::Integer(v as i64),
                    ArgVal::U32(v)   => ScriptVal::Integer(v as i64),
                    ArgVal::I64(v)   => ScriptVal::Integer(v),
                    ArgVal::U64(v)   => ScriptVal::Integer(v as i64),
                    ArgVal::None     => ScriptVal::Nil,
                    _                => ScriptVal::Nil,
                }
            }
        };

        // Drain the output slots (call() cleared arg_vals but not these).
        let outputs = self.script_output_slots
            .drain(..)
            .map(|(idx, slot)| (idx, slot.to_script_val()))
            .collect();

        let struct_outputs = self
            .script_struct_slots
            .drain(..)
            .map(|(idx, sv)| {
                let fields = (0..sv.field_count()).map(|i| sv.script_read(i)).collect::<Result<Vec<_>>>()?;
                Ok((idx, fields))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ScriptResult { return_val, return_struct, outputs, struct_outputs })
    }
}
