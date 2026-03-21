mod test {
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
        let atoidef = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut atoi = atoidef.prep();
        let str = "12345".to_string();
        atoi.push_arg(&str).unwrap();
        let ret = atoi.call().unwrap();
        println!("atoi ret={:?}", ret.as_i32().unwrap());
        assert_eq!(*ret.as_i32().unwrap(), 12345);
    }
    #[test]
    fn test_atoi_simple() {
        let atoidef = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut atoi = atoidef.prep();
        let str = "12345".to_string();
        atoi.push_arg(&str).unwrap();
        let ret = atoi.call().unwrap();
        println!("atoi ret={:?}", ret.as_i32().unwrap());
        assert_eq!(*ret.as_i32().unwrap(), 12345);
    }
    #[test]
    fn test_fread() {
        let fopen_def =
            DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|u64|")).unwrap();
        let fread_def =
            DynCaller::define_function(&format!("{LIBC}|fread|i32,i32,obuff=arg1,ptr|i32|"))
                .unwrap();
        let mut fopen = fopen_def.prep();
        let mut fread = fread_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name).unwrap();
        fopen.push_arg(&mode).unwrap();
        let ret = fopen.call().unwrap();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fread.push_arg(&(1i32)).unwrap();
        fread.push_arg(&(50i32)).unwrap();
        fread.push_mut_arg(&mut buffer).unwrap();
        fread.push_arg(&ret).unwrap();
        let ret2 = fread.call().unwrap();
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
            DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|u64|")).unwrap();
        let fgets_def =
            DynCaller::define_function(&format!("{LIBC}|fgets|ocstr=arg1,i32,ptr|i32|"))
                .unwrap();
        let mut fopen = fopen_def.prep();
        let mut fgets = fgets_def.prep();
        let name = "test.txt2".to_string();
        let mode = "r".to_string();
        fopen.push_arg(&name).unwrap();
        fopen.push_arg(&mode).unwrap();
        let ret = fopen.call().unwrap();
        println!("fopen ret={:x}", ret.as_u64().unwrap());
        let mut buffer: String = String::with_capacity(100);
        fgets.push_mut_arg(&mut buffer).unwrap();
        fgets.push_arg(&(50i32)).unwrap();
        fgets.push_arg(&ret).unwrap();
        let ret2 = fgets.call().unwrap();
        println!("fgets  ret={:?}", ret2.as_i32().unwrap());
        println!("buffer read: {}", buffer);
    }

    #[test]
    fn test_printf() {
        let printf_def = DynCaller::define_function(&format!(
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
        let ret = printf.call().unwrap();
        println!("printf ret={:?}", ret.as_i32().unwrap());
    }

    #[test]
    fn test_scanf() {
        let instr = "hello world 42\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,ocstr=50|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%s".to_string();
        let mut ans = String::new();
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
    }

    #[test]
    fn test_scanf_num_i32() {
        let instr = "42\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i32|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%d".to_string();
        let mut ans = 0i32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 42);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u32() {
        let instr = "4294967295\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u32|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%u".to_string();
        let mut ans = 0u32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 4_294_967_295);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u64() {
        let instr = "18446744073709551615\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u64|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%llu".to_string();
        let mut ans = 0u64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 18_446_744_073_709_551_615);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i64() {
        let instr = "-9223372036854775808\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i64|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%lld".to_string();
        let mut ans = 0i64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, -9_223_372_036_854_775_808);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_f32() {
        let instr = "3.14\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*f32|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%f".to_string();
        let mut ans = 0f32;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert!((ans - 3.14f32).abs() < 1e-4);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i16() {
        let instr = "32767\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i16|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hd".to_string();
        let mut ans = 0i16;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 32767i16);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u16() {
        let instr = "65535\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u16|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hu".to_string();
        let mut ans = 0u16;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 65535u16);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_i8() {
        let instr = "127\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*i8|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hhd".to_string();
        let mut ans = 0i8;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 127i8);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_u8() {
        let instr = "255\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*u8|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%hhu".to_string();
        let mut ans = 0u8;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert_eq!(ans, 255u8);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_num_f64() {
        let instr = "3.14159\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
            "{LIBC}|sscanf|cstr,cstr,*f64|i32|fixargs=2"
        ))
        .unwrap();
        let format = "%lf".to_string();
        let mut ans = 0f64;
        let mut sscanf = sscanf_def.prep();
        sscanf.push_arg(&instr).unwrap();
        sscanf.push_arg(&format).unwrap();
        sscanf.push_mut_arg(&mut ans).unwrap();
        let ret = sscanf.call().unwrap();
        println!("sscanf ret={:?} ans={}", ret.as_i32().unwrap(), ans);
        assert!((ans - 3.14159).abs() < 1e-9);
        assert_eq!(*ret.as_i32().unwrap(), 1);
    }

    #[test]
    fn test_scanf_string_and_numbers() {
        let instr =
            "hello 42 4294967295 18446744073709551615 -9223372036854775808 3.14159\n".to_string();
        let sscanf_def = DynCaller::define_function(&format!(
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
        let ret = sscanf.call().unwrap();
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
            DynCaller::define_function(&format!("{lib}|sum_pair|{{u32,u32}}|u32|")).unwrap();
        let mut inv = def.prep();
        let mut pair = inv.create_struct(0).unwrap();
        pair.push_field(&10u32).unwrap();
        pair.push_field(&32u32).unwrap();
        inv.push_arg(&pair).unwrap();
        let ret = inv.call().unwrap();
        assert_eq!(*ret.as_u32().unwrap(), 42);
    }

    #[test]
    fn test_struct_pointer_input() {
        let lib = struct_fixture_path();
        let def =
            DynCaller::define_function(&format!("{lib}|sum_pair_ptr|*{{u32,u32}}|u32|"))
                .unwrap();
        let mut inv = def.prep();
        let mut pair = def.create_struct(0).unwrap();
        pair.push_field(&11u32).unwrap();
        pair.push_field(&31u32).unwrap();
        inv.push_mut_arg(&mut pair).unwrap();
        let ret = inv.call().unwrap();
        assert_eq!(*ret.as_u32().unwrap(), 42);
    }

    #[test]
    fn test_struct_pointer_mutation() {
        let lib = struct_fixture_path();
        let def = DynCaller::define_function(&format!("{lib}|bump_pair|*{{u32,u32}}|u32|"))
            .unwrap();
        let mut inv = def.prep();
        let mut pair = def.create_struct(0).unwrap();
        pair.push_field(&7u32).unwrap();
        pair.push_field(&8u32).unwrap();
        inv.push_mut_arg(&mut pair).unwrap();
        let ret = inv.call().unwrap();
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
            DynCaller::define_function(&format!("{LIBC}|mktime|*{tm_desc}|i64|")).unwrap();
        let strftime_def = DynCaller::define_function(&format!(
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
        let mktime_ret = mktime.call().unwrap();
        assert_ne!(*mktime_ret.as_i64().unwrap(), -1);

        let format = "%Y-%m-%d %H:%M:%S".to_string();
        let mut output = String::new();
        let mut strftime = strftime_def.prep();
        strftime.push_mut_arg(&mut output).unwrap();
        strftime.push_arg(&64u64).unwrap();
        strftime.push_arg(&format).unwrap();
        strftime.push_mut_arg(&mut tm).unwrap();
        let strftime_ret = strftime.call().unwrap();

        assert_eq!(*strftime_ret.as_u64().unwrap(), 19);
        assert_eq!(output, "2024-01-02 03:04:05");
        assert_eq!(tm.read_field::<i32>(6).unwrap(), 2);
        assert_eq!(tm.read_field::<i32>(7).unwrap(), 1);
    }

    // ── coerce tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_coerce_i64_into_i32_strict_errors() {
        let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|")).unwrap();
        let mut inv = def.prep();
        assert!(inv.push_arg(&100i64).is_err());
    }

    #[test]
    fn test_coerce_i64_into_i32_with_coerce() {
        let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&(-42i64)).unwrap();
        let result = inv.call().unwrap();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_string_into_cstr_slot() {
        // String → cstr slot: works in both strict and coerce mode
        let def = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&"42".to_string()).unwrap();
        let result = inv.call().unwrap();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_int_into_cstr_slot() {
        // int → cstr: format as decimal
        let def = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&42i32).unwrap();
        let result = inv.call().unwrap();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_string_into_int_slot() {
        // str → int: parse "42" → 42
        let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&"42".to_string()).unwrap();
        let result = inv.call().unwrap();
        assert_eq!(*result.as_i32().unwrap(), 42);
    }

    #[test]
    fn test_coerce_invalid_string_into_int_slot_errors() {
        // str → int parse failure: should error
        let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|coerce")).unwrap();
        let mut inv = def.prep();
        assert!(inv.push_arg(&"hello".to_string()).is_err());
    }

    #[test]
    fn test_coerce_strict_type_mismatch_errors() {
        // Without coerce: i64 for i32 slot → Err
        let def = DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|")).unwrap();
        let mut inv = def.prep();
        assert!(inv.push_arg(&42i64).is_err());
    }

    #[test]
    fn test_coerce_multiple_flags() {
        // fixargs=1,coerce together
        let def = DynCaller::define_function(&format!(
            "{LIBC}|printf|cstr,i32|i32|fixargs=1,coerce"
        ))
        .unwrap();
        assert!(def.is_coerce());
        let mut inv = def.prep();
        inv.push_arg(&"value: %d\n".to_string()).unwrap();
        inv.push_arg(&99i64).unwrap(); // i64 coerced to declared i32
        let ret = inv.call().unwrap();
        assert!(*ret.as_i32().unwrap() > 0);
    }

    #[test]
    fn test_fgets_via_file() {
        // Write a known string to a temp file, then read it back with fgets.
        use crate::ArgVal;
        use std::ffi::c_void;

        let path = std::env::temp_dir().join("dyncall_fgets_test.txt");
        std::fs::write(&path, "hello from fgets\n").unwrap();
        let path_str = path.to_str().unwrap().to_string();

        // fopen(path, "r") -> FILE*
        let fopen_def =
            DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|")).unwrap();
        let mut inv = fopen_def.prep();
        inv.push_arg(&path_str).unwrap();
        inv.push_arg(&"r".to_string()).unwrap();
        let file_val = inv.call().unwrap();
        let fp: *mut c_void = *file_val.as_pointer().unwrap();
        assert!(!fp.is_null(), "fopen failed");

        // fgets(buf, 64, fp) -> ptr
        let fgets_def = DynCaller::define_function(&format!(
            "{LIBC}|fgets|ocstr=arg1,i32,ptr|ptr|"
        ))
        .unwrap();
        let mut inv = fgets_def.prep();
        let mut buf = String::new();
        inv.push_mut_arg(&mut buf).unwrap();
        inv.push_arg(&64i32).unwrap();
        inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
        let ret = inv.call().unwrap();
        assert!(
            !(*ret.as_pointer().unwrap()).is_null(),
            "fgets returned NULL"
        );
        assert_eq!(buf, "hello from fgets\n");

        // fclose(fp)
        let fclose_def =
            DynCaller::define_function(&format!("{LIBC}|fclose|ptr|i32|")).unwrap();
        let mut inv = fclose_def.prep();
        inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
        inv.call().unwrap();

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_stderr_fputs() {
        // Get a FILE* for stderr using the platform idiom, then fputs to it.
        // A non-negative return value from fputs confirms the FILE* was valid.
        use crate::ArgVal;

        #[cfg(target_os = "windows")]
        {
            const UCRT: &str = "ucrtbase.dll";
            let iob_def =
                DynCaller::define_function(&format!("{UCRT}|__acrt_iob_func|u32|ptr|"))
                    .unwrap();
            let mut inv = iob_def.prep();
            inv.push_arg(&2u32).unwrap(); // 2 = stderr
            let fp = *inv.call().unwrap().as_pointer().unwrap();
            assert!(!fp.is_null(), "__acrt_iob_func(2) returned NULL");

            let fputs_def =
                DynCaller::define_function(&format!("{UCRT}|fputs|cstr,ptr|i32|")).unwrap();
            let mut inv = fputs_def.prep();
            inv.push_arg(&"[dyncall stderr test]\n".to_string()).unwrap();
            inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
            assert!(*inv.call().unwrap().as_i32().unwrap() >= 0);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let fopen_def =
                DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|"))
                    .unwrap();
            let mut inv = fopen_def.prep();
            inv.push_arg(&"/dev/stderr".to_string()).unwrap();
            inv.push_arg(&"w".to_string()).unwrap();
            let fp = *inv.call().unwrap().as_pointer().unwrap();
            assert!(!fp.is_null(), "fopen(/dev/stderr) returned NULL");

            let fputs_def =
                DynCaller::define_function(&format!("{LIBC}|fputs|cstr,ptr|i32|")).unwrap();
            let mut inv = fputs_def.prep();
            inv.push_arg(&"[dyncall stderr test]\n".to_string()).unwrap();
            inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
            assert!(*inv.call().unwrap().as_i32().unwrap() >= 0);

            let fclose_def =
                DynCaller::define_function(&format!("{LIBC}|fclose|ptr|i32|")).unwrap();
            let mut inv = fclose_def.prep();
            inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
            inv.call().unwrap();
        }
    }

    #[test]
    fn test_stdout_fflush() {
        // Get a FILE* for stdout and fflush it — should return 0 (success).
        use crate::ArgVal;

        #[cfg(target_os = "windows")]
        {
            const UCRT: &str = "ucrtbase.dll";
            let iob_def =
                DynCaller::define_function(&format!("{UCRT}|__acrt_iob_func|u32|ptr|"))
                    .unwrap();
            let mut inv = iob_def.prep();
            inv.push_arg(&1u32).unwrap(); // 1 = stdout
            let fp = *inv.call().unwrap().as_pointer().unwrap();
            assert!(!fp.is_null(), "__acrt_iob_func(1) returned NULL");

            let fflush_def =
                DynCaller::define_function(&format!("{UCRT}|fflush|ptr|i32|")).unwrap();
            let mut inv = fflush_def.prep();
            inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
            assert_eq!(*inv.call().unwrap().as_i32().unwrap(), 0);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let fopen_def =
                DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|"))
                    .unwrap();
            let mut inv = fopen_def.prep();
            inv.push_arg(&"/dev/stdout".to_string()).unwrap();
            inv.push_arg(&"w".to_string()).unwrap();
            let fp = *inv.call().unwrap().as_pointer().unwrap();
            assert!(!fp.is_null(), "fopen(/dev/stdout) returned NULL");

            let fflush_def =
                DynCaller::define_function(&format!("{LIBC}|fflush|ptr|i32|")).unwrap();
            let mut inv = fflush_def.prep();
            inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
            assert_eq!(*inv.call().unwrap().as_i32().unwrap(), 0);

            let fclose_def =
                DynCaller::define_function(&format!("{LIBC}|fclose|ptr|i32|")).unwrap();
            let mut inv = fclose_def.prep();
            inv.push_arg(&ArgVal::Pointer(fp)).unwrap();
            inv.call().unwrap();
        }
    }

    #[test]
    fn test_errno_on_failed_fopen() {
        // fopen a non-existent file with errno flag; the call should fail (NULL)
        // and errno should be non-zero (ENOENT = 2 on most platforms).
        let def =
            DynCaller::define_function(&format!("{LIBC}|fopen|cstr,cstr|ptr|errno")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&"/this/path/does/not/exist/dyncall_test".to_string())
            .unwrap();
        inv.push_arg(&"r".to_string()).unwrap();
        let result = inv.call().unwrap();
        let errno = inv.last_errno().expect("errno flag set but last_errno() returned None");
        assert!(
            result.as_pointer().map(|p| p.is_null()).unwrap_or(true),
            "expected fopen to fail"
        );
        assert_ne!(errno, 0, "expected non-zero errno after failed fopen");
    }

    #[test]
    fn test_errno_not_captured_without_flag() {
        // Without the errno flag, last_errno() should return None.
        let def =
            DynCaller::define_function(&format!("{LIBC}|abs|i32|i32|")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&(-5i32)).unwrap();
        let result = inv.call().unwrap();
        assert_eq!(*result.as_i32().unwrap(), 5);
        assert_eq!(inv.last_errno(), None, "expected None when errno flag not set");
    }

    #[test]
    fn test_push_too_many_args_errors() {
        // Pushing more args than declared must return Err.
        let def = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut inv = def.prep();
        inv.push_arg(&"42".to_string()).unwrap();
        let extra = inv.push_arg(&"extra".to_string());
        assert!(extra.is_err(), "pushing past declared arg count should fail");
    }

    #[test]
    fn test_call_too_few_args_errors() {
        // Calling with fewer args than declared must return Err.
        let def = DynCaller::define_function(&format!("{LIBC}|atoi|cstr|i32|")).unwrap();
        let mut inv = def.prep();
        // Push no arguments — one is expected.
        let result = inv.call();
        assert!(result.is_err(), "calling with too few args should fail");
    }

    // ── CoerceFromField / CoerceIntoField tests ───────────────────────────────

    #[test]
    fn test_coerce_read_field_exact_types() {
        use crate::structs::StructType;
        use crate::{ArgType, StructValue};
        let st = StructType::new(vec![ArgType::I32, ArgType::F64, ArgType::U16]).unwrap();
        let mut sv = StructValue::from_struct_type(&st);
        sv.push_field(&42i32).unwrap();
        sv.push_field(&3.14f64).unwrap();
        sv.push_field(&7u16).unwrap();

        // f64 coerced read — no loss for these values
        assert_eq!(sv.read_field_coerced::<f64>(0).unwrap(), 42.0);
        assert!((sv.read_field_coerced::<f64>(1).unwrap() - 3.14).abs() < 1e-9);
        assert_eq!(sv.read_field_coerced::<f64>(2).unwrap(), 7.0);

        // i64 coerced read
        assert_eq!(sv.read_field_coerced::<i64>(0).unwrap(), 42);
        assert_eq!(sv.read_field_coerced::<i64>(2).unwrap(), 7);
    }

    #[test]
    fn test_coerce_read_all_numeric_types_as_f64() {
        use crate::structs::StructType;
        use crate::{ArgType, StructValue};
        let st = StructType::new(vec![
            ArgType::Char, ArgType::I16, ArgType::U16,
            ArgType::I32,  ArgType::U32,
            ArgType::I64,  ArgType::U64,
            ArgType::F32,  ArgType::F64,
        ]).unwrap();
        let mut sv = StructValue::from_struct_type(&st);
        sv.push_field(&1u8).unwrap();
        sv.push_field(&2i16).unwrap();
        sv.push_field(&3u16).unwrap();
        sv.push_field(&4i32).unwrap();
        sv.push_field(&5u32).unwrap();
        sv.push_field(&6i64).unwrap();
        sv.push_field(&7u64).unwrap();
        sv.push_field(&8.0f32).unwrap();
        sv.push_field(&9.0f64).unwrap();

        for (i, expected) in (1u32..=9).enumerate() {
            assert_eq!(sv.read_field_coerced::<f64>(i).unwrap(), expected as f64,
                "field {i} mismatch");
        }
    }

    #[test]
    fn test_coerce_write_field_i64_into_i32() {
        use crate::structs::StructType;
        use crate::{ArgType, StructValue};
        let st = StructType::new(vec![ArgType::I32]).unwrap();
        let mut sv = StructValue::from_struct_type(&st);
        sv.push_field_coerced(&100i64).unwrap();
        assert_eq!(sv.read_field::<i32>(0).unwrap(), 100);
    }

    #[test]
    fn test_coerce_write_field_f64_into_i32() {
        use crate::structs::StructType;
        use crate::{ArgType, StructValue};
        let st = StructType::new(vec![ArgType::I32, ArgType::F32]).unwrap();
        let mut sv = StructValue::from_struct_type(&st);
        sv.push_field_coerced(&-7.9f64).unwrap();  // truncates to -7
        sv.push_field_coerced(&1.5f64).unwrap();   // stays as f32
        assert_eq!(sv.read_field::<i32>(0).unwrap(), -7);
        assert!((sv.read_field::<f32>(1).unwrap() - 1.5).abs() < 1e-6);
    }

    // ── Linux-only tests ──────────────────────────────────────────────────────

    /// `getuid()` and `getgid()` take no arguments and return a `u32`.
    /// We can't assert specific values, but both must return successfully and
    /// agree with Rust's own `std::os::unix::process` view of the process.
    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_getuid_getgid() {
        let uid_def = DynCaller::define_function(&format!("{LIBC}|getuid||u32|")).unwrap();
        let gid_def = DynCaller::define_function(&format!("{LIBC}|getgid||u32|")).unwrap();

        let uid = *uid_def.prep().call().unwrap().as_u32().unwrap();
        let gid = *gid_def.prep().call().unwrap().as_u32().unwrap();

        // Cross-check against the libc crate's view of the same values.
        assert_eq!(uid, unsafe { libc::getuid() });
        assert_eq!(gid, unsafe { libc::getgid() });
        println!("uid={uid} gid={gid}");
    }

    /// `gettimeofday(struct timeval *, NULL)` fills a two-field struct:
    ///   `tv_sec`  (i64) — seconds since epoch
    ///   `tv_usec` (i64) — microseconds (0 ≤ x < 1_000_000)
    ///
    /// This exercises struct-by-pointer output with a Linux-only POSIX call.
    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_gettimeofday() {
        // struct timeval { time_t tv_sec; suseconds_t tv_usec; }
        // Both fields are 64-bit on x86-64 Linux.
        let def = DynCaller::define_function(
            &format!("{LIBC}|gettimeofday|*{{i64,i64}},ptr|i32|")
        ).unwrap();

        let mut tv = def.create_struct(0).unwrap();
        tv.push_field(&0i64).unwrap(); // tv_sec
        tv.push_field(&0i64).unwrap(); // tv_usec

        let mut inv = def.prep();
        inv.push_mut_arg(&mut tv).unwrap();
        inv.push_arg(&crate::ArgVal::Pointer(std::ptr::null_mut())).unwrap();
        let ret = inv.call().unwrap();

        assert_eq!(*ret.as_i32().unwrap(), 0, "gettimeofday failed");

        let tv_sec  = tv.read_field::<i64>(0).unwrap();
        let tv_usec = tv.read_field::<i64>(1).unwrap();
        println!("tv_sec={tv_sec} tv_usec={tv_usec}");

        // Sanity: seconds since epoch should be somewhere after 2020.
        assert!(tv_sec > 1_577_836_800, "tv_sec looks wrong: {tv_sec}");
        assert!((0..1_000_000).contains(&tv_usec), "tv_usec out of range: {tv_usec}");
    }

    /// `uname(struct utsname *)` fills five fixed-length char arrays (each 65 bytes):
    ///   sysname, nodename, release, version, machine.
    ///
    /// We map each field as a byte buffer of 65 bytes, then decode the first
    /// null-terminated string from it.  Returns 0 on success.
    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_uname() {
        // struct utsname has 6 fields of [char; 65] on Linux x86-64
        // (sysname, nodename, release, version, machine, domainname).
        // We represent each as obuff=65 (fixed-size output byte buffer).
        let def = DynCaller::define_function(
            &format!("{LIBC}|uname|*{{obuff=65,obuff=65,obuff=65,obuff=65,obuff=65,obuff=65}}|i32|")
        );

        // uname takes a plain pointer to the struct, not a struct-by-value arg.
        // Use a raw byte buffer instead.
        use crate::ArgVal;
        let uname_def = DynCaller::define_function(
            &format!("{LIBC}|uname|ptr|i32|")
        ).unwrap();

        let mut buf = vec![0u8; 6 * 65];
        let mut inv = uname_def.prep();
        inv.push_arg(&ArgVal::Pointer(buf.as_mut_ptr() as *mut std::ffi::c_void)).unwrap();
        let ret = inv.call().unwrap();
        assert_eq!(*ret.as_i32().unwrap(), 0, "uname failed");

        // Each field starts at offset n*65; read until the first NUL.
        let field = |n: usize| -> String {
            let start = n * 65;
            let slice = &buf[start..start + 65];
            let end = slice.iter().position(|&b| b == 0).unwrap_or(65);
            String::from_utf8_lossy(&slice[..end]).into_owned()
        };

        let sysname  = field(0);
        let nodename = field(1);
        let release  = field(2);
        let machine  = field(4);

        println!("sysname={sysname} nodename={nodename} release={release} machine={machine}");

        assert_eq!(sysname, "Linux", "expected sysname=Linux, got {sysname}");
        assert!(!nodename.is_empty(), "nodename should not be empty");
        assert!(!release.is_empty(),  "release should not be empty");
        assert_eq!(machine, "x86_64", "expected machine=x86_64, got {machine}");

        // Suppress unused-variable warning from the struct-descriptor attempt.
        let _ = def;
    }
}
