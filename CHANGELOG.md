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

### Tests
- Added regression tests for the code-review findings (#1, #2, #4–#8, #11–#13,
  #14–#16). Those for still-open issues fail until the fixes land.

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
