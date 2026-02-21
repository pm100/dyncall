mod test {
    use std::{
        ffi::{c_void, CString},
        mem,
    };

    use crate::DynCaller;
    // #[test]
    // fn test_caller() {
    //     //  let mut dyncaller = DynCaller::new();
    //     let mut fid = DynCaller::define_function_by_str("msvcrt.dll|puts|ptr|i32|").unwrap();
    //     let str = CString::new("hello").unwrap();
    //     //  str.to_call_arg(&mut fid);
    //     let str = "hello".to_string();
    //     fid.push_arg(&str);
    //     fid.call2();
    //     //println!("puts ret={}", fid.call::<u64>().unwrap());
    //     let namex = CString::new("test.txt").unwrap();
    //     let mode = CString::new("w").unwrap();
    //     let mut fopen = DynCaller::define_function_by_str("msvcrt.dll|fopen|ptr,ptr|u64|").unwrap();
    //     fopen.push_arg(&namex);
    //     fopen.push_arg(&mode);
    //     let ret = fopen.call2();
    //     println!("fopen ret={:?}", ret);
    //     let mut fputs = DynCaller::define_function_by_str("msvcrt.dll|fputs|ptr,ptr|u64").unwrap();
    //     let text = c"Hello from Rust fputs!\n";
    //     fputs.push_arg(text);
    //     fputs.push_arg(ret.as_u64().unwrap());
    //     let ret2 = fputs.call2();
    // }
    // #[test]

    // fn test_stdout() {
    //     //        let mut dyncaller = DynCaller::new();
    //     let namex = CString::new("test.txt").unwrap();
    //     let mode = CString::new("w").unwrap();
    //     let mut fopen = DynCaller::define_function_by_str("msvcrt.dll|fopen|ptr,ptr|u64|").unwrap();
    //     let mut errno = DynCaller::define_function_by_str("msvcrt.dll|_errno||i32|").unwrap();
    //     let mut perror = DynCaller::define_function_by_str("msvcrt.dll|perror|ptr|i32|").unwrap();
    //     fopen.push_arg(&namex);
    //     fopen.push_arg(&mode);
    //     let fh = fopen.call2();

    //     let mut stdio =
    //         DynCaller::define_function_by_str("ucrtbase|__acrt_iob_func|u32|u64|").unwrap();
    //     let mut gle = DynCaller::define_function_by_str("kernel32|GetLastError||u32|").unwrap();
    //     let mut fputs =
    //         DynCaller::define_function_by_str("ucrtbase.dll|fputs|ptr,ptr|i32|").unwrap();
    //     let str = CString::new("hello stdout").unwrap();
    //     stdio.push_arg(&1u32); // stdout index
    //     let stdout_ptr = stdio.call2();
    //     let stdout_ptr = stdout_ptr.as_u64().unwrap();
    //     println!("stdout ptr={:x}", stdout_ptr);
    //     println!("fh ptr={:x}", fh.as_u64().unwrap());
    //     let ret = gle.call2();
    //     println!("GetLastError after __acrt_iob_func: {:?}", ret);

    //     fputs.push_arg(&str);
    //     fputs.push_arg(stdout_ptr);
    //     let retf = fputs.call2();
    //     if *retf.as_i32().unwrap() < 0 {
    //         perror.push_arg(&str);
    //         perror.call2();
    //     }
    //     let err = errno.call2();
    //     println!("errno before fputs: {:?}", err);
    //     let ret = gle.call2();

    //     println!("GetLastError after __acrt_iob_func: {:?}", ret);
    //     println!("fputs ret={:?}", retf);
    // }
    #[test]

    fn test_atoi() {
        //   let mut dyncaller = DynCaller::new();
        let atoidef = DynCaller::define_function_by_str("msvcrt.dll|atoi|cstr|i32|").unwrap();
        let mut atoi = atoidef.prep();
        //let str = CString::new("12345").unwrap();
        let str = "12345".to_string();
        atoi.push_arg(&str);
        let mut ret: i32 = 0;
        let retp = &raw mut ret;
        atoi.call_and_return(retp as *mut c_void);
        println!("atoi ret={:?}", ret);
        assert_eq!(ret, 12345);
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
    // #[test]
    // fn test_fgets() {
    //     let mut dyncaller = DynCaller::new();
    //     let mut fopen = dyncaller
    //         .define_function_by_str("msvcrt.dll|fopen|cstr,cstr|u64")
    //         .unwrap();

    //     let mut fgets = dyncaller
    //         .define_function_by_str("msvcrt.dll|fgets|ocstr,i32,ptr|u64")
    //         .unwrap();
    //     let name = "test.txt2".to_string();
    //     let mode = "r".to_string();
    //     fopen.push_arg(&name);
    //     fopen.push_arg(&mode);
    //     let ret = fopen.call2();
    //     println!("fopen ret={:x}", ret.as_u64().unwrap());
    //     let mut buff = String::with_capacity(100);
    //     fgets.push_mut_arg(&mut buff);
    //     fgets.push_arg(&(100i32));
    //    // fgets.push_arg(&ret);
    //     let ret2 = fgets.call2();
    //     println!("fgets ret={:x}", ret2.as_u64().unwrap());
    // }
    #[test]
    fn test_fread() {
        let fopen_def =
            DynCaller::define_function_by_str("msvcrt.dll|fopen|cstr,cstr|u64|").unwrap();

        let fread_def =
            DynCaller::define_function_by_str("msvcrt.dll|fread|i32,i32,obuff=arg1,ptr|i32|")
                .unwrap();
        let mut fopen = fopen_def.prep();
        let mut fread = fread_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name);
        fopen.push_arg(&mode);
        let ret = fopen.call();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fread.push_arg(&(1i32));
        fread.push_arg(&(50i32));
        fread.push_mut_arg(&mut buffer);

        //let ret = ret.as_u64().unwrap();
        fread.push_arg(&ret);
        let ret2 = fread.call();
        println!("fgets  ret={:?}", ret2.as_i32().unwrap());
        let rlen = ret2.as_i32().unwrap();
        unsafe {
            buffer.as_mut_vec().set_len(*rlen as usize);
        }
        //let s = String::from_utf8_lossy(&buffer);
        println!("buffer read: {}", buffer);
    }

    #[test]
    fn test_fgets() {
        let fopen_def =
            DynCaller::define_function_by_str("msvcrt.dll|fopen|cstr,cstr|u64|").unwrap();

        let fread_def =
            DynCaller::define_function_by_str("msvcrt.dll|fgets|ocstr=arg1,i32,ptr|i32|").unwrap();
        let mut fopen = fopen_def.prep();
        let mut fgets = fread_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name);
        fopen.push_arg(&mode);
        let ret = fopen.call();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fgets.push_mut_arg(&mut buffer);
        fgets.push_arg(&(50i32));

        //let ret = ret.as_u64().unwrap();
        fgets.push_arg(&ret);
        let ret2 = fgets.call();
        println!("fgets  ret={:?}", ret2.as_i32().unwrap());
        //let s = String::from_utf8_lossy(&buffer);
        println!("buffer read: {}", buffer);
    }
    // #[test]
    // fn testcpp() {
    //     let mut atoi = DynCaller::define_function_by_str(
    //         "C:\\work\\ffi\\Dll1\\x64\\Debug\\Dll1.dll|testgets|ocstr|i32|",
    //     )
    //     .unwrap();

    //     let mut buff = String::with_capacity(50);
    //     //atoi.push_arg(&29);
    //     atoi.push_mut_arg(&mut buff);
    //     let ret = atoi.call2();
    //     println!("atoi ret={:?}", buff);
    //     //    assert_eq!(*ret.as_i32().unwrap(), 12345);
    // }
    #[test]
    fn test_printf() {
        //let mut dyncaller = DynCaller::new();
        let printf_def =
            DynCaller::define_function_by_str("msvcrt.dll|printf|cstr,cstr,i32|i32|vararg=1")
                .unwrap();
        let format = "Hello, %s! You are %d years old.\n".to_string();
        let name = "Alice".to_string();
        let age = 30i32;
        let mut printf = printf_def.prep();
        printf.push_arg(&format);
        printf.push_arg(&name);
        printf.push_arg(&age);

        let ret = printf.call();
        println!("printf ret={:?}", ret.as_i32().unwrap());
    }
    #[test]
    fn test_scanf() {
        let instr = "hello world 42\n".to_string();
        let sscanf_def =
            DynCaller::define_function_by_str("msvcrt.dll|sscanf|cstr,cstr,ocstr=50|i32|vararg=1")
                .unwrap();
        let format = "%s".to_string();

        let mut ans = String::new();
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);

        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
    }
    #[test]
    fn test_scanf_num_i32() {
        let instr = "42\n".to_string();
        let sscanf_def =
            DynCaller::define_function_by_str("msvcrt.dll|sscanf|cstr,cstr,*i32|i32|vararg=1")
                .unwrap();
        let format = "%d".to_string();

        let mut ans = 0i32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);

        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 42);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u32() {
        let instr = "4294967295\n".to_string();
        let sscanf_def =
            DynCaller::define_function_by_str("msvcrt.dll|sscanf|cstr,cstr,*u32|i32|vararg=1")
                .unwrap();
        let format = "%u".to_string();

        let mut ans = 0u32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);

        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 4_294_967_295);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u64() {
        let instr = "18446744073709551615\n".to_string();
        let sscanf_def =
            DynCaller::define_function_by_str("msvcrt.dll|sscanf|cstr,cstr,*u64|i32|vararg=1")
                .unwrap();
        let format = "%llu".to_string();

        let mut ans = 0u64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);

        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 18_446_744_073_709_551_615);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i64() {
        let instr = "-9223372036854775808\n".to_string();
        let sscanf_def =
            DynCaller::define_function_by_str("msvcrt.dll|sscanf|cstr,cstr,*i64|i32|vararg=1")
                .unwrap();
        let format = "%lld".to_string();

        let mut ans = 0i64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);

        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, -9_223_372_036_854_775_808);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_f64() {
        let instr = "3.14159\n".to_string();
        let sscanf_def =
            DynCaller::define_function_by_str("msvcrt.dll|sscanf|cstr,cstr,*f64|i32|vararg=1")
                .unwrap();
        let format = "%lf".to_string();

        let mut ans = 0f64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);

        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert!((ans - 3.14159).abs() < 1e-9);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_string_and_numbers() {
        let instr =
            "hello 42 4294967295 18446744073709551615 -9223372036854775808 3.14159\n"
                .to_string();
        let sscanf_def = DynCaller::define_function_by_str(
            "msvcrt.dll|sscanf|cstr,cstr,ocstr=50,*i32,*u32,*u64,*i64,*f64|i32|vararg=1",
        )
        .unwrap();
        let format = "%s %d %u %llu %lld %lf".to_string();

        let mut out = String::with_capacity(50);
        let mut i32v = 0i32;
        let mut u32v = 0u32;
        let mut u64v = 0u64;
        let mut i64v = 0i64;
        let mut f64v = 0f64;

        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut out);
        sscanf.push_mut_arg(&mut i32v);
        sscanf.push_mut_arg(&mut u32v);
        sscanf.push_mut_arg(&mut u64v);
        sscanf.push_mut_arg(&mut i64v);
        sscanf.push_mut_arg(&mut f64v);

        let ret = sscanf.call();
        println!(
            "sscanf ret={:?} out={} i32={} u32={} u64={} i64={} f64={}",
            ret.as_i32().unwrap(),
            out,
            i32v,
            u32v,
            u64v,
            i64v,
            f64v
        );
        assert_eq!(out, "hello");
        assert_eq!(i32v, 42);
        assert_eq!(u32v, 4_294_967_295);
        assert_eq!(u64v, 18_446_744_073_709_551_615);
        assert_eq!(i64v, -9_223_372_036_854_775_808);
        assert!((f64v - 3.14159).abs() < 1e-9);
        assert_eq!(*ret.as_i32().unwrap(), 6);
    }
}
