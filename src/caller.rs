use std::ffi::{self, c_void, CStr, CString};
use std::mem;
use std::path::Path;

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

use crate::args::{type_gen, ArgType, ArgVal, ToArg};
use crate::dylib::DynamicLibrary;
//static DYNCALLER: LazyLock<DynCaller> = LazyLock::new(|| DynCaller::new());
//static GLOBAL_DATA: Mutex<DynCallerData> = Mutex::new();
pub struct DynCaller {
    libs: HashMap<String, DynamicLibrary>,
    //entry_points: HashMap<String, *mut ffi::c_void>,
    //cifs: HashMap<String, ffi_cif>,
    // funcs: HashMap<FunctionId, FuncDef>,
}

#[derive(Clone)]
pub struct FuncDef {
    cif: ffi_cif,
    entry_point: unsafe extern "C" fn(),
    ffi_arg_types: Vec<*mut ffi_type>,
    ffi_return_type: ffi_type,
    pub(crate) arg_types: Vec<ArgType>,
    return_type: ArgType,
    pub(crate) arg_ptrs: Vec<*mut c_void>,
    pub(crate) arg_vals: Vec<ArgVal>,
}

impl FuncDef {
    pub fn push_arg<T>(&mut self, value: &T)
    where
        T: ToArg + ?Sized,
    {
        let argp = value.to_arg(self);
        self.arg_ptrs.push(argp);
    }
    pub fn push_mut_arg<T>(&mut self, value: &mut T)
    where
        T: ToArg + ?Sized,
    {
        let argp = value.to_arg(self);
        self.arg_ptrs.push(argp);
    }
    // pub fn call<T>(&mut self) -> Result<T> {
    //     let mut cif = self.cif;
    //     let mut result = mem::MaybeUninit::<T>::uninit();

    //     unsafe {
    //         ffi_call(
    //             &mut cif,
    //             Some(self.entry_point),
    //             result.as_mut_ptr() as *mut c_void,
    //             self.arg_ptrs.as_mut_ptr(),
    //         );
    //     }

    //     Ok(unsafe { result.assume_init() })
    // }
    pub fn call2(&mut self) -> ArgVal {
        let mut cif = self.cif;

        let result = match self.ffi_return_type.type_ as u32 {
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
    pub fn define_function_by_str(&mut self, funcdef: &str) -> Result<FuncDef> {
        // TODO add conditional dll defs
        let funcdef = funcdef.split("|").collect::<Vec<&str>>();
        if funcdef.len() != 4 {
            bail!("Invalid function definition format. Expected 'lib_name|entry_point_name|arg1,arg2,arg3|return_type'");
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
        let ep = self.get_entry_point(lib_name, entry_point_name)?;
        let entry_point = unsafe { std::mem::transmute(ep) };

        let mut func = FuncDef {
            cif: ffi_cif::default(),
            entry_point,
            ffi_arg_types: ffi_arg_types, //.clone(),
            ffi_return_type: unsafe { *ffi_ret },
            arg_types: my_arg_types,
            return_type: my_ret,
            arg_vals: Vec::with_capacity(arg_count),
            arg_ptrs: Vec::with_capacity(arg_count),
        };
        func.arg_vals.resize(arg_count, ArgVal::None);
        unsafe {
            prep_cif(
                &mut func.cif,
                ffi_abi_FFI_DEFAULT_ABI,
                arg_count as usize,
                &mut func.ffi_return_type,
                func.ffi_arg_types.as_ptr() as *mut *mut ffi_type,
            )
            .map_err(|e| anyhow!(format!("{:?}", e)))?;
        };

        Ok(func)
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
