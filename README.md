# dyncall

A Rust crate for calling functions in dynamic libraries at runtime, without needing compile-time knowledge of their signatures. It wraps [libffi](https://sourceware.org/libffi/) to prepare call interfaces and marshal arguments, including output (pointer) arguments written to by the callee.

[![CI](https://github.com/pm100/dyncall/actions/workflows/ci.yml/badge.svg)](https://github.com/pm100/dyncall/actions/workflows/ci.yml)

## Intended use

`dyncall` is designed as a building block for **scripting languages and interpreters** that need to call arbitrary native functions at runtime — without knowing their signatures at compile time and without requiring the user to write any Rust FFI code.

The typical pattern is:

1. Your scripting language includes a mechanism for the user to declare an external function (library path, symbol, argument types, return type).
2. Your interpreter calls `DynCaller::define_function_by_str` once to compile that declaration into a `FuncDef`.
3. Each time the user's script invokes the function, you create an `Invocation`, push the script's runtime values as arguments, and call it.

The Rust examples in this README demonstrate the raw `dyncall` API. In practice, **you** write the glue layer that translates your language's values into `push_arg` / `push_mut_arg` calls — `dyncall` handles everything below that.

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

### Output buffer sizing (`ocstr` and `obuff`)

For output string and byte-buffer arguments, `dyncall` needs to know how large a buffer to allocate before making the call, because the C function writes into the buffer without knowing Rust's allocation. There are two ways to specify the size:

- **`=N`** — allocate a fixed buffer of exactly N bytes (e.g. `ocstr=256`).
- **`=argK`** — read the size at call time from argument K (zero-based index). Use this when the buffer size is passed as a separate argument to the same function, as is conventional in many C APIs (e.g. `GetTempPathA(nBufferLength, lpBuffer)`).

```
"kernel32.dll|GetTempPathA|u32,ocstr=arg0|u32|"
                                    ^^^^
                arg 1 (the buffer) gets its size from arg 0 (the u32 length)
```

If neither qualifier is given the buffer is not pre-allocated; the callee must not write to it (useful only when the pointer itself is the meaningful value).

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

## Real-world example: BASIC interpreter

[`basic`](https://github.com/pm100/basic) is a fork of [rodneykendall/basic](https://github.com/rodneykendall/basic), a BASIC interpreter written in Rust. The fork adds `dyncall` support, letting BASIC programs call arbitrary C functions at runtime with no Rust recompilation needed.

This is the primary use case `dyncall` is designed for: the interpreter author writes the glue layer once (translating BASIC values into `push_arg` / `push_mut_arg` calls), and from then on script authors can reach any native function simply by naming it in a descriptor string.

### BASIC syntax

```basic
DEF XFN function_name("dll|symbol|param_types|return_type|flags")
LET result = FN function_name(arg1, arg2, ...)
```

### Sample BASIC programs

Call `atoi` to parse a string to an integer:

```basic
10 DEF XFN atoi("msvcrt.dll|atoi|cstr|i32|")
20 LET x = FN atoi("42")
30 PRINT x
```

Compare two strings with `strcmp`:

```basic
10 DEF XFN strcmp("msvcrt.dll|strcmp|cstr,cstr|i32|")
20 LET result = FN strcmp("apple", "banana")
30 PRINT "strcmp result: "; result
```

Parse an integer out of a string with `sscanf` (output pointer argument):

```basic
10 DEF XFN sscanf("msvcrt.dll|sscanf|cstr,cstr,*i32|i32|vararg=2")
20 LET x = 0
30 LET n = FN sscanf("42", "%d", x)
40 PRINT "Parsed: "; x
```

Get a Windows temp path via `GetTempPathA` (output string buffer):

```basic
10 DEF XFN GetTempPathA("kernel32.dll|GetTempPathA|u32,ocstr=arg0|u32|")
20 LET buffer = ""
30 LET pathlen = FN GetTempPathA(260, buffer)
40 PRINT "Temp path: "; buffer
```

### Rust integration

The interpreter registers external functions at `DEF XFN` time and calls them via `FuncDef::prep` / `push_arg` / `push_mut_arg`:

```rust
use dyncall::{ArgType, ArgVal, FuncDef};

// At DEF XFN time: parse the descriptor and store the FuncDef
let fdef = DynCaller::define_function_by_str(defstr).unwrap();
interpreter.external_functions.insert(name, fdef);

// At call time: marshal BASIC values into the invocation
let mut invoke = fdef.prep();
for (i, arg_value) in arg_values.into_iter().enumerate() {
    if let ArgType::Pointer(_) = fdef.get_arg_type(i) {
        invoke.push_mut_arg(&mut out_num_buffer);   // output numeric
    } else if let ArgType::OCString(_) = fdef.get_arg_type(i) {
        invoke.push_mut_arg(&mut out_string_buffer); // output string
    } else {
        match arg_value {
            Value::Number(n) => invoke.push_arg(&(n as i64)),
            Value::String(s) => invoke.push_arg(&s),
        }
    }
}
let ret = invoke.call();
```

After the call, updated output buffers are written back to BASIC variables automatically.

## Running tests

```sh
cargo test
```

Tests run against the system C library and cover `atoi`, `printf`, `fgets`, `fread`, `sscanf` (strings and all numeric types).

## License

MIT
