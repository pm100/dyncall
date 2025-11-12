use std::ffi::{self, c_void, CStr, CString};
use std::mem;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use std::{collections::HashMap, ptr};

use anyhow::Result;
use anyhow::{anyhow, bail};
use enum_as_inner::EnumAsInner;

use libffi::low::*;

use libffi::raw::{
    ffi_abi_FFI_DEFAULT_ABI, ffi_call, FFI_TYPE_DOUBLE, FFI_TYPE_FLOAT, FFI_TYPE_POINTER,
    FFI_TYPE_SINT16, FFI_TYPE_SINT32, FFI_TYPE_SINT64, FFI_TYPE_SINT8, FFI_TYPE_UINT16,
    FFI_TYPE_UINT32, FFI_TYPE_UINT64, FFI_TYPE_UINT8,
};

use crate::dylib::DynamicLibrary;
//static DYNCALLER: LazyLock<DynCaller> = LazyLock::new(|| DynCaller::new());
//static GLOBAL_DATA: Mutex<DynCallerData> = Mutex::new();
pub struct DynCaller {
    libs: HashMap<String, DynamicLibrary>,
    //entry_points: HashMap<String, *mut ffi::c_void>,
    //cifs: HashMap<String, ffi_cif>,
    // funcs: HashMap<FunctionId, FuncDef>,
}
#[derive(EnumAsInner, Clone, Debug)]
pub enum ArgVal {
    Pointer(*mut c_void),
    U64(u64),
    F64(f64),
    I64(i64),
    I32(i32),
    U32(u32),
    I16(i16),
    U16(u16),
    F32(f32),
    Char(u8),
}
impl ArgVal {
    fn payload_ptr(&self) -> *mut c_void {
        use ArgVal::*;
        match self {
            Pointer(ref val) => val as *const _ as *mut c_void,
            U64(ref val) => val as *const _ as *mut c_void,
            I32(ref val) => val as *const _ as *mut c_void,
            U32(ref val) => val as *const _ as *mut c_void,
            I16(ref val) => val as *const _ as *mut c_void,
            U16(ref val) => val as *const _ as *mut c_void,
            F32(ref val) => val as *const _ as *mut c_void,
            F64(ref val) => val as *const _ as *mut c_void,
            I64(ref val) => val as *const _ as *mut c_void,
            Char(ref val) => val as *const _ as *mut c_void,
            // _ => panic!("Unsupported ArgVal variant for payload_ptr"),
            // ...
        }
    }
}

#[derive(Clone)]
pub struct FuncDef {
    cif: ffi_cif,
    entry_point: unsafe extern "C" fn(),
    arg_types: Vec<*mut ffi_type>,
    return_type: ffi_type,
    arg_ptrs: Vec<*mut c_void>,
    arg_vals: Vec<ArgVal>,
}

pub trait ToArg {
    fn to_arg(&self, func: &mut FuncDef) -> *mut c_void;
}

impl ToArg for CString {
    fn to_arg(&self, func: &mut FuncDef) -> *mut c_void {
        let ptr = self.as_ptr();
        let p = unsafe { mem::transmute::<*const i8, *mut c_void>(ptr) };

        func.arg_vals.push(ArgVal::Pointer(p));
        let penum = &func.arg_vals[func.arg_vals.len() - 1];

        penum.payload_ptr()
    }
}

impl ToArg for CStr {
    fn to_arg(&self, func: &mut FuncDef) -> *mut c_void {
        let ptr = self.as_ptr();
        println!("CStr to_arg: {:x}", ptr as u64);
        let p = unsafe { mem::transmute::<*const i8, *mut c_void>(ptr) };

        func.arg_vals.push(ArgVal::Pointer(p));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        println!("CStr to_arg ArgVal ptr: {:p}", pp);
        let ppp = if let ArgVal::Pointer(ref p) = pp {
            p
        } else {
            panic!("Expected Pointer ArgVal")
        };
        println!("CStr to_arg final ptr: {:p}", ppp);
        unsafe { mem::transmute::<&*mut c_void, *mut c_void>(ppp) }
    }
}

impl ToArg for u64 {
    fn to_arg(&self, func: &mut FuncDef) -> *mut c_void {
        func.arg_vals.push(ArgVal::U64(*self));
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}
impl ToArg for ArgVal {
    fn to_arg(&self, func: &mut FuncDef) -> *mut c_void {
        func.arg_vals.push(self.clone());
        let pp = &func.arg_vals[func.arg_vals.len() - 1];
        pp.payload_ptr()
    }
}

impl FuncDef {
    pub fn push_arg<T>(&mut self, value: &T)
    where
        T: ToArg + ?Sized,
    {
        let argp = value.to_arg(self);
        self.arg_ptrs.push(argp);
    }
    pub fn call<T>(&mut self) -> Result<T> {
        let mut cif = self.cif;
        let mut result = mem::MaybeUninit::<T>::uninit();

        unsafe {
            ffi_call(
                &mut cif,
                Some(self.entry_point),
                result.as_mut_ptr() as *mut c_void,
                self.arg_ptrs.as_mut_ptr(),
            );
        }

        Ok(unsafe { result.assume_init() })
    }
    pub fn call2(&mut self) -> ArgVal {
        let mut cif = self.cif;

        let result = match self.return_type.type_ as u32 {
            FFI_TYPE_POINTER => ArgVal::Pointer(ptr::null_mut()),
            FFI_TYPE_UINT64 => ArgVal::U64(0),
            FFI_TYPE_SINT64 => ArgVal::I64(0),
            FFI_TYPE_UINT32 => ArgVal::U32(0),
            FFI_TYPE_SINT32 => ArgVal::I32(0),
            FFI_TYPE_SINT16 => ArgVal::I16(0),
            FFI_TYPE_UINT16 => ArgVal::U16(0),
            FFI_TYPE_UINT8 => ArgVal::Char(0),
            FFI_TYPE_SINT8 => ArgVal::Char(0),
            FFI_TYPE_FLOAT => ArgVal::F32(0.0),
            FFI_TYPE_DOUBLE => ArgVal::F64(0.0),
            _ => panic!("Unsupported return type"),
        };

        let addr = result.payload_ptr();
        unsafe {
            ffi_call(
                &mut cif,
                Some(self.entry_point),
                addr, //as *mut c_void,
                self.arg_ptrs.as_mut_ptr(),
            );
        }
        self.arg_ptrs.clear();
        self.arg_vals.clear();
        result
    }
}

impl DynCaller {
    pub fn new() -> Self {
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

    pub fn define_function_by_str(&mut self, funcdef: &str) -> Result<FuncDef> {
        let funcdef = funcdef.split("|").collect::<Vec<&str>>();
        if funcdef.len() != 4 {
            bail!("Invalid function definition format. Expected 'lib_name|entry_point_name|arg1,arg2,arg3|return_type'");
        };
        let lib_name = funcdef[0];
        let entry_point_name = funcdef[1];
        let args_str = funcdef[2];
        let args = arg_gen(args_str);
        let return_type_str = funcdef[3];
        let return_type = unsafe { *type_gen(return_type_str) };

        let ep = self.get_entry_point(lib_name, entry_point_name)?;
        let entry_point = unsafe { std::mem::transmute(ep) };

        let mut func = FuncDef {
            cif: ffi_cif::default(),
            entry_point,
            arg_types: args.clone(),
            return_type: return_type.clone(),
            arg_vals: Vec::with_capacity(args.len()),

            arg_ptrs: Vec::with_capacity(args.len()),
        };

        unsafe {
            prep_cif(
                &mut func.cif,
                ffi_abi_FFI_DEFAULT_ABI,
                args.len() as usize,
                &mut func.return_type,
                func.arg_types.as_ptr() as *mut *mut ffi_type,
            )
            .map_err(|e| anyhow!(format!("{:?}", e)))?;
        };

        Ok(func)
    }

    pub fn call<T>(&mut self, func_def: &FuncDef, args: &mut Vec<*mut c_void>) -> Result<T>
    where
        T: Default,
    {
        //let le = unsafe { GetLastError() };
        // let mut cif = self.get_cif(lib_name, entry_point_name)?;
        // let entry_point = self.get_entry_point(lib_name, entry_point_name)?;
        //let func_def = self.funcs.get(&id).ok_or(anyhow!("not found"))?;
        let mut cif = func_def.cif;
        let mut result = mem::MaybeUninit::<T>::uninit();
        // let mut args = vec![&mut 99u32 as *mut _ as *mut c_void];
        let ep = unsafe { std::mem::transmute(func_def.entry_point) };
        // unsafe {
        //     SetLastError(le);
        // }
        unsafe {
            ffi_call(
                &mut cif,
                Some(ep),
                result.as_mut_ptr() as *mut c_void,
                args.as_mut_ptr(),
            );
        }

        Ok(unsafe { result.assume_init() })
    }
}

pub struct Args {
    argsdef: Vec<*mut c_void>,
}

impl Args {
    pub fn new() -> Self {
        Args { argsdef: vec![] }
    }
    pub fn push<T>(&mut self, value: &T) {
        unsafe {
            self.argsdef
                .push(mem::transmute::<*const T, *mut c_void>(value));
        }
    }
}
fn arg_gen(args: &str) -> Vec<*mut ffi_type> {
    let mut argsdef: Vec<*mut ffi_type> = vec![];
    for a in args.split(',') {
        argsdef.push(type_gen(a));
    }
    argsdef
}
fn type_gen(at: &str) -> *mut ffi_type {
    match at.trim() {
        "u8" | "char" => &raw mut types::uint8,
        "i8" => &raw mut types::sint8,
        "u16" => &raw mut types::uint16,
        "i16" => &raw mut types::sint16,
        "u32" => &raw mut types::uint32,
        "i32" => &raw mut types::sint32,
        "u64" => &raw mut types::uint64,
        "i64" => &raw mut types::sint64,
        "f32" => &raw mut types::float,
        "f64" => &raw mut types::double,
        "ptr" | "*" => &raw mut types::pointer,
        _ => panic!("unknown type {}", at),
    }
}

#[test]
fn test_caller() {
    let mut dyncaller = DynCaller::new();
    let mut fid = dyncaller
        .define_function_by_str("msvcrt.dll|puts|ptr|i32")
        .unwrap();
    let str = CString::new("hello").unwrap();
    //  str.to_call_arg(&mut fid);
    fid.push_arg(&str);
    println!("puts ret={}", fid.call::<u64>().unwrap());
    let namex = CString::new("test.txt").unwrap();
    let mode = CString::new("w").unwrap();
    let mut fopen = dyncaller
        .define_function_by_str("msvcrt.dll|fopen|ptr,ptr|u64")
        .unwrap();
    fopen.push_arg(&namex);
    fopen.push_arg(&mode);
    let ret = fopen.call2();
    println!("fopen ret={:?}", ret);
    let mut fputs = dyncaller
        .define_function_by_str("msvcrt.dll|fputs|ptr,ptr|u64")
        .unwrap();
    let text = c"Hello from Rust fputs!\n";
    fputs.push_arg(text);
    fputs.push_arg(&ret);
    let ret2 = fputs.call::<u64>().unwrap();
}
#[test]

fn test_atoi() {
    let mut dyncaller = DynCaller::new();
    let mut atoi = dyncaller
        .define_function_by_str("msvcrt.dll|atoi|ptr|i32")
        .unwrap();
    let str = CString::new("12345").unwrap();
    atoi.push_arg(&str);
    let ret = atoi.call2();
    println!("atoi ret={:?}", ret);
    assert_eq!(*ret.as_i32().unwrap(), 12345);
}
#[test]
fn test_caller_str() {
    let mut dyncaller = DynCaller::new();
    let fopen = dyncaller
        .define_function_by_str("msvcrt.dll|fopen|ptr,ptr|u64")
        .unwrap();

    let fputs = dyncaller
        .define_function_by_str("msvcrt.dll|fputs|ptr,ptr|u64")
        .unwrap();

    //let mut m = Marshaller::new();
    let name = c"test.txt2"; //.to_string();
    let mode = c"w";
    let pmode = mode.as_ptr(); // as *mut c_void;
    let pname = name.as_ptr(); // as *mut c_void;
    let _rp = &raw const pname;

    let mut pvec: Vec<*mut c_void> = Vec::new();
    pvec.push(unsafe { mem::transmute::<*const *const i8, *mut c_void>(&pname) });
    //  pvec.push(name.to_call_arg());
    pvec.push(unsafe { mem::transmute::<*const *const i8, *mut c_void>(&pmode) });
    println!("pvec[0]={:x}", pvec[0] as u64);
    println!("pvec[1]={:x}", pvec[1] as u64);
    // let (buf, mut args) = m.build_buffer();
    let ret = dyncaller.call::<u64>(&fopen, &mut pvec).unwrap();
    println!("fopen ret={:x}", ret);

    let text = c"Hello from Rust fputs!\n";
    let ptext = text.as_ptr(); // as *mut c_void;
    let mut pvec2: Vec<*mut c_void> = Vec::new();

    pvec2.push(unsafe { mem::transmute::<*const *const i8, *mut c_void>(&ptext) });
    pvec2.push(unsafe { mem::transmute::<*const u64, *mut c_void>(&ret) });
    let ret2 = dyncaller.call::<u64>(&fputs, &mut pvec2).unwrap();
    println!("fputs ret={:x}", ret2);
}
