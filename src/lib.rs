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
//! 1. Define a function using a descriptor string with [`DynCaller::define_function_by_str`].
//! 2. Prepare an [`Invocation`] from the [`FuncDef`] via [`FuncDef::prep`].
//! 3. Push arguments with [`Invocation::push_arg`] (input) or [`Invocation::push_mut_arg`] (output).
//! 4. Call with [`Invocation::call`] or [`Invocation::call_and_return`].
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
//! | `void`          | no return value                                    |
//!
//! ### Flags
//!
//! | Flag          | Meaning                                              |
//! |---------------|------------------------------------------------------|
//! | `vararg=N`    | Variadic function with `N` fixed arguments           |
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
//! let def = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
//! let mut inv = def.prep();
//! inv.push_arg(&"42".to_string());
//! let result = inv.call();
//! assert_eq!(*result.as_i32().unwrap(), 42);
//! ```

pub mod args;
pub mod caller;
mod dylib;
pub mod invoke;
#[cfg(test)]
mod test;
pub use args::ArgType;
pub use args::ArgVal;
pub use args::LengthDef;
pub use caller::DynCaller;
pub use caller::FuncDef;
pub use invoke::Invocation;
