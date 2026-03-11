# dyncall

A Rust crate for calling functions in dynamic libraries at runtime, without needing compile-time knowledge of their signatures. It wraps [libffi](https://sourceware.org/libffi/) to prepare call interfaces and marshal arguments, including output (pointer) arguments written to by the callee.

[![CI](https://github.com/pm100/dyncall/actions/workflows/ci.yml/badge.svg)](https://github.com/pm100/dyncall/actions/workflows/ci.yml)

## Features

- Call any function in a shared library using a simple descriptor string
- Supports all common numeric types, C strings, and raw byte buffers
- Handles output (pointer) arguments — values written back by the callee
- Supports variadic functions (`printf`, `sscanf`, etc.)
- Cross-platform: Windows, macOS, Linux

## Usage

Add to `Cargo.toml`:

```toml
[dependencies]
dyncall = { git = "https://github.com/pm100/dyncall" }
```

### Workflow

1. Define a function with [`DynCaller::define_function_by_str`]
2. Prepare an [`Invocation`] via [`FuncDef::prep`]
3. Push arguments with `push_arg` (input) or `push_mut_arg` (output)
4. Call with `call` or `call_and_return`

## Function descriptor format

```
"library|function|arg1,arg2,...|return_type|flags"
```

All five `|`-separated fields are required (the last two may be empty).

### Type tokens

| Token                  | C / Rust type                                      |
|------------------------|----------------------------------------------------|
| `i8`, `u8`             | `int8_t` / `uint8_t`                               |
| `i16`, `u16`           | `int16_t` / `uint16_t`                             |
| `i32`, `u32`           | `int32_t` / `uint32_t`                             |
| `i64`, `u64`           | `int64_t` / `uint64_t`                             |
| `f32`, `f64`           | `float` / `double`                                 |
| `ptr`                  | opaque `void *`                                    |
| `cstr`                 | input `const char *` (null-terminated)             |
| `ocstr[=N\|=argK]`     | output `char *` buffer; size fixed or from arg K   |
| `obuff[=N\|=argK]`     | output raw byte buffer; size fixed or from arg K   |
| `*T`                   | output pointer `T *` (e.g. `*i32`, `*f64`)         |
| `void`                 | no return value                                    |

### Flags

| Flag        | Meaning                                            |
|-------------|----------------------------------------------------|
| `vararg=N`  | Variadic function with `N` fixed arguments         |

## Examples

### Call `atoi`

```rust
use dyncall::DynCaller;

#[cfg(target_os = "windows")] const LIBC: &str = "msvcrt.dll";
#[cfg(target_os = "macos")]   const LIBC: &str = "libSystem.B.dylib";
#[cfg(target_os = "linux")]   const LIBC: &str = "libc.so.6";

let def = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
let mut inv = def.prep();
inv.push_arg(&"42".to_string());
let result = inv.call();
assert_eq!(*result.as_i32().unwrap(), 42);
```

### Call `printf` (variadic)

```rust
// printf(const char *fmt, ...) → int
// vararg=1 means 1 fixed argument (the format string)
let def = DynCaller::define_function_by_str(
    &format!("{LIBC}|printf|cstr,cstr,i32|i32|vararg=1")
).unwrap();
let mut inv = def.prep();
inv.push_arg(&"Hello, %s! You are %d years old.\n".to_string());
inv.push_arg(&"Alice".to_string());
inv.push_arg(&30i32);
inv.call();
```

### Read an output argument with `sscanf`

```rust
// sscanf writes the parsed value back through a pointer argument
let def = DynCaller::define_function_by_str(
    &format!("{LIBC}|sscanf|cstr,cstr,*i32|i32|vararg=2")
).unwrap();
let mut ans = 0i32;
let mut inv = def.prep();
inv.push_arg(&"42\n".to_string());
inv.push_arg(&"%d".to_string());
inv.push_mut_arg(&mut ans);
inv.call();
assert_eq!(ans, 42);
```

## Running tests

```sh
cargo test
```

Tests run against the system C library and cover `atoi`, `printf`, `fgets`, `fread`, `sscanf` (strings and all numeric types).

## License

MIT
