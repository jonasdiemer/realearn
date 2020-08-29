/* automatically generated by rust-bindgen */

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

#[allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
pub mod root {
    #[allow(unused_imports)]
    use self::super::root;
    pub mod std {
        #[allow(unused_imports)]
        use self::super::super::root;
    }
    pub type INT_PTR = ::std::os::raw::c_longlong;
    pub type EEL_F = f64;
    extern "C" {
        pub fn NSEEL_HOSTSTUB_EnterMutex();
    }
    extern "C" {
        pub fn NSEEL_HOSTSTUB_LeaveMutex();
    }
    extern "C" {
        pub fn NSEEL_init() -> ::std::os::raw::c_int;
    }
    extern "C" {
        pub fn NSEEL_quit();
    }
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct functionType {
        _unused: [u8; 0],
    }
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct eel_function_table {
        pub list: *mut root::functionType,
        pub list_size: ::std::os::raw::c_int,
    }
    #[test]
    fn bindgen_test_layout_eel_function_table() {
        assert_eq!(
            ::std::mem::size_of::<eel_function_table>(),
            16usize,
            concat!("Size of: ", stringify!(eel_function_table))
        );
        assert_eq!(
            ::std::mem::align_of::<eel_function_table>(),
            8usize,
            concat!("Alignment of ", stringify!(eel_function_table))
        );
        assert_eq!(
            unsafe { &(*(::std::ptr::null::<eel_function_table>())).list as *const _ as usize },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(eel_function_table),
                "::",
                stringify!(list)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<eel_function_table>())).list_size as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(eel_function_table),
                "::",
                stringify!(list_size)
            )
        );
    }
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct _compileContext {
        _unused: [u8; 0],
    }
    pub type NSEEL_PPPROC = ::std::option::Option<
        unsafe extern "C" fn(
            data: *mut ::std::os::raw::c_void,
            data_size: ::std::os::raw::c_int,
            userfunc_data: *mut root::_compileContext,
        ) -> *mut ::std::os::raw::c_void,
    >;
    extern "C" {
        pub fn NSEEL_addfunctionex2(
            name: *const ::std::os::raw::c_char,
            nparms: ::std::os::raw::c_int,
            code_startaddr: *mut ::std::os::raw::c_char,
            code_len: ::std::os::raw::c_int,
            pproc: root::NSEEL_PPPROC,
            fptr: *mut ::std::os::raw::c_void,
            fptr2: *mut ::std::os::raw::c_void,
            destination: *mut root::eel_function_table,
        );
    }
    extern "C" {
        pub fn NSEEL_addfunc_ret_type(
            name: *const ::std::os::raw::c_char,
            np: ::std::os::raw::c_int,
            ret_type: ::std::os::raw::c_int,
            pproc: root::NSEEL_PPPROC,
            fptr: *mut ::std::os::raw::c_void,
            destination: *mut root::eel_function_table,
        );
    }
    extern "C" {
        pub fn NSEEL_addfunc_varparm_ex(
            name: *const ::std::os::raw::c_char,
            min_np: ::std::os::raw::c_int,
            want_exact: ::std::os::raw::c_int,
            pproc: root::NSEEL_PPPROC,
            fptr: ::std::option::Option<
                unsafe extern "C" fn(
                    arg1: *mut ::std::os::raw::c_void,
                    arg2: root::INT_PTR,
                    arg3: *mut *mut root::EEL_F,
                ) -> root::EEL_F,
            >,
            destination: *mut root::eel_function_table,
        );
    }
    extern "C" {
        pub fn NSEEL_getstats() -> *mut ::std::os::raw::c_int;
    }
    pub type NSEEL_VMCTX = *mut ::std::os::raw::c_void;
    pub type NSEEL_CODEHANDLE = *mut ::std::os::raw::c_void;
    extern "C" {
        pub fn NSEEL_VM_alloc() -> root::NSEEL_VMCTX;
    }
    extern "C" {
        pub fn NSEEL_VM_free(ctx: root::NSEEL_VMCTX);
    }
    extern "C" {
        pub fn NSEEL_VM_SetFunctionTable(
            arg1: root::NSEEL_VMCTX,
            tab: *mut root::eel_function_table,
        );
    }
    extern "C" {
        pub fn NSEEL_VM_SetFunctionValidator(
            arg1: root::NSEEL_VMCTX,
            validateFunc: ::std::option::Option<
                unsafe extern "C" fn(
                    fn_name: *const ::std::os::raw::c_char,
                    user: *mut ::std::os::raw::c_void,
                ) -> *const ::std::os::raw::c_char,
            >,
            user: *mut ::std::os::raw::c_void,
        );
    }
    extern "C" {
        pub fn NSEEL_VM_remove_unused_vars(_ctx: root::NSEEL_VMCTX);
    }
    extern "C" {
        pub fn NSEEL_VM_clear_var_refcnts(_ctx: root::NSEEL_VMCTX);
    }
    extern "C" {
        pub fn NSEEL_VM_remove_all_nonreg_vars(_ctx: root::NSEEL_VMCTX);
    }
    extern "C" {
        pub fn NSEEL_VM_enumallvars(
            ctx: root::NSEEL_VMCTX,
            func: ::std::option::Option<
                unsafe extern "C" fn(
                    name: *const ::std::os::raw::c_char,
                    val: *mut root::EEL_F,
                    ctx: *mut ::std::os::raw::c_void,
                ) -> ::std::os::raw::c_int,
            >,
            userctx: *mut ::std::os::raw::c_void,
        );
    }
    extern "C" {
        pub fn NSEEL_VM_regvar(
            ctx: root::NSEEL_VMCTX,
            name: *const ::std::os::raw::c_char,
        ) -> *mut root::EEL_F;
    }
    extern "C" {
        pub fn NSEEL_VM_getvar(
            ctx: root::NSEEL_VMCTX,
            name: *const ::std::os::raw::c_char,
        ) -> *mut root::EEL_F;
    }
    extern "C" {
        pub fn NSEEL_VM_get_var_refcnt(
            _ctx: root::NSEEL_VMCTX,
            name: *const ::std::os::raw::c_char,
        ) -> ::std::os::raw::c_int;
    }
    extern "C" {
        pub fn NSEEL_VM_set_var_resolver(
            ctx: root::NSEEL_VMCTX,
            res: ::std::option::Option<
                unsafe extern "C" fn(
                    userctx: *mut ::std::os::raw::c_void,
                    name: *const ::std::os::raw::c_char,
                ) -> *mut root::EEL_F,
            >,
            userctx: *mut ::std::os::raw::c_void,
        );
    }
    extern "C" {
        pub fn NSEEL_VM_freeRAM(ctx: root::NSEEL_VMCTX);
    }
    extern "C" {
        pub fn NSEEL_VM_freeRAMIfCodeRequested(arg1: root::NSEEL_VMCTX);
    }
    extern "C" {
        pub fn NSEEL_VM_wantfreeRAM(ctx: root::NSEEL_VMCTX) -> ::std::os::raw::c_int;
    }
    extern "C" {
        pub fn NSEEL_VM_SetGRAM(ctx: root::NSEEL_VMCTX, gram: *mut *mut ::std::os::raw::c_void);
    }
    extern "C" {
        pub fn NSEEL_VM_FreeGRAM(ufd: *mut *mut ::std::os::raw::c_void);
    }
    extern "C" {
        pub fn NSEEL_VM_SetCustomFuncThis(
            ctx: root::NSEEL_VMCTX,
            thisptr: *mut ::std::os::raw::c_void,
        );
    }
    extern "C" {
        pub fn NSEEL_VM_getramptr(
            ctx: root::NSEEL_VMCTX,
            offs: ::std::os::raw::c_uint,
            validCount: *mut ::std::os::raw::c_int,
        ) -> *mut root::EEL_F;
    }
    extern "C" {
        pub fn NSEEL_VM_getramptr_noalloc(
            ctx: root::NSEEL_VMCTX,
            offs: ::std::os::raw::c_uint,
            validCount: *mut ::std::os::raw::c_int,
        ) -> *mut root::EEL_F;
    }
    extern "C" {
        pub fn NSEEL_VM_setramsize(
            ctx: root::NSEEL_VMCTX,
            maxent: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int;
    }
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct eelStringSegmentRec {
        pub _next: *mut root::eelStringSegmentRec,
        pub str_start: *const ::std::os::raw::c_char,
        pub str_len: ::std::os::raw::c_int,
    }
    #[test]
    fn bindgen_test_layout_eelStringSegmentRec() {
        assert_eq!(
            ::std::mem::size_of::<eelStringSegmentRec>(),
            24usize,
            concat!("Size of: ", stringify!(eelStringSegmentRec))
        );
        assert_eq!(
            ::std::mem::align_of::<eelStringSegmentRec>(),
            8usize,
            concat!("Alignment of ", stringify!(eelStringSegmentRec))
        );
        assert_eq!(
            unsafe { &(*(::std::ptr::null::<eelStringSegmentRec>()))._next as *const _ as usize },
            0usize,
            concat!(
                "Offset of field: ",
                stringify!(eelStringSegmentRec),
                "::",
                stringify!(_next)
            )
        );
        assert_eq!(
            unsafe {
                &(*(::std::ptr::null::<eelStringSegmentRec>())).str_start as *const _ as usize
            },
            8usize,
            concat!(
                "Offset of field: ",
                stringify!(eelStringSegmentRec),
                "::",
                stringify!(str_start)
            )
        );
        assert_eq!(
            unsafe { &(*(::std::ptr::null::<eelStringSegmentRec>())).str_len as *const _ as usize },
            16usize,
            concat!(
                "Offset of field: ",
                stringify!(eelStringSegmentRec),
                "::",
                stringify!(str_len)
            )
        );
    }
    extern "C" {
        pub fn NSEEL_VM_SetStringFunc(
            ctx: root::NSEEL_VMCTX,
            onString: ::std::option::Option<
                unsafe extern "C" fn(
                    caller_this: *mut ::std::os::raw::c_void,
                    list: *mut root::eelStringSegmentRec,
                ) -> root::EEL_F,
            >,
            onNamedString: ::std::option::Option<
                unsafe extern "C" fn(
                    caller_this: *mut ::std::os::raw::c_void,
                    name: *const ::std::os::raw::c_char,
                ) -> root::EEL_F,
            >,
        );
    }
    extern "C" {
        pub fn NSEEL_code_compile(
            ctx: root::NSEEL_VMCTX,
            code: *const ::std::os::raw::c_char,
            lineoffs: ::std::os::raw::c_int,
        ) -> root::NSEEL_CODEHANDLE;
    }
    extern "C" {
        pub fn NSEEL_code_compile_ex(
            ctx: root::NSEEL_VMCTX,
            code: *const ::std::os::raw::c_char,
            lineoffs: ::std::os::raw::c_int,
            flags: ::std::os::raw::c_int,
        ) -> root::NSEEL_CODEHANDLE;
    }
    extern "C" {
        pub fn NSEEL_code_getcodeerror(ctx: root::NSEEL_VMCTX) -> *mut ::std::os::raw::c_char;
    }
    extern "C" {
        pub fn NSEEL_code_geterror_flag(ctx: root::NSEEL_VMCTX) -> ::std::os::raw::c_int;
    }
    extern "C" {
        pub fn NSEEL_code_execute(code: root::NSEEL_CODEHANDLE);
    }
    extern "C" {
        pub fn NSEEL_code_free(code: root::NSEEL_CODEHANDLE);
    }
    extern "C" {
        pub fn NSEEL_code_getstats(code: root::NSEEL_CODEHANDLE) -> *mut ::std::os::raw::c_int;
    }
}
