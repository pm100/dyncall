# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
