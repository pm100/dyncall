use std::collections::btree_map::OccupiedEntry;
use std::collections::hash_map::Entry;
use std::ffi::{self, c_void};
use std::mem;
use std::path::Path;
use std::{collections::HashMap, ptr};
type RawLib = isize;
use anyhow::Result;
use anyhow::{anyhow, bail};
use libffi::low::*;
use libffi::raw::ffi_call;
use libloading::os::windows::Library;

use crate::dylib::DynamicLibrary;

pub struct DynCaller {
    libs: HashMap<String, DynamicLibrary>,
    entry_points: HashMap<String, *mut ffi::c_void>,
    cifs: HashMap<String, ffi_cif>,
}

impl DynCaller {
    pub fn new() -> Self {
        DynCaller {
            libs: HashMap::new(),
            entry_points: HashMap::new(),
            cifs: HashMap::new(),
        }
    }

    fn get_lib(&mut self, lib_name: &str) -> Result<DynamicLibrary> {
        if self.libs.contains_key(lib_name) {
            return Ok(self.libs.get(lib_name).unwrap().clone());
        }
        // let res = self
        //     .libs
        //     .entry(lib_name.to_string())
        //     .or_insert_with(|| unsafe { libloading::Library::new(lib_name).unwrap() });

        // {
        //     if let Some(lib) = self.libs.get(lib_name) {
        //         return Ok(lib);
        //     }
        // }

        let lib = unsafe { DynamicLibrary::open(Some(Path::new(lib_name)))? };

        self.libs.insert(lib_name.to_string(), lib);
        // // return Ok(lib);
        return Ok(self.libs.get(lib_name).unwrap().clone());
    }

    fn get_entry_point(
        &mut self,
        lib_name: &str,
        entry_point_name: &str,
    ) -> Result<*mut ffi::c_void> {
        let name = format!("{}::{}", lib_name, entry_point_name);

        // if self.entry_points.contains_key(&name) {
        //     return Ok(self.entry_points.get(&name).unwrap().clone());
        // }
        let lib = self.get_lib(lib_name)?;
        match self.entry_points.entry(name) {
            Entry::Occupied(e) => {
                return Ok(e.get().clone());
            }
            Entry::Vacant(e) => {
                //let lib = self.get_lib(lib_name)?;

                let ep = unsafe { lib.symbol(entry_point_name)? };
                let q = e.insert(ep);
                //Ok(*q.clone())
                Ok(ep)
            }
        }
        //     let lib = self.get_lib(lib_name)?;
        //     let x = unsafe { libloading::os::windows::Library::from_raw(lib) };
        //     let y = libloading::Library::from(x);
        //     //  let lib = unsafe { libloading::Library::from_raw(lib as *mut std::ffi::c_void) };
        //     let entry_point: libloading::Symbol<unsafe extern "C" fn()> =
        //         unsafe { y.get(entry_point_name.as_bytes())? };
        //   //  self.entry_points.insert(name.clone(), entry_point);
        //     return Ok(self.entry_points.get(&name).unwrap().clone());
    }
    fn get_cif(&mut self, lib_name: &str, entry_point: &str) -> Result<ffi_cif> {
        let name = format!("{}::{}", lib_name, entry_point);
        // if let Some(cif) = self.cifs.get(&name) {
        //     return Ok(cif);
        // }
        // let mut cif = Default::default();
        // unsafe {
        //     prep_cif(
        //         &mut cif,
        //         ffi_abi_FFI_DEFAULT_ABI,
        //         0,
        //         &mut types::void,
        //         std::ptr::null_mut(),
        //     )
        //     .map_err(|e| anyhow!(format!("{:?}", e)))?;
        // };
        // self.cifs.insert(name.clone(), cif);
        // Ok(self.cifs.get(&name).unwrap())

        match self.cifs.entry(name) {
            Entry::Occupied(e) => Ok((*e.get()).clone()),
            Entry::Vacant(e) => {
                let mut cif = Default::default();
                unsafe {
                    prep_cif(
                        &mut cif,
                        ffi_abi_FFI_DEFAULT_ABI,
                        0,
                        &mut types::void,
                        std::ptr::null_mut(),
                    )
                    .map_err(|e| anyhow!(format!("{:?}", e)))?;
                };
                Ok(*e.insert(cif))
                // Ok(e.get())
            }
        }
    }
    pub fn setup_call(
        &mut self,
        lib_name: &str,
        entry_point_name: &str,
        mut return_type: ffi_type,
        args: &Vec<&mut ffi_type>,
    ) -> Result<()> {
        let lib = self.get_lib(lib_name)?;
        let entry_point = self.get_entry_point(lib_name, entry_point_name)?;
        // let mut rt = types::uint8;
        let mut cif = Default::default();
        unsafe {
            prep_cif(
                &mut cif,
                ffi_abi_FFI_DEFAULT_ABI,
                args.len() as usize,
                //ptr::addr_of_mut!(return_type), // as *mut ffi_type,
                &mut return_type,
                args.as_ptr() as *mut *mut ffi_type,
            )
            .map_err(|e| anyhow!(format!("{:?}", e)))?;
        };
        let name = format!("{}::{}", lib_name, entry_point_name);
        self.cifs.insert(name, cif);
        Ok(())
    }
    pub fn call<T>(
        &mut self,
        lib_name: &str,
        entry_point_name: &str,
        args: &mut Vec<*mut c_void>,
    ) -> Result<T>
    where
        T: Default,
    {
        let le = unsafe { GetLastError() };
        let mut cif = self.get_cif(lib_name, entry_point_name)?;
        let entry_point = self.get_entry_point(lib_name, entry_point_name)?;

        let mut result = mem::MaybeUninit::<T>::uninit();
        // let mut args = vec![&mut 99u32 as *mut _ as *mut c_void];
        let ep = unsafe { std::mem::transmute(entry_point) };
        unsafe {
            SetLastError(le);
        }
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
#[allow(non_snake_case)]
extern "system" {
    fn SetLastError(error: libc::c_uint);
    fn SetThreadErrorMode(uMode: libc::c_uint, oldMode: *mut libc::c_uint) -> libc::c_uint;
    fn LoadLibraryW(name: *const libc::c_void) -> *mut libc::c_void;
    fn GetModuleHandleExW(
        dwFlags: libc::c_uint,
        name: *const u16,
        handle: *mut *mut libc::c_void,
    ) -> libc::c_uint;
    fn GetProcAddress(handle: *mut libc::c_void, name: *const libc::c_char) -> *mut libc::c_void;
    fn FreeLibrary(handle: *mut libc::c_void);
    fn SetErrorMode(uMode: libc::c_uint) -> libc::c_uint;
    fn GetLastError() -> libc::c_uint;
}
