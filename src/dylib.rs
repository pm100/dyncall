// Copyright 2013-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Dynamic library facilities.
//!
//! A simple wrapper over the platform's dynamic library facilities

extern crate libc;
use anyhow::Result;

use std::ffi::{CString, OsString};
use std::mem;
use std::path::{Path, PathBuf};
use std::{env, ffi};
#[derive(Clone)]
pub struct DynamicLibrary {
    handle: usize,
}

#[allow(dead_code)]
impl DynamicLibrary {
    /// Lazily open a dynamic library. When passed None it gives a
    /// handle to the calling process
    pub fn open(filename: Option<&Path>) -> Result<DynamicLibrary> {
        let maybe_library = dl::open(filename.map(|path| path.as_os_str()));

        match maybe_library {
            Err(err) => Err(err),
            Ok(handle) => Ok(DynamicLibrary {
                handle: handle as usize,
            }),
        }
    }

    /// Prepends a path to this process's search path for dynamic libraries
    pub fn prepend_search_path(path: &Path) {
        let mut search_path = DynamicLibrary::search_path();
        search_path.insert(0, path.to_path_buf());
        env::set_var(
            DynamicLibrary::envvar(),
            &DynamicLibrary::create_path(&search_path),
        );
    }

    /// From a slice of paths, create a new vector which is suitable to be an
    /// environment variable for this platforms dylib search path.
    pub fn create_path(path: &[PathBuf]) -> OsString {
        let mut newvar = OsString::new();
        for (i, path) in path.iter().enumerate() {
            if i > 0 {
                newvar.push(DynamicLibrary::separator());
            }
            newvar.push(path);
        }
        newvar
    }

    /// Returns the environment variable for this process's dynamic library
    /// search path
    pub fn envvar() -> &'static str {
        if cfg!(windows) {
            "PATH"
        } else if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        }
    }

    fn separator() -> &'static str {
        if cfg!(windows) {
            ";"
        } else {
            ":"
        }
    }

    /// Returns the current search path for dynamic libraries being used by this
    /// process
    pub fn search_path() -> Vec<PathBuf> {
        match env::var_os(DynamicLibrary::envvar()) {
            Some(var) => env::split_paths(&var).collect(),
            None => Vec::new(),
        }
    }

    /// Access the value at the symbol of the dynamic library
    pub unsafe fn symbol(&self, symbol: &str) -> Result<*mut ffi::c_void> {

        let raw_string = CString::new(symbol).unwrap();
        let maybe_symbol_value =
            dl::check_for_errors_in(|| dl::symbol(self.handle as *mut u8, raw_string.as_ptr()));

        // The value must not be constructed if there is an error so
        // the destructor does not run.
        match maybe_symbol_value {
            Err(err) => Err(err),
            Ok(symbol_value) => Ok(mem::transmute(symbol_value)),
        }
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd"
))]
mod dl {
    use anyhow::bail;
    use anyhow::Result;
    use libc;
    use std::ffi::{CStr, CString, OsStr};
    use std::os::unix::ffi::OsStrExt;
    use std::ptr;
    use std::str;
    pub fn open(filename: Option<&OsStr>) -> Result<*mut u8> {
        check_for_errors_in(|| unsafe {
            match filename {
                Some(filename) => open_external(filename),
                None => open_internal(),
            }
        })
    }

    const LAZY: libc::c_int = 1;

    unsafe fn open_external(filename: &OsStr) -> *mut u8 {
        let s = CString::new(filename.as_bytes()).unwrap(); //to_cstring().unwrap();
        dlopen(s.as_ptr(), LAZY) as *mut u8
    }

    unsafe fn open_internal() -> *mut u8 {
        dlopen(ptr::null(), LAZY) as *mut u8
    }

    pub fn check_for_errors_in<T, F>(f: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        unsafe {
            let result = f();

            let last_error = dlerror();
            if last_error.is_null() {
                Ok(result)
            } else {
                let s = CStr::from_ptr(last_error).to_bytes();
                bail!(str::from_utf8(s).unwrap().to_string())
            }
        }
    }

    pub unsafe fn symbol(handle: *mut u8, symbol: *const libc::c_char) -> *mut u8 {
        dlsym(handle as *mut libc::c_void, symbol) as *mut u8
    }
    pub unsafe fn close(handle: *mut u8) {
        dlclose(handle as *mut libc::c_void);
        ()
    }

    extern "C" {
        fn dlopen(filename: *const libc::c_char, flag: libc::c_int) -> *mut libc::c_void;
        fn dlerror() -> *mut libc::c_char;
        fn dlsym(handle: *mut libc::c_void, symbol: *const libc::c_char) -> *mut libc::c_void;
        fn dlclose(handle: *mut libc::c_void) -> libc::c_int;
    }
}

#[cfg(target_os = "windows")]
mod dl {
    use std::ffi::OsStr;
    use std::iter::Iterator;
    use anyhow::{anyhow, Result};
    use std::ops::FnOnce;
    use std::option::Option::{self, None, Some};
    use std::os::windows::prelude::*;
    use std::ptr;
    use std::vec::Vec;

    pub fn open(filename: Option<&OsStr>) -> Result<*mut u8> {
        // disable "dll load failed" error dialog.
        let use_thread_mode = true;
        let prev_error_mode = unsafe {
            let new_error_mode = 1;
            let mut prev_error_mode = 0;
            let _result = SetThreadErrorMode(new_error_mode, &mut prev_error_mode);
            prev_error_mode
        };

        unsafe {
            SetLastError(0);
        }

        let result = match filename {
            Some(filename) => {
                let filename_str: Vec<_> =
                    filename.encode_wide().chain(Some(0).into_iter()).collect();
                let result = unsafe { LoadLibraryW(filename_str.as_ptr() as *const libc::c_void) };
                if result == ptr::null_mut() {
                    let err = unsafe { GetLastError() };
                    Err(anyhow!("Error code {:08x}", err))
                } else {
                    Ok(result as *mut u8)
                }
            }
            None => {
                let mut handle = ptr::null_mut();
                let succeeded =
                    unsafe { GetModuleHandleExW(0 as libc::c_uint, ptr::null(), &mut handle) };
                if succeeded == 0 {
                    let err = unsafe { GetLastError() };
                    Err(anyhow!("Error code {:08x}", err))
                } else {
                    Ok(handle as *mut u8)
                }
            }
        };

        unsafe {
            if use_thread_mode {
                SetThreadErrorMode(prev_error_mode, ptr::null_mut());
            } else {
                SetErrorMode(prev_error_mode);
            }
        }

        result
    }

    pub fn check_for_errors_in<T, F>(f: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        unsafe {
            SetLastError(0);

            let result = f();

            let error = GetLastError();
            if 0 == error {
                Ok(result)
            } else {
                Err(anyhow::anyhow!("Error code {}", error))
            }
        }
    }

    pub unsafe fn symbol(handle: *mut u8, symbol: *const libc::c_char) -> *mut u8 {
        GetProcAddress(handle as *mut libc::c_void, symbol) as *mut u8
    }
    #[allow(dead_code)]
    pub unsafe fn close(handle: *mut u8) {
        FreeLibrary(handle as *mut libc::c_void);
    }

    #[allow(non_snake_case, dead_code)]
    extern "system" {
        fn SetLastError(error: libc::size_t);
        fn SetThreadErrorMode(uMode: libc::c_uint, oldMode: *mut libc::c_uint) -> libc::c_uint;
        fn LoadLibraryW(name: *const libc::c_void) -> *mut libc::c_void;
        fn GetModuleHandleExW(
            dwFlags: libc::c_uint,
            name: *const u16,
            handle: *mut *mut libc::c_void,
        ) -> libc::c_uint;
        fn GetProcAddress(
            handle: *mut libc::c_void,
            name: *const libc::c_char,
        ) -> *mut libc::c_void;
        fn FreeLibrary(handle: *mut libc::c_void);
        fn SetErrorMode(uMode: libc::c_uint) -> libc::c_uint;
        fn GetLastError() -> libc::c_uint;
    }
}
