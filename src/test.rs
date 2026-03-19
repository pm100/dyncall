mod test {
    use std::ffi::c_void;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::OnceLock;

    use crate::DynCaller;

    #[cfg(target_os = "windows")]
    const LIBC: &str = "msvcrt.dll";
    #[cfg(target_os = "macos")]
    const LIBC: &str = "libSystem.B.dylib";
    #[cfg(target_os = "linux")]
    const LIBC: &str = "libc.so.6";
    static STRUCT_FIXTURE_PATH: OnceLock<String> = OnceLock::new();

    fn struct_fixture_path() -> &'static str {
        STRUCT_FIXTURE_PATH.get_or_init(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let source = manifest_dir.join("tools").join("struct_fixture.rs");
            let output_dir = manifest_dir.join("target").join("struct-fixture");
            fs::create_dir_all(&output_dir).unwrap();
            let output = output_dir.join(struct_fixture_filename());
            let status = Command::new("rustc")
                .args([
                    "--crate-type",
                    "cdylib",
                    "--edition",
                    "2021",
                    source.to_str().unwrap(),
                    "-o",
                    output.to_str().unwrap(),
                ])
                .status()
                .unwrap();
            assert!(status.success(), "failed to build struct fixture");
            output
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
    }

    fn struct_fixture_filename() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "struct_fixture.dll"
        }
        #[cfg(target_os = "macos")]
        {
            "libstruct_fixture.dylib"
        }
        #[cfg(target_os = "linux")]
        {
            "libstruct_fixture.so"
        }
    }

    #[test]
    fn test_atoi() {
        let atoidef = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut atoi = atoidef.prep();
        let str = "12345".to_string();
        atoi.push_arg(&str).unwrap();
        let mut ret: i32 = 0;
        let retp = &raw mut ret;
        atoi.call_and_return(retp as *mut c_void);
        println!("atoi ret={:?}", ret);
        assert_eq!(ret, 12345);
    }
    #[test]
    fn test_atoi_simple() {
        let atoidef = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut atoi = atoidef.prep();
        let str = "12345".to_string();
        atoi.push_arg(&str).unwrap();
        let ret = atoi.call();
        println!("atoi ret={:?}", ret.as_i32().unwrap());
        assert_eq!(*ret.as_i32().unwrap(), 12345);
    }
    #[test]
    fn test_fread() {
        let fopen_def =
            DynCaller::define_function_by_str(&format!("{LIBC}|fopen|cstr,cstr|u64|")).unwrap();
        let fread_def =
            DynCaller::define_function_by_str(&format!("{LIBC}|fread|i32,i32,obuff=arg1,ptr|i32|"))
                .unwrap();
        let mut fopen = fopen_def.prep();
        let mut fread = fread_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name).unwrap();
        fopen.push_arg(&mode).unwrap();
        let ret = fopen.call();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fread.push_arg(&(1i32)).unwrap();
        fread.push_arg(&(50i32)).unwrap();
        fread.push_mut_arg(&mut buffer).unwrap();
        fread.push_arg(&ret).unwrap();
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
        let fgets_def =
            DynCaller::define_function_by_str(&format!("{LIBC}|fgets|ocstr=arg1,i32,ptr|i32|"))
                .unwrap();
        let mut fopen = fopen_def.prep();
        let mut fgets = fgets_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name).unwrap();
        fopen.push_arg(&mode).unwrap();
        let ret = fopen.call();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fgets.push_mut_arg(&mut buffer).unwrap();
        fgets.push_arg(&(50i32)).unwrap();
        fgets.push_arg(&ret).unwrap();
        let ret2 = fgets.call();
        println!("fgets  ret={:?}", ret2.as_i32().unwrap());
        println!("buffer read: {}", buffer);
    }

    #[test]
    fn test_printf() {
        let printf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|printf|cstr,cstr,i32|i32|fixargs=1"
        ))
        .unwrap();
        let format = "Hello, %s! You are %d years old.\n".to_string();
        let name = "Alice".to_string();
        let age = 30i32;
        let mut printf = printf_def.prep();
        printf.push_arg(&format).unwrap();
        printf.push_arg(&name).unwrap();
        printf.push_arg(&age).unwrap();
        let ret = printf.call();
        println!("printf ret={:?}", ret.as_i32().unwrap());
    }

    #[test]
    fn test_scanf() {
        let instr = "hello world 42\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,ocstr=50|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%s".to_string();
        let mut ans = String::new();
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
    }

    #[test]
    fn test_scanf_num_i32() {
        let instr = "42\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i32|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%d".to_string();
        let mut ans = 0i32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 42);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u32() {
        let instr = "4294967295\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u32|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%u".to_string();
        let mut ans = 0u32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 4_294_967_295);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u64() {
        let instr = "18446744073709551615\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u64|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%llu".to_string();
        let mut ans = 0u64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 18_446_744_073_709_551_615);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i64() {
        let instr = "-9223372036854775808\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i64|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%lld".to_string();
        let mut ans = 0i64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, -9_223_372_036_854_775_808);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_f32() {
        let instr = "3.14\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*f32|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%f".to_string();
        let mut ans = 0f32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert!((ans - 3.14f32).abs() < 1e-4);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i16() {
        let instr = "32767\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i16|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hd".to_string();
        let mut ans = 0i16;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 32767i16);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u16() {
        let instr = "65535\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u16|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hu".to_string();
        let mut ans = 0u16;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 65535u16);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i8() {
        let instr = "127\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i8|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hhd".to_string();
        let mut ans = 0i8;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 127i8);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u8() {
        let instr = "255\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u8|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hhu".to_string();
        let mut ans = 0u8;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 255u8);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_f64() {
        let instr = "3.14159\n".to_string();
        let sscanf_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|sscanf|cstr,cstr,*f64|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%lf".to_string();
        let mut ans = 0f64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
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
            "{LIBC}|sscanf|cstr,cstr,ocstr=50,*i32,*u32,*u64,*i64,*f64|i32|fixargs=2"
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
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut out).unwrap();
        sscanf.push_mut_arg(&mut i32v).unwrap();
        sscanf.push_mut_arg(&mut u32v).unwrap();
        sscanf.push_mut_arg(&mut u64v).unwrap();
        sscanf.push_mut_arg(&mut i64v).unwrap();
        sscanf.push_mut_arg(&mut f64v).unwrap();
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

    #[test]
    fn test_struct_by_value() {
        let lib = struct_fixture_path();
        let def =
            DynCaller::define_function_by_str(&format!("{lib}|sum_pair|{{u32,u32}}|u32|")).unwrap();
        let mut inv = def.prep();
        let mut pair = inv.create_struct(0).unwrap();
        pair.push_field(&10u32).unwrap();
        pair.push_field(&32u32).unwrap();
        inv.push_arg(&pair).unwrap();
        let ret = inv.call();
        assert_eq!(*ret.as_u32().unwrap(), 42);
    }

    #[test]
    fn test_struct_pointer_input() {
        let lib = struct_fixture_path();
        let def =
            DynCaller::define_function_by_str(&format!("{lib}|sum_pair_ptr|*{{u32,u32}}|u32|"))
                .unwrap();
        let mut inv = def.prep();
        let mut pair = def.create_struct(0).unwrap();
        pair.push_field(&11u32).unwrap();
        pair.push_field(&31u32).unwrap();
        inv.push_mut_arg(&mut pair).unwrap();
        let ret = inv.call();
        assert_eq!(*ret.as_u32().unwrap(), 42);
    }

    #[test]
    fn test_struct_pointer_mutation() {
        let lib = struct_fixture_path();
        let def = DynCaller::define_function_by_str(&format!("{lib}|bump_pair|*{{u32,u32}}|u32|"))
            .unwrap();
        let mut inv = def.prep();
        let mut pair = def.create_struct(0).unwrap();
        pair.push_field(&7u32).unwrap();
        pair.push_field(&8u32).unwrap();
        inv.push_mut_arg(&mut pair).unwrap();
        let ret = inv.call();
        assert_eq!(*ret.as_u32().unwrap(), 18);
        assert_eq!(pair.read_field::<u32>(0).unwrap(), 8);
        assert_eq!(pair.read_field::<u32>(1).unwrap(), 10);
    }

    #[test]
    fn test_mktime_and_strftime_with_struct_tm() {
        // Windows MSVCRT struct tm has only the 9 standard fields (36 bytes).
        // Linux/macOS libc appends tm_gmtoff (long = i64) and tm_zone (char* = i64),
        // making the struct 56 bytes. Passing a too-small buffer causes mktime to
        // overflow it when writing back normalised values.
        #[cfg(target_os = "windows")]
        let tm_desc = "{i32,i32,i32,i32,i32,i32,i32,i32,i32}";
        #[cfg(not(target_os = "windows"))]
        let tm_desc = "{i32,i32,i32,i32,i32,i32,i32,i32,i32,i64,i64}";
        let mktime_def =
            DynCaller::define_function_by_str(&format!("{LIBC}|mktime|*{tm_desc}|i64|")).unwrap();
        let strftime_def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|strftime|ocstr=arg1,u64,cstr,*{tm_desc}|u64|"
        ))
        .unwrap();

        let mut tm = mktime_def.create_struct(0).unwrap();
        tm.push_field(&5i32).unwrap();
        tm.push_field(&4i32).unwrap();
        tm.push_field(&3i32).unwrap();
        tm.push_field(&2i32).unwrap();
        tm.push_field(&0i32).unwrap();
        tm.push_field(&124i32).unwrap();
        tm.push_field(&0i32).unwrap();
        tm.push_field(&0i32).unwrap();
        tm.push_field(&(-1i32)).unwrap();

        let mut mktime = mktime_def.prep();
        mktime.push_mut_arg(&mut tm).unwrap();
        let mktime_ret = mktime.call();
        assert_ne!(*mktime_ret.as_i64().unwrap(), -1);

        let format = "%Y-%m-%d %H:%M:%S".to_string();
        let mut output = String::new();
        let mut strftime = strftime_def.prep();
        strftime.push_mut_arg(&mut output).unwrap();
        strftime.push_arg(&64u64).unwrap();
        strftime.push_arg(&format).unwrap();
        strftime.push_mut_arg(&mut tm).unwrap();
        let strftime_ret = strftime.call();

        assert_eq!(*strftime_ret.as_u64().unwrap(), 19);
        assert_eq!(output, "2024-01-02 03:04:05");
        assert_eq!(tm.read_field::<i32>(6).unwrap(), 2);
        assert_eq!(tm.read_field::<i32>(7).unwrap(), 1);
    }

    // ── coerce tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_coerce_i64_into_i32_strict_errors() {
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|")).unwrap();
        let mut inv = def.prep();
        assert!(inv.push_arg(&100i64).is_err());
    }

    #[test]
    fn test_coerce_i64_into_i32_with_coerce() {
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&(-42i64)).unwrap();
        let result = inv.call();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_string_into_cstr_slot() {
        // String → cstr slot: works in both strict and coerce mode
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&"42".to_string()).unwrap();
        let result = inv.call();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_int_into_cstr_slot() {
        // int → cstr: format as decimal
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|atoi|cstr|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&42i32).unwrap();
        let result = inv.call();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_string_into_int_slot() {
        // str → int: parse "42" → 42
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&"42".to_string()).unwrap();
        let result = inv.call();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_invalid_string_into_int_slot_errors() {
        // str → int parse failure: should error
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
        let mut inv = def.prep();
        assert!(inv.push_arg(&"hello".to_string()).is_err());
    }

    #[test]
    fn test_coerce_strict_type_mismatch_errors() {
        // Without coerce: i64 for i32 slot → Err
        let def = DynCaller::define_function_by_str(&format!("{LIBC}|abs|i32|i32|")).unwrap();
        let mut inv = def.prep();
        assert!(inv.push_arg(&42i64).is_err());
    }

    #[test]
    fn test_coerce_multiple_flags() {
        // fixargs=1,coerce together
        let def = DynCaller::define_function_by_str(&format!(
            "{LIBC}|printf|cstr,i32|i32|fixargs=1,coerce"
        ))
        .unwrap();
        assert!(def.is_coerce());
        let mut inv = def.prep();
        inv.push_arg(&"value: %d\n".to_string()).unwrap();
        inv.push_arg(&99i64).unwrap(); // i64 coerced to declared i32
        let ret = inv.call();
        assert!(*ret.as_i32().unwrap() > 0);
    }
}
