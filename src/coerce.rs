use std::ffi::{c_void, CString};

use anyhow::{bail, Result};

use crate::args::{ArgType, ArgVal};
use crate::invoke::Invocation;

/// Push a scalar ArgVal pair and return its payload pointer.
fn push_pair(func: &mut Invocation, val: ArgVal) -> Result<*mut c_void> {
    func.arg_vals.push(val.clone());
    func.arg_vals.push(val);
    let pp = &func.arg_vals[func.arg_vals.len() - 1];
    Ok(pp.payload_ptr())
}

/// Coerce an integer value to the declared argument type and push it.
pub fn push_coerced_int(
    func: &mut Invocation,
    val: i64,
    declared: &ArgType,
) -> Result<*mut c_void> {
    match declared {
        ArgType::Char => push_pair(func, ArgVal::Char(val as u8)),
        ArgType::I16 => push_pair(func, ArgVal::I16(val as i16)),
        ArgType::U16 => push_pair(func, ArgVal::U16(val as u16)),
        ArgType::I32 => push_pair(func, ArgVal::I32(val as i32)),
        ArgType::U32 => push_pair(func, ArgVal::U32(val as u32)),
        ArgType::I64 => push_pair(func, ArgVal::I64(val)),
        ArgType::U64 => push_pair(func, ArgVal::U64(val as u64)),
        ArgType::F32 => push_pair(func, ArgVal::F32(val as f32)),
        ArgType::F64 => push_pair(func, ArgVal::F64(val as f64)),
        ArgType::OpaquePointer | ArgType::Pointer(_) => {
            let p = val as usize as *mut c_void;
            push_pair(func, ArgVal::Pointer(p))
        }
        ArgType::CString | ArgType::OCString(_) => {
            // integer → string: format as decimal
            let s = CString::new(val.to_string()).unwrap();
            let p = s.as_ptr() as *mut c_void;
            func.arg_vals.push(ArgVal::Pointer(p));
            func.arg_vals.push(ArgVal::CString(s));
            let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
            Ok(pbuff.payload_ptr())
        }
        other => bail!("Cannot coerce integer to {:?}", other),
    }
}

/// Coerce a float value to the declared argument type and push it.
pub fn push_coerced_float(
    func: &mut Invocation,
    val: f64,
    declared: &ArgType,
) -> Result<*mut c_void> {
    match declared {
        ArgType::Char => push_pair(func, ArgVal::Char(val as u8)),
        ArgType::I16 => push_pair(func, ArgVal::I16(val as i16)),
        ArgType::U16 => push_pair(func, ArgVal::U16(val as u16)),
        ArgType::I32 => push_pair(func, ArgVal::I32(val as i32)),
        ArgType::U32 => push_pair(func, ArgVal::U32(val as u32)),
        ArgType::I64 => push_pair(func, ArgVal::I64(val as i64)),
        ArgType::U64 => push_pair(func, ArgVal::U64(val as u64)),
        ArgType::F32 => push_pair(func, ArgVal::F32(val as f32)),
        ArgType::F64 => push_pair(func, ArgVal::F64(val)),
        other => bail!("Cannot coerce float to {:?}", other),
    }
}

/// Coerce a CString to the declared argument type and push it.
pub fn push_coerced_str(
    func: &mut Invocation,
    val: CString,
    declared: &ArgType,
) -> Result<*mut c_void> {
    match declared {
        ArgType::CString | ArgType::OCString(_) => {
            let p = val.as_ptr() as *mut c_void;
            func.arg_vals.push(ArgVal::Pointer(p));
            func.arg_vals.push(ArgVal::CString(val));
            let pbuff = &func.arg_vals[func.arg_vals.len() - 2];
            Ok(pbuff.payload_ptr())
        }
        ArgType::Char => {
            let b = val.as_bytes().first().copied().unwrap_or(0);
            push_pair(func, ArgVal::Char(b))
        }
        ArgType::I16 | ArgType::I32 | ArgType::I64 => {
            let s = val.to_str().map_err(|e| anyhow::anyhow!(e))?;
            let n: i64 = s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("cannot parse {:?} as integer", s))?;
            push_coerced_int(func, n, declared)
        }
        ArgType::U16 | ArgType::U32 | ArgType::U64 => {
            let s = val.to_str().map_err(|e| anyhow::anyhow!(e))?;
            let n: u64 = s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("cannot parse {:?} as unsigned integer", s))?;
            push_coerced_int(func, n as i64, declared)
        }
        ArgType::F32 | ArgType::F64 => {
            let s = val.to_str().map_err(|e| anyhow::anyhow!(e))?;
            let f: f64 = s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("cannot parse {:?} as float", s))?;
            push_coerced_float(func, f, declared)
        }
        other => bail!("Cannot coerce string to {:?}", other),
    }
}
