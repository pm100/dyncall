# Writing a dyncall adapter for your scripting language

This guide is for authors who want to integrate `dyncall` into a scripting
language interpreter or REPL, so that script authors can call arbitrary native
functions at runtime without writing any Rust FFI code themselves.

## Contents

- [What dyncall does for you](#what-dyncall-does-for-you)
- [Core types](#core-types)
- [Mapping your language's value model](#mapping-your-languages-value-model)
- [Registration: def-time work](#registration-def-time-work)
- [Dispatch: call-time work](#dispatch-call-time-work)
  - [Scalar input arguments](#scalar-input-arguments)
  - [Output (pointer) arguments](#output-pointer-arguments)
  - [Output string buffers (ocstr)](#output-string-buffers-ocstr)
  - [Struct arguments](#struct-arguments)
  - [Making the call and reading the result](#making-the-call-and-reading-the-result)
- [The coerce flag](#the-coerce-flag)
- [The errno flag](#the-errno-flag)
- [Complete worked examples](#complete-worked-examples)
  - [BASIC interpreter (f64 value model)](#basic-interpreter-f64-value-model)
  - [Forth interpreter (i64 value model)](#forth-interpreter-i64-value-model)
  - [Lox interpreter (object value model)](#lox-interpreter-object-value-model)
- [Summary checklist](#summary-checklist)

---

## What dyncall does for you

`dyncall` handles the hard, unsafe, platform-specific work of calling a C
function whose signature is not known at compile time:

- Opens the shared library and resolves the symbol
- Builds a `libffi` call interface from a descriptor string
- Allocates and pins all argument buffers
- Dispatches the call through libffi
- Returns a typed result value

**You** write the thin translation layer between your language's runtime values
and the `push_arg` / `push_mut_arg` / `push_field` calls that `dyncall`
expects. That layer is typically 100–300 lines of Rust, written once. After
that, script authors can call any C API by name.

---

## Core types

| Type | Where | Purpose |
|------|-------|---------|
| `DynCaller::define_function(descriptor)` | `dyncall` | Parse a descriptor string; returns a `FuncDef` |
| `FuncDef` | `dyncall` | Compiled function definition. Clone it, store it, call `.prep()` on it. |
| `Invocation` | `dyncall` | Single prepared call. Push args, then call it. |
| `ArgType` | `dyncall` | Enum of the declared type of each argument |
| `ArgVal` | `dyncall` | Enum of runtime argument/return values |
| `StructValue` | `dyncall` | A flat C struct with field-by-field accessors |
| `ScriptVal` | `dyncall` | `Number(f64)` or `Str(String)` — the adapter bridge type |

Imports you'll almost always need:

```rust
use dyncall::{ArgType, ArgVal, DynCaller, FuncDef, ScriptVal, StructValue};
```

---

## Mapping your language's value model

Before writing a single line of glue code, decide how your runtime values map
to `dyncall` argument types.

### Single numeric type (BASIC, Lox, JavaScript-style)

If your language represents all numbers as `f64`:

```
f64 number  →  push_arg(&(val as i32)) / push_arg(&val) / …
              (use the declared ArgType to pick the right cast)
f64 that is a pointer  →  ArgVal::Pointer(val as usize as *mut c_void)
String  →  push_arg(&string_value)
```

With the `coerce` flag on the descriptor (see below) you can skip the per-type
match and just call `push_arg(&val_as_f64)` — dyncall does the cast internally.

### Integer stack (Forth, PostScript-style)

If your language uses `i64` on its stack and represents floats as bit-cast
integers:

```
i64 (integer semantics)  →  cast to the declared width and call push_arg
i64 (float semantics)  →  f32::from_bits(val as u32) or f64::from_bits(val as u64)
strings  →  store in a side Vec<CString>, push the index onto the stack
```

### Object / tagged-union model (Lox, Ruby-style)

If your language has a tagged-union `Value` type:

```
Value::Number(f64)   →  push_arg with declared-type cast
Value::String(s)     →  push_arg(&s) for cstr args
Value::Nil / other   →  push 0 or return Err
```

---

## Registration: def-time work

When the script author declares an external function (once, at startup or load
time), parse the descriptor and store a `FuncDef`:

```rust
// At declaration time — e.g. user wrote:
//   DEF XFN abs("msvcrt.dll|abs|i32|i32|")
fn register_external(name: &str, descriptor: &str, registry: &mut HashMap<String, FuncDef>) {
    match DynCaller::define_function(descriptor) {
        Ok(fdef) => { registry.insert(name.to_string(), fdef); }
        Err(e)   => eprintln!("bad descriptor '{}': {}", descriptor, e),
    }
}
```

`FuncDef` is `Clone + Send + Sync`, so you can store it anywhere that suits
your interpreter's architecture (a hash map keyed by function name is common).

**`define_function` validates the descriptor eagerly** — a typo in the type
string is caught here, not on the first call.

---

## Dispatch: call-time work

When the script calls an external function, retrieve its `FuncDef` and build
an `Invocation`. The pattern is always the same:

```rust
let mut inv = fdef.prep();
// push arguments ...
let result = inv.call()?;
// read result ...
```

Argument order is the order declared in the descriptor. Use
`fdef.get_arg_count()` and `fdef.get_arg_type(i)` to iterate.

### Scalar input arguments

Match on `ArgType` and push the right Rust type:

```rust
match fdef.get_arg_type(i) {
    ArgType::Char  => inv.push_arg(&(val as u8))?,
    ArgType::I16   => inv.push_arg(&(val as i16))?,
    ArgType::U16   => inv.push_arg(&(val as u16))?,
    ArgType::I32   => inv.push_arg(&(val as i32))?,
    ArgType::U32   => inv.push_arg(&(val as u32))?,
    ArgType::I64   => inv.push_arg(&(val as i64))?,
    ArgType::U64   => inv.push_arg(&(val as u64))?,
    ArgType::F32   => inv.push_arg(&(val as f32))?,
    ArgType::F64   => inv.push_arg(&val)?,
    ArgType::CString       => inv.push_arg(&string_value)?,
    ArgType::OpaquePointer => inv.push_arg(&ArgVal::Pointer(val as usize as *mut _))?,
    _ => { /* handled elsewhere */ }
}
```

If you always use the `coerce` flag (recommended for scripting languages — see
below), you can simplify to:

```rust
// With coerce flag: push_arg accepts any numeric type and converts automatically
inv.push_arg(&(val as f64))?;   // i64 or f64 value — coerce handles the rest
```

### Output (pointer) arguments

For `*i32`, `*f64`, etc. — the C function writes back through a pointer. You
must allocate a Rust value, get a mutable reference to it, and recover it after
the call:

```rust
// Allocate a boxed value so its address is stable during the call
let mut out: Box<i32> = Box::new(initial_value as i32);
inv.push_mut_arg(out.as_mut())?;

let _ = inv.call()?;

// The C function has written into *out
let written_back = *out as f64;  // or however your language uses it
```

**Common pitfall:** using a stack variable whose address can change. Always box
output values or store them in a pre-allocated `Vec`.

If the script provides an initial value for the output slot (e.g. BASIC passes
a numeric variable by reference), initialise the box with that value so the
callee sees a sensible starting state.

### Output string buffers (ocstr)

The descriptor `ocstr=256` or `ocstr=argK` pre-allocates a `char *` buffer that
the C function fills. You push a `String` (used as the initial content / sizing
hint) and read it back after the call:

```rust
// The user provides a variable to receive the string
let mut out_string = String::new();   // or the current variable value
inv.push_mut_arg(&mut out_string)?;
inv.call()?;
// out_string now contains whatever the C function wrote
write_back_to_script_variable(out_string);
```

`dyncall` pre-allocates the C buffer, makes the call, then copies the
null-terminated result back into your `String`.

### Struct arguments

Structs in dyncall descriptors look like `{i32,f64,cstr}` (by value) or
`*{i32,f64}` (pointer — written back after the call). The most adapter-friendly
path uses `ScriptVal`:

**Inspecting struct field layout:**

Use `ArgType::struct_type()` to query how many fields a struct has and what
their types are — without pattern-matching on the `Struct`/`Pointer` variants
yourself:

```rust
let arg_type = fdef.get_arg_type(i);

if let Some(st) = arg_type.struct_type() {
    // struct_type() works for both {…} and *{…} arg types
    println!("{} fields", st.field_count());
    for j in 0..st.field_count() {
        println!("  field {}: {:?}", j, st.field_type(j).unwrap());
        // field_type returns &ArgType — I32, F64, CString, etc.
    }
}

// Is the struct passed by-value (no write-back) or by-pointer (write-back)?
let is_writeback = arg_type.is_struct_ptr();   // true for *{…}
let is_by_value  = arg_type.is_struct_by_value(); // true for {…}
```

**Building a struct from script values:**

```rust
// Convert your language's array/list of values into ScriptVal
let script_vals: Vec<ScriptVal> = my_array.iter().map(|v| match v {
    MyValue::Number(n)   => ScriptVal::Number(*n as f64),
    MyValue::Str(s)      => ScriptVal::Str(s.clone()),
    _                    => ScriptVal::Number(0.0),
}).collect();

let arg_type = fdef.get_arg_type(i);   // ArgType::Struct(_) or ArgType::Pointer(_)
let mut sv = StructValue::from_script_vals(arg_type, &script_vals)?;
```

For `{...}` (by value): `inv.push_arg(&sv)?`  
For `*{...}` (pointer): `inv.push_mut_arg(&mut sv)?`

**Important:** `StructValue` must stay alive for the duration of the call. Build
all struct values *before* calling `.prep()` on the invocation, or store them in
a `Vec` that outlives the invocation.

**Reading struct fields back after a `*{...}` call or a struct return:**

```rust
// struct return: result is ArgVal::StructValue(sv)
if let ArgVal::StructValue(sv) = &result {
    for fi in 0..sv.field_count() {
        match sv.script_read(fi)? {
            ScriptVal::Number(n) => store_number(fi, n),
            ScriptVal::Str(s)    => store_string(fi, s),
        }
    }
}

// pointer-to-struct: after the call, the StructValue was mutated in place
if let Some(sv) = get_struct_slot(i) {
    for fi in 0..sv.field_count() {
        write_back_to_script_array(fi, sv.script_read(fi)?);
    }
}
```

`script_read` always returns `ScriptVal::Number(f64)` for numeric fields and
`ScriptVal::Str(String)` for `cstr` fields — no need to know the underlying C
type.

### Making the call and reading the result

```rust
let result = inv.call()?;   // or inv.call_and_return() for more control

match fdef.get_return_type() {
    ArgType::Void          => { /* no return value */ }
    ArgType::I32           => push_number(*result.as_i32()? as f64),
    ArgType::F64           => push_number(*result.as_f64()?),
    ArgType::OpaquePointer => push_pointer(*result.as_pointer()?),
    ArgType::CString       => push_string(result.as_cstr_string()?),
    ArgType::Struct(_)     => handle_struct_return(&result),
    // ... etc
}
```

The return value accessor names mirror the type tokens: `as_i32()`, `as_u64()`,
`as_f32()`, `as_f64()`, `as_pointer()`, `as_struct_value()`, …

---

## The coerce flag

Add `coerce` to the descriptor's flags field to enable automatic type coercion
when pushing arguments:

```
"msvcrt.dll|abs|i32|i32|coerce"
```

With `coerce`:
- Any numeric Rust type pushed into any numeric slot is cast automatically
- A string pushed into a numeric slot is parsed as a decimal number
- A number pushed into a `cstr` slot is formatted as a decimal string

This is almost always the right choice for scripting languages, because script
values are typically loosely typed and the user shouldn't have to think about
whether their `42.0` will fit into an `i32` slot.

Without `coerce`, `push_arg` returns `Err` if the Rust type of the value
doesn't exactly match the declared argument type.

### Global coerce default

If your language is loosely typed throughout, enable coercion once at startup
instead of appending `coerce` to every descriptor your users write:

```rust
// At interpreter startup — applies to all subsequent define_function calls.
DynCaller::set_default_coerce(true);
```

After this, every descriptor behaves as if it included `coerce`, even when the
script author omits it.  Descriptors that already include `coerce` are
unaffected (the flag is OR-d in).

```rust
// Both of these are equivalent after set_default_coerce(true):
DynCaller::define_function("msvcrt.dll|abs|i32|i32|")
DynCaller::define_function("msvcrt.dll|abs|i32|i32|coerce")
```

Query the current setting with `DynCaller::default_coerce() -> bool`.

## The errno flag

Add `errno` to capture the C errno (or `GetLastError` on Windows) immediately
after the call, before anything else can overwrite it:

```
"msvcrt.dll|fopen|cstr,cstr|ptr|errno"
```

After the call:

```rust
let result = inv.call()?;
if result.as_pointer().map(|p| p.is_null()).unwrap_or(true) {
    let err = inv.last_errno().unwrap();
    report_script_error(format!("native call failed: errno={}", err));
}
```

`last_errno()` returns `Some(code)` when the flag is set, `None` when not.

---

## Complete worked examples

The three adapter implementations below are the real code used in the
[BASIC](https://github.com/pm100/basic),
[Forth](https://github.com/pm100/forth-rs), and
[Lox](https://github.com/pm100/loxido) forks.

### BASIC interpreter (f64 value model)

BASIC stores all numbers as `f64` and all strings as `String`. Struct
arguments are passed as BASIC numeric arrays.

**Registration** (`DEF XFN` statement):

```rust
// src/dyncalls.rs
let fdef = DynCaller::define_function(descriptor_string)?;
self.external_functions.insert(function_name, fdef);
```

**Dispatch** (`FN name(args)` call):

```rust
let mut invoke = fdef.prep();

// Pre-build struct slots from BASIC arrays before pushing
let mut struct_slots: Vec<Option<StructValue>> = vec![None; arg_count];
for i in 0..arg_count {
    let arg_type = fdef.get_arg_type(i);
    if matches!(arg_type, ArgType::Struct(_) | ArgType::Pointer(_)) {
        let array = self.arrays.get(array_name_for_arg(i)).unwrap();
        let script_vals: Vec<ScriptVal> = array.data.iter().map(|v| match v {
            Value::Number(n) => ScriptVal::Number(*n),
            Value::String(s) => ScriptVal::Str(s.clone()),
            _                => ScriptVal::Number(0.0),
        }).collect();
        struct_slots[i] = Some(StructValue::from_script_vals(arg_type, &script_vals)?);
    }
}

// Push each argument in declaration order
for (i, val) in arg_values.into_iter().enumerate() {
    match fdef.get_arg_type(i) {
        ArgType::Struct(_) => invoke.push_arg(struct_slots[i].as_ref().unwrap())?,
        ArgType::Pointer(inner) if matches!(inner.as_ref(), ArgType::Struct(_)) => {
            invoke.push_mut_arg(struct_slots[i].as_mut().unwrap())?
        }
        ArgType::Pointer(_) => {
            // output numeric — pre-allocated box, address stable during call
            invoke.push_mut_arg(&mut out_num_box)?;
        }
        ArgType::OCString(_) => invoke.push_mut_arg(&mut out_string)?,
        _ => match val {
            Value::Number(n) => invoke.push_arg(&(n as i64))?,   // coerce flag handles the rest
            Value::String(s) => invoke.push_arg(&s)?,
        }
    }
}

let ret = invoke.call()?;

// Write pointer-to-struct fields back into the BASIC array
for (i, slot) in struct_slots.iter().enumerate() {
    if matches!(fdef.get_arg_type(i), ArgType::Pointer(_)) {
        if let Some(sv) = slot {
            for fi in 0..sv.field_count() {
                if let ScriptVal::Number(n) = sv.script_read(fi)? {
                    basic_array[fi] = Value::Number(n);
                }
            }
        }
    }
}
```

**Struct return** — BASIC stores a `Token::Struct` and makes field access via
array subscript syntax, e.g. `result(2)`:

```rust
ArgVal::StructValue(sv) => Token::Struct(sv.clone()),
```

---

### Forth interpreter (i64 value model)

Forth's data stack is entirely `i64`. Floats are stored as bit-cast integers.
Strings are stored in a side-pool (`Vec<CString>`) with integer indices on the
stack.

**Struct slots span multiple stack cells** — one cell per field, deepest first:

```rust
// src/ffi.rs — compute how many stack slots each argument needs
fn slot_count(arg_type: &ArgType) -> usize {
    if is_struct_arg(arg_type) {
        StructValue::new(arg_type).map(|sv| sv.field_count()).unwrap_or(0)
    } else {
        1
    }
}
```

**Dispatch:**

```rust
pub fn dispatch(func_def: &FuncDef, forth: &mut Forth) -> Result<(), Error> {
    let total_slots: usize = (0..func_def.get_arg_count())
        .map(|i| slot_count(func_def.get_arg_type(i)))
        .sum();

    // Pop everything from the stack; reverse so raw[0] = first arg
    let mut raw: Vec<i64> = (0..total_slots)
        .map(|_| forth.stack_pop())
        .collect::<Result<_,_>>()?;
    raw.reverse();

    let mut inv = func_def.prep();
    let mut offset = 0;

    // Pre-build struct values
    let mut struct_values: Vec<Option<StructValue>> = vec![None; func_def.get_arg_count()];
    for i in 0..func_def.get_arg_count() {
        let n = slot_count(func_def.get_arg_type(i));
        if is_struct_arg(func_def.get_arg_type(i)) {
            let script_vals: Vec<ScriptVal> = raw[offset..offset + n]
                .iter()
                .map(|&v| ScriptVal::Number(v as f64))
                .collect();
            struct_values[i] = Some(
                StructValue::from_script_vals(func_def.get_arg_type(i), &script_vals)?
            );
        }
        offset += n;
    }

    // Push args
    offset = 0;
    for i in 0..func_def.get_arg_count() {
        let n = slot_count(func_def.get_arg_type(i));
        let arg_type = func_def.get_arg_type(i);

        if is_struct_arg(arg_type) {
            let sv = struct_values[i].as_mut().unwrap();
            if matches!(arg_type, ArgType::Pointer(_)) {
                inv.push_mut_arg(sv)?;
            } else {
                inv.push_arg(sv)?;
            }
        } else {
            let val = raw[offset];
            match arg_type {
                ArgType::I32           => inv.push_arg(&(val as i32))?,
                ArgType::I64           => inv.push_arg(&val)?,
                ArgType::F32           => inv.push_arg(&f32::from_bits(val as u32))?,
                ArgType::F64           => inv.push_arg(&f64::from_bits(val as u64))?,
                ArgType::CString       => {
                    // val is an index into forth.strings
                    let cstr = &forth.strings[val as usize];
                    inv.push_arg(cstr.as_c_str())?;
                }
                ArgType::OpaquePointer => {
                    inv.push_arg(&ArgVal::Pointer(val as usize as *mut _))?;
                }
                _ => inv.push_arg(&val)?,
            }
        }
        offset += n;
    }

    let result = inv.call()?;

    // Struct return: push one stack cell per field
    if let ArgVal::StructValue(sv) = &result {
        for fi in 0..sv.field_count() {
            match sv.script_read(fi)? {
                ScriptVal::Number(n) => forth.stack_push(n as i64),
                ScriptVal::Str(s) => {
                    let idx = forth.strings.len();
                    forth.strings.push(CString::new(s)?);
                    forth.stack_push(idx as i64);
                }
            }
        }
        return Ok(());
    }

    // Scalar return
    match func_def.get_return_type() {
        ArgType::Void          => {}
        ArgType::I32           => forth.stack_push(*result.as_i32()? as i64),
        ArgType::F64           => forth.stack_push(f64::to_bits(*result.as_f64()?) as i64),
        ArgType::OpaquePointer => forth.stack_push(*result.as_pointer()? as usize as i64),
        _ => forth.stack_push(0),
    }
    Ok(())
}
```

---

### Lox interpreter (object value model)

Lox (Loxido) has a GC-managed `Value` enum. The dyncall integration adds a new
`ExternalData::Struct(StructValue)` variant so structs are first-class Lox objects.

**Built-in native functions exposed to scripts:**

| Function | Purpose |
|----------|---------|
| `exfun(descriptor)` | Parse descriptor → `ExternalFunction` value |
| `exstruct(descriptor)` | Allocate an empty `StructValue` → `ExternalData` |
| `exfield(obj, i)` | Read field `i` from a struct object |
| `exsetfield(obj, i, val)` | Write field `i` of a struct object |
| `exout(fn, args...)` | Call a function that returns an output (pointer) argument |
| `exstrbuf(fn, size, args...)` | Call a function that fills an output string buffer |
| `exvalue(fn, args...)` | Call a function and return its return value |

**Struct round-trip example in Lox:**

```lox
// Define the function that takes a *{i32,i32} struct
var bump = exfun("myffi.so|bump_pair|*{i32,i32}|void|");

// Allocate and populate the struct
var pair = exstruct("myffi.so|bump_pair|*{i32,i32}|void|");
exsetfield(pair, 0, 7);
exsetfield(pair, 1, 8);

// Call (pair fields are mutated in place)
bump(pair);

print exfield(pair, 0);   // → 8
print exfield(pair, 1);   // → 10
```

**Key implementation notes:**

1. `exstruct` scans the descriptor for the first `Struct` or `Pointer` arg type
   to infer the layout — no separate type declaration needed.

2. `exsetfield` rebuilds the struct from scratch using `from_script_vals` because
   `StructValue` doesn't support in-place field mutation. Read all current
   values, overwrite the one at `idx`, rebuild:

```rust
let mut fields: Vec<ScriptVal> = (0..field_count)
    .map(|fi| sv.script_read(fi))
    .collect::<Result<_,_>>()?;
fields[idx] = new_val;
sv.reset();
for field in &fields {
    match field {
        ScriptVal::Number(n) => sv.push_field_coerced(n)?,
        ScriptVal::Str(_)    => sv.push_field_coerced(&0.0f64)?,  // cstr: write null
    }
}
```

3. GC safety: capture all `ScriptVal` data from the GC before any mutable GC
   access (`deref_mut`), because Lox's GC does not support aliased borrows.

---

## Summary checklist

Use this list when writing a new adapter.

### One-time setup

- [ ] Add `dyncall = "0.1"` to `Cargo.toml`
- [ ] Decide where to store `FuncDef` values (a `HashMap<String, FuncDef>` is typical)
- [ ] Decide your value-model mapping (see [Mapping section](#mapping-your-languages-value-model))
- [ ] Add a script-level syntax for declaring external functions (e.g. `DEF XFN`, `extern:`, `exfun`)

### Per-argument marshaling

- [ ] Scalar inputs: match `ArgType`, cast to the right Rust primitive, call `push_arg`
- [ ] Pointer outputs (`*i32`, etc.): box the value, call `push_mut_arg`, read back after call
- [ ] String inputs (`cstr`): call `push_arg(&string)`
- [ ] String outputs (`ocstr`): call `push_mut_arg(&mut string)`, read back after call
- [ ] Struct inputs (`{...}`): build `StructValue` via `from_script_vals`, call `push_arg`
- [ ] Struct pointer inputs (`*{...}`): build `StructValue`, call `push_mut_arg`, write fields back
- [ ] Consider using the `coerce` flag to simplify numeric marshaling

### Return value handling

- [ ] Scalar: use `result.as_i32()`, `as_f64()`, `as_pointer()`, etc.
- [ ] Struct return: check `ArgVal::StructValue(sv)`, iterate with `sv.script_read(fi)`
- [ ] Void: no return value to push

### Robustness

- [ ] Propagate `define_function` errors to the script (at def time)
- [ ] Propagate `push_arg` / `call` errors to the script (at call time)
- [ ] For error-reporting C functions, add the `errno` flag and check `inv.last_errno()` after the call
- [ ] Verify the adapter with real C library calls (atoi, strlen, sscanf, localeconv)
