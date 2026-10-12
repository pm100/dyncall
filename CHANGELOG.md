# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- `FuncDef` no longer leaves dangling pointers in its `ffi_cif` after being
  cloned or moved (use-after-free). The cif, argument-type array and struct
  types now live in one `Arc`-owned allocation, and the return type points at
  libffi's static type instead of a moved copy. (#1)
- `FuncDef` is now genuinely `Clone + Send + Sync`, matching `ADAPTER_GUIDE.md`.
  Cloning is a cheap reference-count bump. (#2)
- Pushing an `ArgVal` is now checked against the declared argument type instead
  of passing mismatched bytes to libffi. With `coerce`, numeric variants are
  converted; pointer parameters also accept a pointer-sized integer. (#4)
- `StructValue` arguments are now validated against the declared struct: a
  different layout is rejected (previously libffi could read, or the callee write,
  past the buffer), and by-value structs must have every field pushed. Pointer-to-
  struct arguments may still be left partly unpopulated (zero-filled). Added
  `StructType::same_layout`. (#5)
- `call_scripted` dropped the struct for `*{...}` return types (adapters saw a
  NULL/zero). `ScriptResult` now has a `return_struct: Option<StructValue>`
  field carrying it.

- `push_mut_arg` now takes `&'a mut T` tied to the `Invocation`'s lifetime, so
  the borrow checker rejects dropping or reading a value before `call()`.
  `CStr` arguments are copied rather than borrowed. **Breaking:** code that read
  or dropped an output value before the invocation's last use no longer compiles. (#3)
- `ocstr` output buffers are zero-filled before the call and decoded lossily to
  UTF-8 afterwards: no uninitialised reads, no invalid `String`s. (#6)
- `ocstr` without a length is rejected by `define_function`; `argK` length
  references must name another integer argument. (#7, #8)
- Output buffer lengths are checked: negative values, values above 256 MiB and
  oversized argument indices return `Err` instead of panicking or aborting. (#8)
- `call()` returns `cstr` results as an owned `ArgVal::Str` (no more leaked
  `*mut String`). A NULL `const char *` is returned as a null `ArgVal::Pointer`
  / `ScriptVal::Nil` instead of an empty string. (#9, #12)
- Library load errors name the library (and symbol) and use readable OS
  messages on Windows; stale `dlerror` state is cleared. (#11)
- `i8` is now a distinct signed type (`ArgType::I8`, `ArgVal::I8`); `u8` stays
  unsigned and coerced reads of `u8` no longer sign-extend. (#13)
- New type tokens: `usize`/`size_t`, `isize`/`ssize_t`, `clong`, `culong`, `bool`. (#14)
- With the `errno` flag the platform error code is reset before the call, so a
  successful call no longer reports a stale value. (#15)
- Added `FuncDef::arg_type` (non-panicking); `FuncDef::create_struct` /
  `Invocation::create_struct` return `Err` for an out-of-range index. (#16)
- Variadic arguments declared as `f32`, `u8`/`i8` or 16-bit integers (which C
  promotes) are rejected, as is `fixargs` larger than the argument count. (#17)
- Removed the unused, unsound `DynamicLibrary::prepend_search_path`. (#10)
- MSRV raised to 1.82 (the code already required `&raw`).

### Documentation (review follow-up)
- Added a "Safety and trust" section; fixed the README `call_and_return` and
  `call()` examples, the rustdoc warnings and the "consumed" wording. (#10, #18)

### Repository
- Untracked `session`, `test.txt`, `src/test.txt` and `.vscode/`; added a
  package `exclude` list; CI now runs on macOS too and has clippy, rustdoc and
  MSRV jobs. (#19)

### Tests
- Added regression tests for the code-review findings (#1–#9, #11–#17).

### Changed
- `LengthDef::Arg` now accepts all integer-width length arguments (`u8`/`i8` via
  `Char`, `u16`, `i16`) in addition to `u32`/`i32`/`u64`/`i64`.
- `strlen` result in `post_process_args` is now capped at the allocated buffer
  capacity, preventing a potential overrun if a callee forgets to null-terminate.
- All production-code `panic!` and risky `unwrap()` calls replaced with proper
  `anyhow::bail!` / `?` error propagation.
- `CString::new` for user-supplied strings (symbol names, library paths, string
  arguments) now returns `Err` instead of panicking on interior nul bytes.
- Mutex poison on the global `DYNCALLER` is now recovered gracefully via
  `unwrap_or_else(|p| p.into_inner())`.

### Documentation
- Added thread-safety section to crate-level docs.
- Added doc comments to `ToStructField`, `FromStructField`, `CoerceFromField`,
  and `CoerceIntoField` traits.
- Added MSRV (`rust-version = "1.80"`) to `Cargo.toml`.

## [0.1.0] - Initial release
