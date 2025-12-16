mod test {
    use std::{
        ffi::{c_void, CString},
        mem,
    };

    use crate::DynCaller;
    #[test]
    fn test_caller() {
        let mut dyncaller = DynCaller::new();
        let mut fid = dyncaller
            .define_function_by_str("msvcrt.dll|puts|ptr|i32")
            .unwrap();
        let str = CString::new("hello").unwrap();
        //  str.to_call_arg(&mut fid);
        let str = "hello".to_string();
        fid.push_arg(&str);
        fid.call2();
        //println!("puts ret={}", fid.call::<u64>().unwrap());
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
        let ret2 = fputs.call2();
    }
    #[test]
    fn test_stdout() {
        let mut dyncaller = DynCaller::new();
        let namex = CString::new("test.txt").unwrap();
        let mode = CString::new("w").unwrap();
        let mut fopen = dyncaller
            .define_function_by_str("msvcrt.dll|fopen|ptr,ptr|u64")
            .unwrap();
        let mut errno = dyncaller
            .define_function_by_str("msvcrt.dll|_errno||i32")
            .unwrap();
        let mut perror = dyncaller
            .define_function_by_str("msvcrt.dll|perror|ptr|i32")
            .unwrap();
        fopen.push_arg(&namex);
        fopen.push_arg(&mode);
        let fh = fopen.call2();

        let mut stdio = dyncaller
            .define_function_by_str("ucrtbase|__acrt_iob_func|u32|u64")
            .unwrap();
        let mut gle = dyncaller
            .define_function_by_str("kernel32|GetLastError||u32")
            .unwrap();
        let mut fputs = dyncaller
            .define_function_by_str("ucrtbase.dll|fputs|ptr,ptr|i32")
            .unwrap();
        let str = CString::new("hello stdout").unwrap();
        stdio.push_arg(&1u32); // stdout index
        let stdout_ptr = stdio.call2();
        println!("stdout ptr={:x}", stdout_ptr.as_u64().unwrap());
        println!("fh ptr={:x}", fh.as_u64().unwrap());
        let ret = gle.call2();
        println!("GetLastError after __acrt_iob_func: {:?}", ret);

        fputs.push_arg(&str);
        fputs.push_arg(&stdout_ptr);
        let retf = fputs.call2();
        if *retf.as_i32().unwrap() < 0 {
            perror.push_arg(&str);
            perror.call2();
        }
        let err = errno.call2();
        println!("errno before fputs: {:?}", err);
        let ret = gle.call2();

        println!("GetLastError after __acrt_iob_func: {:?}", ret);
        println!("fputs ret={:?}", retf);
    }
    #[test]

    fn test_atoi() {
        let mut dyncaller = DynCaller::new();
        let mut atoi = dyncaller
            .define_function_by_str("msvcrt.dll|atoi|ptr|i32")
            .unwrap();
        let str = CString::new("12345").unwrap();
        let str = "12345".to_string();
        atoi.push_arg(&str);
        let ret = atoi.call2();
        println!("atoi ret={:?}", ret);
        assert_eq!(*ret.as_i32().unwrap(), 12345);
    }
    // #[test]
    // fn test_caller_str() {
    //     let mut dyncaller = DynCaller::new();
    //     let fopen = dyncaller
    //         .define_function_by_str("msvcrt.dll|fopen|ptr,ptr|u64")
    //         .unwrap();

    //     let fputs = dyncaller
    //         .define_function_by_str("msvcrt.dll|fputs|ptr,ptr|u64")
    //         .unwrap();

    //     //let mut m = Marshaller::new();
    //     let name = c"test.txt2"; //.to_string();
    //     let mode = c"w";
    //     let pmode = mode.as_ptr(); // as *mut c_void;
    //     let pname = name.as_ptr(); // as *mut c_void;
    //     let _rp = &raw const pname;

    //     let mut pvec: Vec<*mut c_void> = Vec::new();
    //     pvec.push(unsafe { mem::transmute::<*const *const i8, *mut c_void>(&pname) });
    //     //  pvec.push(name.to_call_arg());
    //     pvec.push(unsafe { mem::transmute::<*const *const i8, *mut c_void>(&pmode) });
    //     println!("pvec[0]={:x}", pvec[0] as u64);
    //     println!("pvec[1]={:x}", pvec[1] as u64);
    //     // let (buf, mut args) = m.build_buffer();
    //     let ret = dyncaller.call::<u64>(&fopen, &mut pvec).unwrap();
    //     println!("fopen ret={:x}", ret);

    //     let text = c"Hello from Rust fputs!\n";
    //     let ptext = text.as_ptr(); // as *mut c_void;
    //     let mut pvec2: Vec<*mut c_void> = Vec::new();

    //     pvec2.push(unsafe { mem::transmute::<*const *const i8, *mut c_void>(&ptext) });
    //     pvec2.push(unsafe { mem::transmute::<*const u64, *mut c_void>(&ret) });
    //     let ret2 = dyncaller.call::<u64>(&fputs, &mut pvec2).unwrap();
    //     println!("fputs ret={:x}", ret2);
    // }
    #[test]
    fn test_fgets() {
        let mut dyncaller = DynCaller::new();
        let mut fopen = dyncaller
            .define_function_by_str("msvcrt.dll|fopen|cstr,cstr|u64")
            .unwrap();

        let mut fgets = dyncaller
            .define_function_by_str("msvcrt.dll|fgets|ocstr,i32,ptr|u64")
            .unwrap();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name);
        fopen.push_arg(&mode);
        let ret = fopen.call2();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buff = String::with_capacity(100);
        fgets.push_mut_arg(&mut buff);
        fgets.push_arg(&(100i32));
        fgets.push_arg(&ret);
        let ret2 = fgets.call2();
        println!("fgets ret={:x}", ret2.as_u64().unwrap());
    }
}
