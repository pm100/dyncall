mod test {
    use std::ffi::c_void;

    use crate::DynCaller;

    #[cfg(target_os = "windows")]
    const LIBC: &str = "msvcrt.dll";
    #[cfg(target_os = "macos")]
    const LIBC: &str = "libSystem.B.dylib";
    #[cfg(target_os = "linux")]
    const LIBC: &str = "libc.so.6";

    #[test]
    fn test_atoi() {
        let atoidef =
            DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut atoi = atoidef.prep();
        let str = "12345".to_string();
        atoi.push_arg(&str);
        let mut ret: i32 = 0;
        let retp = &raw mut ret;
        atoi.call_and_return(retp as *mut c_void);
        println!("atoi ret={:?}", ret);
        assert_eq!(ret, 12345);
    }

    #[test]
    fn test_fread() {
        let fopen_def =
            DynCaller::define_function_by_str(&format!("{LIBC}|fopen|cstr,cstr|u64|")).unwrap();
        let fread_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|fread|i32,i32,obuff=arg1,ptr|i32|"
        ))
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
        fread.push_arg(&ret);
        let ret2 = fread.call();
        println!("fgets  ret={:?}", ret2.as_i32().unwrap());
        let rlen = ret2.as_i32().unwrap();
        unsafe {
            buffer.as_mut_vec().set_len(*rlen as usize);
        }
        println!("buffer read: {}", buffer);
    }

    #[test]
    fn test_fgets() {
        let fopen_def =
            DynCaller::define_function_by_str(&format!("{LIBC}|fopen|cstr,cstr|u64|")).unwrap();
        let fgets_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|fgets|ocstr=arg1,i32,ptr|i32|"
        ))
        .unwrap();
        let mut fopen = fopen_def.prep();
        let mut fgets = fgets_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name);
        fopen.push_arg(&mode);
        let ret = fopen.call();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fgets.push_mut_arg(&mut buffer);
        fgets.push_arg(&(50i32));
        fgets.push_arg(&ret);
        let ret2 = fgets.call();
        println!("fgets  ret={:?}", ret2.as_i32().unwrap());
        println!("buffer read: {}", buffer);
    }

    #[test]
    fn test_printf() {
        let printf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|printf|cstr,cstr,i32|i32|vararg=1"
        ))
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
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,ocstr=50|i32|vararg=1"
        ))
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
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i32|i32|vararg=1"
        ))
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
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u32|i32|vararg=1"
        ))
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
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u64|i32|vararg=1"
        ))
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
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i64|i32|vararg=1"
        ))
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
    fn test_scanf_num_f32() {
        let instr = "3.14\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*f32|i32|vararg=1"
        ))
        .unwrap();
        let format = "%f".to_string();
        let mut ans = 0f32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert!((ans - 3.14f32).abs() < 1e-4);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i16() {
        let instr = "32767\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i16|i32|vararg=1"
        ))
        .unwrap();
        let format = "%hd".to_string();
        let mut ans = 0i16;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 32767i16);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u16() {
        let instr = "65535\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u16|i32|vararg=1"
        ))
        .unwrap();
        let format = "%hu".to_string();
        let mut ans = 0u16;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 65535u16);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i8() {
        let instr = "127\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i8|i32|vararg=1"
        ))
        .unwrap();
        let format = "%hhd".to_string();
        let mut ans = 0i8;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 127i8);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u8() {
        let instr = "255\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u8|i32|vararg=1"
        ))
        .unwrap();
        let format = "%hhu".to_string();
        let mut ans = 0u8;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr);
        sscanf.push_arg(&format);
        sscanf.push_mut_arg(&mut ans);
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 255u8);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_f64() {
        let instr = "3.14159\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*f64|i32|vararg=1"
        ))
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
            "hello 42 4294967295 18446744073709551615 -9223372036854775808 3.14159\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,ocstr=50,*i32,*u32,*u64,*i64,*f64|i32|vararg=1"
        ))
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
