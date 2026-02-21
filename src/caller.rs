use std::ffi::{self, c_void, CStr, CString};
use std::mem;
use std::path::Path;

use std::sync::{LazyLock, Mutex};
use std::{collections::HashMap, ptr};

use anyhow::Result;
use anyhow::{anyhow, bail};
use enum_as_inner::EnumAsInner;

use libc::strlen;
use libffi::low::*;

use libffi::raw::{
    ffi_abi_FFI_DEFAULT_ABI, ffi_call, FFI_TYPE_DOUBLE, FFI_TYPE_FLOAT, FFI_TYPE_POINTER,
    FFI_TYPE_SINT16, FFI_TYPE_SINT32, FFI_TYPE_SINT64, FFI_TYPE_SINT8, FFI_TYPE_UINT16,
    FFI_TYPE_UINT32, FFI_TYPE_UINT64, FFI_TYPE_UINT8,
};

use crate::args::{ArgType, ArgVal, LengthDef, ToArg, ToMutArg};
use crate::dylib::DynamicLibrary;
use crate::invoke::Invocation;
static DYNCALLER: LazyLock<Mutex<DynCaller>> = LazyLock::new(|| Mutex::new(DynCaller::new()));
//static GLOBAL_DATA: Mutex<DynCallerData> = Mutex::new();
pub struct DynCaller {
    libs: HashMap<String, DynamicLibrary>,
    //entry_points: HashMap<String, *mut ffi::c_void>,
    //cifs: HashMap<String, ffi_cif>,
    // funcs: HashMap<FunctionId, FuncDef>,
}

#[derive(Clone)]
pub struct FuncDef {
    pub(crate) cif: ffi_cif,
    pub(crate) entry_point: unsafe extern "C" fn(),
    pub(crate) ffi_arg_types: Vec<*mut ffi_type>,
    pub(crate) ffi_return_type: ffi_type,
    pub(crate) arg_types: Vec<ArgType>,
    pub(crate) return_type: ArgType,
    // pub(crate) arg_ptrs: Vec<*mut c_void>,
    // pub(crate) arg_vals: Vec<ArgVal>,
    //val_offsets: Vec<u8>,
}
impl FuncDef {
    pub fn prep(&self) -> Invocation {
        Invocation {
            func_def: self,
            arg_ptrs: Vec::with_capacity(self.arg_types.len()),
            arg_vals: Vec::with_capacity(self.arg_types.len() * 4),
        }
    }
    pub fn get_arg_count(&self) -> usize {
        self.arg_types.len()
    }
    pub fn get_arg_type(&self, index: usize) -> &ArgType {
        &self.arg_types[index]
    }
    pub fn get_return_type(&self) -> &ArgType {
        &self.return_type
    }
}

struct Flags {
    vararg: bool,
    fixed_args: u8,
}
impl DynCaller {
    fn new() -> Self {
        DynCaller {
            libs: HashMap::new(),
        }
    }

    fn get_lib(&mut self, lib_name: &str) -> Result<DynamicLibrary> {
        if self.libs.contains_key(lib_name) {
            return Ok(self.libs.get(lib_name).unwrap().clone());
        }

        let lib = DynamicLibrary::open(Some(Path::new(lib_name)))?;

        self.libs.insert(lib_name.to_string(), lib);
        // // return Ok(lib);
        return Ok(self.libs.get(lib_name).unwrap().clone());
    }

    fn get_entry_point(
        &mut self,
        lib_name: &str,
        entry_point_name: &str,
    ) -> Result<*mut ffi::c_void> {
        let lib = self.get_lib(lib_name)?;
        let ep = unsafe { lib.symbol(entry_point_name)? };
        Ok(ep)
    }
    // fn internal_type_to_type(arg_type: *mut ffi_type) -> ArgType {
    //     unsafe {
    //         match (*arg_type).type_ as u32 {
    //             FFI_TYPE_POINTER => ArgType::Pointer,
    //             FFI_TYPE_UINT64 => ArgType::U64,
    //             FFI_TYPE_SINT64 => ArgType::I64,
    //             FFI_TYPE_UINT32 => ArgType::U32,
    //             FFI_TYPE_SINT32 => ArgType::I32,
    //             FFI_TYPE_SINT16 => ArgType::I16,
    //             FFI_TYPE_UINT16 => ArgType::U16,
    //             FFI_TYPE_UINT8 => ArgType::Char,
    //             FFI_TYPE_SINT8 => ArgType::Char,
    //             FFI_TYPE_FLOAT => ArgType::F32,
    //             FFI_TYPE_DOUBLE => ArgType::F64,
    //             _ => panic!("Unsupported  type"),
    //         }
    //     }
    // }
    pub fn define_function_by_str(funcdef: &str) -> Result<FuncDef> {
        //pub fn define_function_by_str(&mut self, funcdef: &str) -> Result<FuncDef<'_>> {
        // TODO add conditional dll defs
        //let  dyncaller = &mut DYNCALLER;
        let funcdef = funcdef.split("|").collect::<Vec<&str>>();
        if funcdef.len() != 5 {
            bail!("Invalid function definition format. Expected 'lib_name|entry_point_name|arg1,arg2,arg3|return_type|flags'");
        };
        let lib_name = funcdef[0];
        let entry_point_name = funcdef[1];
        let args_str = funcdef[2];
        //let args = arg_gen(args_str);
        let mut my_arg_types = Vec::new();
        let mut ffi_arg_types = Vec::new();
        if args_str.len() > 0 {
            for a in args_str.split(',') {
                let (ffi_type, my_type) = type_gen(a);
                ffi_arg_types.push(ffi_type);
                my_arg_types.push(my_type);
            }
        }
        let return_type_str = funcdef[3];
        let (ffi_ret, my_ret) = type_gen(return_type_str);
        let arg_count = my_arg_types.len();
        let ep = DYNCALLER
            .lock()
            .unwrap()
            .get_entry_point(lib_name, entry_point_name)?;
        let entry_point = unsafe { std::mem::transmute(ep) };
        let flag_str = funcdef[4];
        let mut func = FuncDef {
            cif: ffi_cif::default(),
            entry_point,
            ffi_arg_types: ffi_arg_types, //.clone(),
            ffi_return_type: unsafe { *ffi_ret },
            arg_types: my_arg_types,
            return_type: my_ret,
            //  arg_vals: Vec::with_capacity(arg_count * 4), // worst case guess
            //  arg_ptrs: Vec::with_capacity(arg_count),
        };
        let flags = Self::parse_flags(flag_str)?;
        //  func.arg_vals.resize(arg_count, ArgVal::None);
        unsafe {
            if flags.vararg {
                prep_cif_var(
                    &mut func.cif,
                    ffi_abi_FFI_DEFAULT_ABI,
                    flags.fixed_args as usize,
                    arg_count as usize,
                    &mut func.ffi_return_type,
                    func.ffi_arg_types.as_ptr() as *mut *mut ffi_type,
                )
                .map_err(|e| anyhow!(format!("{:?}", e)))?;
            } else {
                prep_cif(
                    &mut func.cif,
                    ffi_abi_FFI_DEFAULT_ABI,
                    arg_count as usize,
                    &mut func.ffi_return_type,
                    func.ffi_arg_types.as_ptr() as *mut *mut ffi_type,
                )
                .map_err(|e| anyhow!(format!("{:?}", e)))?;
            }
        };

        Ok(func)
    }
    fn parse_flags(flag_str: &str) -> Result<Flags> {
        let mut flags = Flags {
            vararg: false,
            fixed_args: 0,
        };
        for flag in flag_str.split(',') {
            if let Some(fixed_count) = flag.strip_prefix("vararg=") {
                flags.vararg = true;
                flags.fixed_args = fixed_count.parse::<u8>()?;
            }
        }
        Ok(flags)
    }
    //     pub fn call<T>(&mut self, func_def: &FuncDef, args: &mut Vec<*mut c_void>) -> Result<T>
    //     where
    //         T: Default,
    //     {
    //         //let le = unsafe { GetLastError() };
    //         // let mut cif = self.get_cif(lib_name, entry_point_name)?;
    //         // let entry_point = self.get_entry_point(lib_name, entry_point_name)?;
    //         //let func_def = self.funcs.get(&id).ok_or(anyhow!("not found"))?;
    //         let mut cif = func_def.cif;
    //         let mut result = mem::MaybeUninit::<T>::uninit();
    //         // let mut args = vec![&mut 99u32 as *mut _ as *mut c_void];
    //         let ep = unsafe { std::mem::transmute(func_def.entry_point) };
    //         // unsafe {
    //         //     SetLastError(le);
    //         // }
    //         unsafe {
    //             ffi_call(
    //                 &mut cif,
    //                 Some(ep),
    //                 result.as_mut_ptr() as *mut c_void,
    //                 args.as_mut_ptr(),
    //             );
    //         }

    //         Ok(unsafe { result.assume_init() })
    //     }
    // }

    // pub struct Args {
    //     argsdef: Vec<*mut c_void>,
    // }

    // impl Args {
    //     pub fn new() -> Self {
    //         Args { argsdef: vec![] }
    //     }
    //     pub fn push<T>(&mut self, value: &T) {
    //         unsafe {
    //             self.argsdef
    //                 .push(mem::transmute::<*const T, *mut c_void>(value));
    //         }
    //     }
    // }
    // fn arg_gen(args: &str) -> Vec<*mut ffi_type> {
    //     let mut argsdef: Vec<*mut ffi_type> = vec![];
    //     if args.len() > 0 {
    //         for a in args.split(',') {
    //             argsdef.push(type_gen(a));
    //         }
    //     }
    //     argsdef
}
fn parse_def(def: &str) -> (&str, Option<&str>) {
    // parse type definition like "xxxx[=yy]"

    let parts = def.split('=').collect::<Vec<&str>>();
    if parts.len() == 2 {
        return (parts[0], Some(parts[1]));
    }
    (parts[0], None)
}
fn type_gen(at: &str) -> (*mut ffi_type, ArgType) {
    match at.strip_prefix('*') {
        Some(rest) => {
            let (base_type, my_type) = type_gen(rest);
            return (&raw mut types::pointer, ArgType::Pointer(Box::new(my_type)));
        }
        None => { /* continue */ }
    }
    let (parsed_arg, qualifier) = parse_def(at);
    match parsed_arg.trim() {
        "u8" => (&raw mut types::uint8, ArgType::Char),
        "i8" => (&raw mut types::sint8, ArgType::Char),
        "u16" => (&raw mut types::uint16, ArgType::U16),
        "i16" => (&raw mut types::sint16, ArgType::I16),
        "u32" => (&raw mut types::uint32, ArgType::U32),
        "i32" => (&raw mut types::sint32, ArgType::I32),
        "u64" => (&raw mut types::uint64, ArgType::U64),
        "i64" => (&raw mut types::sint64, ArgType::I64),
        "f32" => (&raw mut types::float, ArgType::F32),
        "f64" => (&raw mut types::double, ArgType::F64),
        "ptr" => (&raw mut types::pointer, ArgType::OpaquePointer),
        "cstr" => (&raw mut types::pointer, ArgType::CString),
        "ocstr" => {
            let ldef = if let Some(qual) = qualifier {
                if qual.starts_with("arg") {
                    let arg_idx: u8 = qual[3..].parse().unwrap();
                    LengthDef::Arg(arg_idx)
                } else {
                    let fixed_len: usize = qual.parse().unwrap();
                    LengthDef::Fixed(fixed_len)
                }
            } else {
                LengthDef::None
            };
            (&raw mut types::pointer, ArgType::OCString(ldef))
        }
        "obuff" => {
            let ldef = if let Some(qual) = qualifier {
                if qual.starts_with("arg") {
                    let arg_idx: u8 = qual[3..].parse().unwrap();
                    LengthDef::Arg(arg_idx)
                } else {
                    let fixed_len: usize = qual.parse().unwrap();
                    LengthDef::Fixed(fixed_len)
                }
            } else {
                LengthDef::None
            };
            (&raw mut types::pointer, ArgType::OByteBuffer(ldef))
        }
        "buff" => (&raw mut types::pointer, ArgType::ByteBuffer),
        "void" => (&raw mut types::void, ArgType::Void),
        _ => panic!("unknown type {}", at),
    }
}
