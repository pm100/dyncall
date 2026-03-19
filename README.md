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
- Supports flat primitive structs by value and pointers to those structs
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
3. Push arguments with `push_arg` (input) or `push_mut_arg` (output / pointer)
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
| `{T1,T2,...}`          | flat struct passed by value                        |
| `*{T1,T2,...}`         | pointer to a flat struct                           |
| `void`                 | no return value                                    |

Struct support in the first version is limited to **flat structs whose fields are primitive scalar types** (`i8/u8`, `i16/u16`, `i32/u32`, `i64/u64`, `f32`, `f64`). Nested structs and pointer fields inside `{...}` are not supported yet.

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

| Flag        | Meaning                                                               |
|-------------|-----------------------------------------------------------------------|
| `fixargs=N` | Variadic function with `N` fixed arguments                            |
| `coerce`    | Enable automatic type coercion when pushing arguments (see below)     |

Flags can be combined with a comma: `fixargs=1,coerce`.

### Type coercion (`coerce` flag)

By default `push_arg` returns `Err` if the Rust type of the value does not match the declared argument type. Adding `coerce` to the flags relaxes this — mismatched types are converted automatically where possible:

| Given value     | Declared type | Coercion applied                              |
|-----------------|---------------|-----------------------------------------------|
| any integer     | any integer   | widen or truncate                             |
| any integer     | `f32`/`f64`   | numeric convert (`42` → `42.0`)               |
| `f32`/`f64`     | any integer   | truncate (`3.7` → `3`)                        |
| any integer     | `cstr`        | format as decimal string (`42` → `"42"`)      |
| `String`/`CStr` | `cstr`        | direct                                        |
| `String`/`CStr` | any integer   | parse decimal (`"42"` → `42`), `Err` if not valid |
| incompatible    | any           | `Err`                                         |

```rust
// Strict mode (default) — i64 into an i32 slot returns Err
let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|")).unwrap();
let mut inv = def.prep();
assert!(inv.push_arg(&42i64).is_err());

// Coerce mode — any numeric type works, strings are parsed
let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
let mut inv = def.prep();
inv.push_arg(&42i64).unwrap();        // i64 → i32: truncate
inv = def.prep();
inv.push_arg(&"-42".to_string()).unwrap(); // String → i32: parse
let result = inv.call();
assert_eq!(*result.as_i32().unwrap(), 42);
```

## Examples

### Call `atoi`

```rust
use dyncall::DynCaller;

#[cfg(target_os = "windows")] const LIBC: &str = "msvcrt.dll";
#[cfg(target_os = "macos")]   const LIBC: &str = "libSystem.B.dylib";
#[cfg(target_os = "linux")]   const LIBC: &str = "libc.so.6";

let def = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
let mut inv = def.prep();
inv.push_arg(&"42".to_string()).unwrap();
let result = inv.call();
assert_eq!(*result.as_i32().unwrap(), 42);
```

### Call `printf` (variadic)

```rust
// printf(const char *fmt, ...) → int
// fixargs=1 means 1 fixed argument (the format string)
let def = DynCaller::define_function_by_str(
    &format!("{LIBC}|printf|cstr,cstr,i32|i32|fixargs=1")
).unwrap();
let mut inv = def.prep();
inv.push_arg(&"Hello, %s! You are %d years old.\n".to_string()).unwrap();
inv.push_arg(&"Alice".to_string()).unwrap();
inv.push_arg(&30i32).unwrap();
inv.call();
```

### Read an output argument with `sscanf`

```rust
// sscanf writes the parsed value back through a pointer argument
let def = DynCaller::define_function_by_str(
    &format!("{LIBC}|sscanf|cstr,cstr,*i32|i32|fixargs=2")
).unwrap();
let mut ans = 0i32;
let mut inv = def.prep();
inv.push_arg(&"42\n".to_string()).unwrap();
inv.push_arg(&"%d".to_string()).unwrap();
inv.push_mut_arg(&mut ans).unwrap();
inv.call();
assert_eq!(ans, 42);
```

### Pass a struct by value

```rust
let def = DynCaller::define_function_by_str(
    "myffi.dll|sum_pair|{u32,u32}|u32|"
).unwrap();

let mut inv = def.prep();
let mut pair = inv.create_struct(0).unwrap();
pair.push_field(&10u32).unwrap();
pair.push_field(&32u32).unwrap();

inv.push_arg(&pair);
let result = inv.call();
assert_eq!(*result.as_u32().unwrap(), 42);
```

### Pass a pointer to a struct

```rust
let def = DynCaller::define_function_by_str(
    "myffi.dll|bump_pair|*{u32,u32}|u32|"
).unwrap();

let mut pair = def.create_struct(0).unwrap();
pair.push_field(&7u32).unwrap();
pair.push_field(&8u32).unwrap();

let mut inv = def.prep();
inv.push_mut_arg(&mut pair);
inv.call();

assert_eq!(pair.read_field::<u32>(0).unwrap(), 8);
assert_eq!(pair.read_field::<u32>(1).unwrap(), 10);
```

### Call `localeconv`

```rust
// localeconv(void) -> struct lconv *
//
// `dyncall` can call this today by treating the return value as an opaque
// pointer. The pointed-to `struct lconv` cannot be described with `{...}`
// yet because it contains pointer fields.
let def = DynCaller::define_function_by_str(
    &format!("{LIBC}|localeconv||ptr|")
).unwrap();

let mut inv = def.prep();
let result = inv.call();
assert!(!result.as_pointer().unwrap().is_null());
```

## Real-world example: BASIC interpreter

[`basic`](https://github.com/pm100/basic) is a fork of [rodneykendall/basic](https://github.com/rodneykendall/basic), a BASIC interpreter written in Rust. The fork adds `dyncall` support, letting BASIC programs call arbitrary C functions at runtime with no Rust recompilation needed.

This is the primary use case `dyncall` is designed for: the interpreter author writes the glue layer once (translating BASIC values into `push_arg` / `push_mut_arg` calls), and from then on script authors can reach any native function simply by naming it in a descriptor string.

### BASIC syntax

```basic
DEF XFN function_name("dll|symbol|param_types|return_type|flags")
LET result = FN function_name(arg1, arg2, ...)
```

For struct arguments, the BASIC integration uses a numeric array as the backing storage. Pass the array name as the argument for `{...}` or `*{...}` descriptors, and map fields by flattened array order. If the descriptor is `*{...}`, any fields mutated by the native function are written back into the same BASIC array after the call.

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

## Standard streams on Windows

Passing `stdin`, `stdout`, or `stderr` as a `FILE *` to native functions (e.g. `fputs`, `fflush`) is platform-specific.

### Linux / macOS

Use `fopen` on the special device nodes `/dev/stdin`, `/dev/stdout`, `/dev/stderr`:

```rust
// Linux / macOS
#[cfg(target_os = "linux")]  const LIBC: &str = "libc.so.6";
#[cfg(target_os = "macos")]  const LIBC: &str = "libSystem.B.dylib";

let fopen_def = DynCaller::define_function_by_str(&format!("{LIBC}|fopen|cstr,cstr|ptr|")).unwrap();
let mut inv = fopen_def.prep();
inv.push_arg(&"/dev/stderr".to_string()).unwrap();
inv.push_arg(&"w".to_string()).unwrap();
let stderr_fp = *inv.call().as_pointer().unwrap();

let fputs_def = DynCaller::define_function_by_str(&format!("{LIBC}|fputs|cstr,ptr|i32|")).unwrap();
let mut inv = fputs_def.prep();
inv.push_arg(&"hello from dyncall".to_string()).unwrap();
inv.push_arg(&ArgVal::Pointer(stderr_fp)).unwrap();
inv.call();
```

### Windows

Windows ships multiple C runtimes (`msvcrt.dll`, `ucrtbase.dll`, etc.). Each maintains its own internal `FILE` table. If you obtain a `FILE *` from one CRT and pass it to a function from a different CRT, the call silently fails.

Use `__acrt_iob_func` from `ucrtbase.dll` to obtain a standard stream handle, then call I/O functions from the **same** `ucrtbase.dll` (indices: 0=stdin, 1=stdout, 2=stderr):

```rust
// Windows
let iob_def = DynCaller::define_function_by_str("ucrtbase.dll|__acrt_iob_func|u32|ptr|").unwrap();
let mut inv = iob_def.prep();
inv.push_arg(&2u32).unwrap(); // 2 = stderr
let stderr_fp = *inv.call().as_pointer().unwrap();

let fputs_def = DynCaller::define_function_by_str("ucrtbase.dll|fputs|cstr,ptr|i32|").unwrap();
let mut inv = fputs_def.prep();
inv.push_arg(&"hello from dyncall".to_string()).unwrap();
inv.push_arg(&ArgVal::Pointer(stderr_fp)).unwrap();
inv.call();
```

Note that `ucrtbase.dll` does **not** export `printf`, `fprintf`, `sscanf`, or `mktime`. Those remain in `msvcrt.dll`. Only use `ucrtbase.dll` for calls that accept or return `FILE *` handles.

### BASIC integration

In the BASIC integration, pointer return values are stored as numbers and can be passed directly back to subsequent calls:

**Windows:**
```basic
10 DEF XFN c_iob("ucrtbase.dll|__acrt_iob_func|u32|ptr|coerce")
20 DEF XFN c_fputs("ucrtbase.dll|fputs|cstr,ptr|i32|")
30 LET fp = FN c_iob(2)
40 LET r = FN c_fputs("hello from BASIC", fp)
```

**Linux / macOS:**
```basic
10 DEF XFN c_fopen("libc.so.6|fopen|cstr,cstr|ptr|")
20 DEF XFN c_fputs("libc.so.6|fputs|cstr,ptr|i32|")
30 LET fp = FN c_fopen("/dev/stderr", "w")
40 LET r = FN c_fputs("hello from BASIC", fp)
```


Parse an integer out of a string with `sscanf` (output pointer argument):

```basic
10 DEF XFN sscanf("msvcrt.dll|sscanf|cstr,cstr,*i32|i32|fixargs=2")
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

Pass a `struct tm *` via a BASIC array and let `strftime` write derived fields back into that same array:

```basic
10 DIM TM(8)
20 LET TM(0) = 30
30 LET TM(1) = 15
40 LET TM(2) = 8
50 LET TM(3) = 11
60 LET TM(4) = 2
70 LET TM(5) = 124
80 LET TM(6) = 0
90 LET TM(7) = 0
100 LET TM(8) = -1
110 DEF XFN mktime("msvcrt.dll|mktime|*{i32,i32,i32,i32,i32,i32,i32,i32,i32}|i64|")
120 DEF XFN strftime("msvcrt.dll|strftime|ocstr=arg1,u64,cstr,*{i32,i32,i32,i32,i32,i32,i32,i32,i32}|u64|")
130 LET out$ = ""
140 LET ts = FN mktime(TM)
150 LET n = FN strftime(out$, 64, "%Y-%m-%d %H:%M:%S", TM)
160 PRINT ts
170 PRINT out$
180 PRINT TM(6)
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
            Value::Number(n) => invoke.push_arg(&(n as i64)).unwrap(),
            Value::String(s) => invoke.push_arg(&s).unwrap(),
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

