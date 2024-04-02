use libffi::low::*;
use libloading::os::windows::Library;
use std::{
    ffi::{self, c_int, c_uint, c_void},
    io::stdin,
    mem,
    ops::Deref,
    path::Path,
    vec,
};

use crate::{caller::DynCaller, dylib::DynamicLibrary, marshal::Marshaller};
mod caller;
mod dylib;
mod marshal;
fn main() {
    let mut m = Marshaller::new();
    m.push(1u32);
    m.push(42u32);
    let (buf, mut ptr) = m.build_buffer();
    let mut d = DynCaller::new();
    let argsdef = vec![];
    unsafe {
        d.setup_call("kernel32.dll", "GetLastError", types::uint32, &argsdef)
            .unwrap()
    };

    let dyl = DynamicLibrary::open(Some(Path::new("kernel32.dll"))).unwrap();
    let glep: extern "C" fn() -> libc::c_uint = unsafe {
        std::mem::transmute::<*mut c_void, extern "C" fn() -> libc::c_uint>(
            dyl.symbol("GetLastError").unwrap(),
        )
    };
    glep();
    println!("Hello, world!");
    stdin().read_line(&mut String::new()).unwrap();
    unsafe {
        extern "C" fn c_function(a: u32, b: u32) -> u32 {
            a + b
        }

        let result = unsafe {
            let mut args: Vec<*mut ffi_type> = vec![&mut types::uint32, &mut types::uint32];
            let mut cif: ffi_cif = Default::default();
            let x = args.as_mut_ptr();
            let y = vec![
                &mut 4u32 as *mut _ as *mut c_void,
                &mut 5u32 as *mut _ as *mut c_void,
            ]
            .as_mut_ptr();

            let y = ptr.as_mut_ptr();
            prep_cif(
                &mut cif,
                ffi_abi_FFI_DEFAULT_ABI,
                2,
                &mut types::uint64,
                x, //args.as_mut_ptr(),
            )
            .unwrap();

            call::<u64>(
                &mut cif,
                CodePtr(c_function as *mut _),
                y, //vec![
                   // &mut 4u64 as *mut _ as *mut c_void,
                   // &mut 5u64 as *mut _ as *mut c_void,
                   // ]
                   // .as_mut_ptr(),
            )
        };
        let mut v = 1u32;
        let mut w = 1u32;
        let mut v = vec![1, 2];
        let g = v.as_mut_ptr();
        let gg = *g;

        println!("y: {:?}", gg);
        println!("y: {:?}", g);
        println!("{}", result);
        let y = vec![
            &mut 4u32 as *mut _ as *mut c_void,
            &mut 5u32 as *mut _ as *mut c_void,
        ]
        .as_mut_ptr();
        type FP = unsafe extern "C" fn();
        let lib = libloading::Library::new("kernel32.dll").unwrap();
        //lib.into::<imp::Library>().into_raw();
        // let x: Library = lib.into();
        // let y = x.into_raw() as isize;
        let lib2 = libloading::Library::new("C:\\work\\ffi\\Dll1\\x64\\Debug\\Dll1.dll").unwrap();
        //C:\work\ffi\Dll1\x64\Debug\Dll1.dll
        let sle: libloading::Symbol<unsafe extern "C" fn()> = lib.get(b"SetLastError").unwrap();
        //let sle: libloading::Symbol<unsafe extern "C" fn()> = lib2.get(b"test").unwrap();
        let gle_fp = *lib.get::<FP>(b"GetLastError\0").unwrap();
        let sle_fp = *sle;

        let mut gle_args_def: Vec<*mut ffi_type> = vec![];
        let mut gle_cif: ffi_cif = Default::default();

        let mut sle_args_def: [*mut ffi_type; 1] = [&mut types::uint32];
        let mut sle_cif: ffi_cif = Default::default();

        let mut sle_result = mem::MaybeUninit::<u32>::uninit();
        let mut gle_result = mem::MaybeUninit::<u32>::uninit();

        prep_cif(
            &mut gle_cif,
            ffi_abi_FFI_DEFAULT_ABI,
            0,
            &mut types::uint32,
            gle_args_def.as_mut_ptr(),
        )
        .unwrap();

        prep_cif(
            &mut sle_cif,
            ffi_abi_FFI_DEFAULT_ABI,
            1,
            &mut types::void,
            sle_args_def.as_mut_ptr(),
        )
        .unwrap();

        let mut sle_args = vec![&mut 99u32 as *mut _ as *mut c_void];

        libffi::raw::ffi_call(
            &mut sle_cif,
            Some(sle_fp),
            sle_result.as_mut_ptr() as *mut c_void,
            sle_args.as_mut_ptr(),
        );
        let ret = d
            .call::<u32>("kernel32.dll", "GetLastError", &mut vec![])
            .unwrap();
        println!("DynGetLastError: 0x{:08x}", ret);
        libffi::raw::ffi_call(
            &mut gle_cif,
            Some(gle_fp),
            gle_result.as_mut_ptr() as *mut c_void,
            &sle_args as *const _ as *mut *mut c_void,
        );
        println!("GetLastError: 0x{:08x}", gle_result.assume_init());

        let ret = d
            .call::<u32>("kernel32.dll", "GetLastError", &mut vec![])
            .unwrap();
        println!("GetLastError: 0x{:08x}", ret);
        let ff = glep();
        println!("GetLastError: 0x{:08x}", ff);
    }
}

/*

call "lib", "func",


*/
