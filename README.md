# dyncall

A Rust crate for calling functions in dynamic libraries at runtime, without needing compile-time knowledge of their signatures. It wraps [libffi](https://sourceware.org/libffi/) to prepare call interfaces and marshal arguments, including output (pointer) arguments written to by the callee.

[![CI](https://github.com/pm100/dyncall/actions/workflows/ci.yml/badge.svg)](https://github.com/pm100/dyncall/actions/workflows/ci.yml)

## Intended use

`dyncall` is designed as a building block for **scripting languages and interpreters** that need to call arbitrary native functions at runtime — without knowing their signatures at compile time and without requiring the user to write any Rust FFI code.

The typical pattern is:

1. Your scripting language includes a mechanism for the user to declare an external function (library path, symbol, argument types, return type).
2. Your interpreter calls `DynCaller::define_function` once to compile that declaration into a `FuncDef`.
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
dyncall = "0.1"
```

### Workflow

1. Define a function with [`DynCaller::define_function`]
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

Struct fields may be any of the primitive scalar types (`i8/u8`, `i16/u16`, `i32/u32`, `i64/u64`, `f32`, `f64`) or `cstr` (a `char *` pointer followed to produce an owned string). Nested structs are not supported.

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
| `errno`     | Capture platform error after call; read via `inv.last_errno()`        |

Flags can be combined with a comma: `fixargs=1,coerce`.

### Error capture (`errno` flag)

Adding `errno` to the flags causes the platform error code to be captured immediately after the foreign function returns — before any other code can overwrite it.

```rust
let def = DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|errno")).unwrap();
let mut inv = def.prep();
inv.push_arg(&"/no/such/file".to_string()).unwrap();
inv.push_arg(&"r".to_string()).unwrap();
let result = inv.call();
if result.as_pointer().map(|p| p.is_null()).unwrap_or(true) {
    println!("fopen failed, errno={}", inv.last_errno().unwrap());
}
```

`last_errno()` returns `Some(value)` when the flag is set, `None` when it is not.

**Windows note:** `std::io::Error::last_os_error()` reads `GetLastError()`, not the C `errno`. For functions that set the C errno (e.g. `fopen`, `fread`), the captured value may differ from what `errno.h` would report. Call `_errno()` from `ucrtbase.dll` to get a pointer to the thread-local C errno if needed.

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
let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|")).unwrap();
let mut inv = def.prep();
assert!(inv.push_arg(&42i64).is_err());

// Coerce mode — any numeric type works, strings are parsed
let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
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

let def = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
let mut inv = def.prep();
inv.push_arg(&"42".to_string()).unwrap();
let result = inv.call();
assert_eq!(*result.as_i32().unwrap(), 42);
```

### Call `printf` (variadic)

```rust
// printf(const char *fmt, ...) → int
// fixargs=1 means 1 fixed argument (the format string)
let def = DynCaller::define_function(
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
let def = DynCaller::define_function(
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
let def = DynCaller::define_function(
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
let def = DynCaller::define_function(
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
// The return type *{cstr,cstr} tells dyncall to follow the pointer and
// return a StructValue containing the first two fields (decimal_point,
// thousands_sep) as owned strings.
let def = DynCaller::define_function(
    &format!("{LIBC}|localeconv||*{{cstr,cstr}}|")
).unwrap();

let mut inv = def.prep();
let result = inv.call();
let sv = result.as_struct_value().unwrap();
// field 0: decimal_point, field 1: thousands_sep
println!("decimal_point: {:?}", sv.script_read(0).unwrap());
println!("thousands_sep: {:?}", sv.script_read(1).unwrap());
```

## Language forks using dyncall

These interpreter forks use `dyncall` as their FFI layer, and serve as real-world examples of the library in use:

| Language | Repository | Notes |
|----------|-----------|-------|
| BASIC | [pm100/basic](https://github.com/pm100/basic) | Fork of rodneykendall/basic |
| Forth | [pm100/forth-rs](https://github.com/pm100/forth-rs) | Stack-based Forth interpreter |
| Lox (Loxido) | [pm100/loxido](https://github.com/pm100/loxido) | Fork of tdp-org/loxido (Crafting Interpreters Lox) |

Each fork adds a thin glue layer that translates the language's runtime values into `push_arg` / `push_mut_arg` calls — the interpreter author writes this once and the script author can then call any native function by name.

For a step-by-step guide to writing your own adapter, see **[ADAPTER_GUIDE.md](ADAPTER_GUIDE.md)**.

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

let fopen_def = DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|")).unwrap();
let mut inv = fopen_def.prep();
inv.push_arg(&"/dev/stderr".to_string()).unwrap();
inv.push_arg(&"w".to_string()).unwrap();
let stderr_fp = *inv.call().as_pointer().unwrap();

let fputs_def = DynCaller::define_function(&format!("{LIBC}|fputs|cstr,ptr|i32|")).unwrap();
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
let iob_def = DynCaller::define_function("ucrtbase.dll|__acrt_iob_func|u32|ptr|").unwrap();
let mut inv = iob_def.prep();
inv.push_arg(&2u32).unwrap(); // 2 = stderr
let stderr_fp = *inv.call().as_pointer().unwrap();

let fputs_def = DynCaller::define_function("ucrtbase.dll|fputs|cstr,ptr|i32|").unwrap();
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
use dyncall::{ArgType, ArgVal, DynCaller, ScriptVal, StructValue};

// At DEF XFN time: parse the descriptor and store the FuncDef
let fdef = DynCaller::define_function(defstr).unwrap();
interpreter.external_functions.insert(name, fdef);

// At call time: marshal BASIC values into the invocation
let mut invoke = fdef.prep();
for (i, arg_value) in arg_values.into_iter().enumerate() {
    if let ArgType::Pointer(_) = fdef.get_arg_type(i) {
        invoke.push_mut_arg(&mut out_num_buffer).unwrap();   // output numeric
    } else if let ArgType::OCString(_) = fdef.get_arg_type(i) {
        invoke.push_mut_arg(&mut out_string_buffer).unwrap(); // output string
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

### Scripting adapter helpers

`dyncall` provides helpers so adapter authors don't need to work directly with `StructValue` field types.

#### Building a struct from script values

Convert your language's runtime values to [`ScriptVal`] and call [`StructValue::from_script_vals`]:

```rust
use dyncall::{ArgType, ScriptVal, StructValue};

// BASIC stores all numbers as f64; convert to ScriptVal before building
let script_vals: Vec<ScriptVal> = basic_array.iter().map(|v| match v {
    Value::Number(n) => ScriptVal::Number(*n),
    Value::String(s) => ScriptVal::Str(s.clone()),
}).collect();
let sv = StructValue::from_script_vals(arg_type, &script_vals).unwrap();
```

Forth uses `i64` on its stack — the conversion is just a cast:

```rust
let script_vals: Vec<ScriptVal> = stack_slots.iter()
    .map(|&n| ScriptVal::Number(n as f64))
    .collect();
let sv = StructValue::from_script_vals(arg_type, &script_vals).unwrap();
```

#### Reading struct fields after a call

[`StructValue::script_read`] returns a [`ScriptVal`] for each field — numeric fields as `ScriptVal::Number(f64)`, `cstr` fields as `ScriptVal::Str(String)`:

```rust
if let Some(sv) = ret.as_struct_value() {
    for fi in 0..sv.field_count() {
        match sv.script_read(fi).unwrap() {
            ScriptVal::Number(n) => write_back_number(fi, n),
            ScriptVal::Str(s)    => write_back_string(fi, s),
        }
    }
}
```

## Performance

Benchmarks measure two things: end-to-end throughput and raw per-call overhead.
Run them with `cargo bench` (results appear in `target/criterion/`).

### File copy — 50,000 lines (~96 bytes each)

Each iteration opens two files, copies every line, and closes them.
The dyncall version calls `fopen`/`fgets`/`fputs`/`fclose` through the dispatcher;
the native version uses `BufReader`/`BufWriter`.

| Approach | Time | Ratio |
|----------|------|-------|
| Native Rust | 13.6 ms | 1× |
| dyncall | 60.0 ms | ~4.4× |

The overhead comes from ~100,000 individual dyncall invocations (two per line).

### Per-call overhead — 10,000 calls to `abs`

| Approach | Total (10k calls) | Per call |
|----------|-------------------|----------|
| Native Rust `i32::abs` | ~5 µs | ~0.5 ns |
| dyncall | ~1.45 ms | ~145 ns |

Each dyncall invocation costs roughly **145 ns** — it dynamically resolves argument
types, marshals values through libffi, and dispatches the call at runtime.
This is fast enough that scripting-language overhead dominates in practice.

## Running tests

```sh
cargo test
```

Tests run against the system C library and cover `atoi`, `printf`, `fgets`, `fread`, `sscanf` (strings and all numeric types).

## License

MIT

