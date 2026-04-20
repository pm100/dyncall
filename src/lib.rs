//! # dyncall
//!
//! `dyncall` is a crate for calling functions in dynamic libraries at runtime,
//! without needing compile-time knowledge of their signatures. It wraps
//! [`libffi`](https://sourceware.org/libffi/) to prepare call interfaces and
//! marshal arguments, including output (pointer) arguments written to by the
//! callee.
//!
//! ## Workflow
//!
//! 1. Define a function using a descriptor string with [`DynCaller::define_function`].
//! 2. Prepare an [`Invocation`] from the [`FuncDef`] via [`FuncDef::prep`].
//! 3. Push arguments with [`Invocation::push_arg`] (input) or [`Invocation::push_mut_arg`] (output).
//! 4. Call with [`Invocation::call`].
//!
//! ## Thread safety
//!
//! [`DynCaller::define_function`] and [`DynCaller::set_default_coerce`] are safe
//! to call from multiple threads. Library handles are cached in a global
//! `LazyLock<Mutex<DynCaller>>`; each call acquires the lock only for the
//! library-load and symbol-lookup phase, before the `ffi_cif` is prepared.
//!
//! [`Invocation`] is **not** `Send` or `Sync` — it must be used on the thread
//! that created it. Each thread should call [`FuncDef::prep`] independently to
//! obtain its own `Invocation`.
//!
//!
//! ## Function descriptor format
//!
//! ```text
//! "library|function|arg1,arg2,...|return_type|flags"
//! ```
//!
//! ### Type tokens
//!
//! | Token           | Rust / C type                                      |
//! |-----------------|----------------------------------------------------|
//! | `i8`, `u8`      | `int8_t` / `uint8_t`                               |
//! | `i16`, `u16`    | `int16_t` / `uint16_t`                             |
//! | `i32`, `u32`    | `int32_t` / `uint32_t`                             |
//! | `i64`, `u64`    | `int64_t` / `uint64_t`                             |
//! | `f32`, `f64`    | `float` / `double`                                 |
//! | `ptr`           | opaque `void *`                                    |
//! | `cstr`          | input `const char *` (null-terminated)             |
//! | `ocstr[=N\|=argK]` | output `char *` buffer; size fixed or from arg K|
//! | `obuff[=N\|=argK]` | output raw byte buffer; size fixed or from arg K|
//! | `*T`            | output pointer `T *` (e.g. `*i32`, `*f64`)        |
//! | `{T1,T2,...}`   | flat struct passed by value                        |
//! | `*{T1,T2,...}`  | pointer to a flat struct                           |
//! | `void`          | no return value                                    |
//!
//! ### Flags
//!
//! | Flag          | Meaning                                              |
//! |---------------|------------------------------------------------------|
//! | `fixargs=N`   | Variadic function with `N` fixed arguments           |
//! | `coerce`      | Enable automatic type coercion when pushing arguments|
//! | `errno`       | Capture platform error after the call; read via [`Invocation::last_errno`]|
//!
//! ## Example
//!
//! ```no_run
//! use dyncall::DynCaller;
//!
//! // Call atoi("42") from the system C library
//! #[cfg(target_os = "windows")] const LIBC: &str = "msvcrt.dll";
//! #[cfg(target_os = "macos")]   const LIBC: &str = "libSystem.B.dylib";
//! #[cfg(target_os = "linux")]   const LIBC: &str = "libc.so.6";
//!
//! let def = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
//! let mut inv = def.prep();
//! inv.push_arg(&"42".to_string()).unwrap();
//! let result = inv.call().unwrap();
//! assert_eq!(*result.as_i32().unwrap(), 42);
//! ```
//!
//! ## Error capture (`errno` flag)
//!
//! Add `errno` to the flags field to capture the platform error code
//! immediately after the call (before any other code can overwrite it).
//! The captured value is available via [`Invocation::last_errno`], which
//! returns `Some(n)` when the flag is set and `None` otherwise.
//!
//! ```no_run
//! use dyncall::DynCaller;
//! #[cfg(target_os = "windows")] const LIBC: &str = "msvcrt.dll";
//! #[cfg(target_os = "macos")]   const LIBC: &str = "libSystem.B.dylib";
//! #[cfg(target_os = "linux")]   const LIBC: &str = "libc.so.6";
//!
//! let def = DynCaller::define_function(
//!     &format!("{LIBC}|fopen|cstr,cstr|ptr|errno")
//! ).unwrap();
//! let mut inv = def.prep();
//! inv.push_arg(&"__no_such_file__.txt".to_string()).unwrap();
//! inv.push_arg(&"r".to_string()).unwrap();
//! let result = inv.call().unwrap();
//! if result.as_pointer().map(|p| p.is_null()).unwrap_or(true) {
//!     println!("fopen failed, errno={}", inv.last_errno().unwrap());
//! }
//! ```

//! For struct arguments, create a [`StructValue`] from the argument's declared
//! type, push each field in order with [`StructValue::push_field`], then pass
//! the completed value into the [`Invocation`].
//!
//! ## Standard streams (`FILE *`)
//!
//! Passing `stdin`, `stdout`, or `stderr` as a `FILE *` argument is
//! platform-specific. The approach differs between Windows and Unix.
//!
//! ### Linux / macOS
//!
//! Open the special device nodes `/dev/stdin`, `/dev/stdout`, `/dev/stderr`
//! with `fopen`. The returned `ptr` can be stored in an [`ArgVal::Pointer`]
//! and passed to any subsequent `FILE *` parameter:
//!
//! ```no_run
//! use dyncall::{DynCaller, ArgVal};
//! #[cfg(target_os = "linux")]  const LIBC: &str = "libc.so.6";
//! #[cfg(target_os = "macos")]  const LIBC: &str = "libSystem.B.dylib";
//! # #[cfg(not(any(target_os = "linux", target_os = "macos")))] const LIBC: &str = "";
//!
//! let fopen = DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|")).unwrap();
//! let mut inv = fopen.prep();
//! inv.push_arg(&"/dev/stderr".to_string()).unwrap();
//! inv.push_arg(&"w".to_string()).unwrap();
//! let stderr_fp = *inv.call().unwrap().as_pointer().unwrap();
//!
//! let fputs = DynCaller::define_function(&format!("{LIBC}|fputs|cstr,ptr|i32|")).unwrap();
//! let mut inv = fputs.prep();
//! inv.push_arg(&"hello from dyncall\n".to_string()).unwrap();
//! inv.push_arg(&ArgVal::Pointer(stderr_fp)).unwrap();
//! inv.call().unwrap();
//! ```
//!
//! ### Windows
//!
//! Windows has multiple C runtimes (`msvcrt.dll`, `ucrtbase.dll`, …), each
//! with its own `FILE` table. A `FILE *` from one CRT **cannot** be used with
//! functions from a different CRT — the call silently fails.
//!
//! Use `__acrt_iob_func(n)` from `ucrtbase.dll` to get a standard stream
//! handle (0 = stdin, 1 = stdout, 2 = stderr), then call I/O functions from
//! the **same** `ucrtbase.dll`:
//!
//! ```no_run
//! use dyncall::{DynCaller, ArgVal};
//!
//! let iob = DynCaller::define_function("ucrtbase.dll|__acrt_iob_func|u32|ptr|").unwrap();
//! let mut inv = iob.prep();
//! inv.push_arg(&2u32).unwrap(); // 2 = stderr
//! let stderr_fp = *inv.call().unwrap().as_pointer().unwrap();
//!
//! let fputs = DynCaller::define_function("ucrtbase.dll|fputs|cstr,ptr|i32|").unwrap();
//! let mut inv = fputs.prep();
//! inv.push_arg(&"hello from dyncall\n".to_string()).unwrap();
//! inv.push_arg(&ArgVal::Pointer(stderr_fp)).unwrap();
//! inv.call().unwrap();
//! ```
//!
//! Note: `ucrtbase.dll` does **not** export `printf`, `fprintf`, `sscanf`, or
//! `mktime`. Use `msvcrt.dll` for those; use `ucrtbase.dll` only for calls
//! that accept or return `FILE *`.

pub mod args;
pub mod caller;
pub(crate) mod coerce;
mod dylib;
pub mod invoke;
pub mod structs;
#[cfg(test)]
mod test;
pub use args::ArgType;
pub use args::ArgVal;
pub use args::LengthDef;
pub use caller::DynCaller;
pub use caller::FuncDef;
pub use invoke::Invocation;
pub use structs::CoerceFromField;
pub use structs::CoerceIntoField;
pub use structs::FromStructField;
pub use structs::ScriptVal;
pub use invoke::ScriptResult;
pub use structs::StructType;
pub use structs::StructValue;
pub use structs::ToStructField;
