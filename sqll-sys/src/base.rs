pub const SQLITE_OK: ::core::ffi::c_int = 0;
pub const SQLITE_ROW: ::core::ffi::c_int = 100;
pub const SQLITE_DONE: ::core::ffi::c_int = 101;
pub const SQLITE_OPEN_READONLY: ::core::ffi::c_int = 1;
pub const SQLITE_OPEN_READWRITE: ::core::ffi::c_int = 2;
pub const SQLITE_OPEN_CREATE: ::core::ffi::c_int = 4;
pub const SQLITE_OPEN_URI: ::core::ffi::c_int = 64;
pub const SQLITE_OPEN_MEMORY: ::core::ffi::c_int = 128;
pub const SQLITE_OPEN_NOMUTEX: ::core::ffi::c_int = 32768;
pub const SQLITE_OPEN_FULLMUTEX: ::core::ffi::c_int = 65536;
pub const SQLITE_OPEN_SHAREDCACHE: ::core::ffi::c_int = 131072;
pub const SQLITE_OPEN_PRIVATECACHE: ::core::ffi::c_int = 262144;
pub const SQLITE_OPEN_NOFOLLOW: ::core::ffi::c_int = 16777216;
pub const SQLITE_OPEN_EXRESCODE: ::core::ffi::c_int = 33554432;
pub const SQLITE_PREPARE_PERSISTENT: ::core::ffi::c_int = 1;
pub const SQLITE_PREPARE_NORMALIZE: ::core::ffi::c_int = 2;
pub const SQLITE_PREPARE_NO_VTAB: ::core::ffi::c_int = 4;
pub const SQLITE_INTEGER: ::core::ffi::c_int = 1;
pub const SQLITE_FLOAT: ::core::ffi::c_int = 2;
pub const SQLITE_BLOB: ::core::ffi::c_int = 4;
pub const SQLITE_NULL: ::core::ffi::c_int = 5;
pub const SQLITE_TEXT: ::core::ffi::c_int = 3;
unsafe extern "C" {
    pub fn sqlite3_libversion() -> *const ::core::ffi::c_char;
}
unsafe extern "C" {
    pub fn sqlite3_libversion_number() -> ::core::ffi::c_int;
}
#[repr(C)]
pub struct sqlite3 {
    _unused: [u8; 0],
}
pub type sqlite_int64 = ::core::ffi::c_longlong;
pub type sqlite3_int64 = sqlite_int64;
unsafe extern "C" {
    pub fn sqlite3_close_v2(arg1: *mut sqlite3) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_extended_result_codes(
        arg1: *mut sqlite3,
        onoff: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_last_insert_rowid(arg1: *mut sqlite3) -> sqlite3_int64;
}
unsafe extern "C" {
    pub fn sqlite3_changes(arg1: *mut sqlite3) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_total_changes(arg1: *mut sqlite3) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_busy_handler(
        arg1: *mut sqlite3,
        arg2: ::core::option::Option<
            unsafe extern "C" fn(
                arg1: *mut ::core::ffi::c_void,
                arg2: ::core::ffi::c_int,
            ) -> ::core::ffi::c_int,
        >,
        arg3: *mut ::core::ffi::c_void,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_busy_timeout(arg1: *mut sqlite3, ms: ::core::ffi::c_int) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_malloc(arg1: ::core::ffi::c_int) -> *mut ::core::ffi::c_void;
}
unsafe extern "C" {
    pub fn sqlite3_free(arg1: *mut ::core::ffi::c_void);
}
unsafe extern "C" {
    pub fn sqlite3_open_v2(
        filename: *const ::core::ffi::c_char,
        ppDb: *mut *mut sqlite3,
        flags: ::core::ffi::c_int,
        zVfs: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_errmsg(arg1: *mut sqlite3) -> *const ::core::ffi::c_char;
}
unsafe extern "C" {
    pub fn sqlite3_errstr(arg1: ::core::ffi::c_int) -> *const ::core::ffi::c_char;
}
#[repr(C)]
pub struct sqlite3_stmt {
    _unused: [u8; 0],
}
unsafe extern "C" {
    pub fn sqlite3_prepare_v3(
        db: *mut sqlite3,
        zSql: *const ::core::ffi::c_char,
        nByte: ::core::ffi::c_int,
        prepFlags: ::core::ffi::c_uint,
        ppStmt: *mut *mut sqlite3_stmt,
        pzTail: *mut *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_bind_blob(
        arg1: *mut sqlite3_stmt,
        arg2: ::core::ffi::c_int,
        arg3: *const ::core::ffi::c_void,
        n: ::core::ffi::c_int,
        arg4: ::core::option::Option<unsafe extern "C" fn(arg1: *mut ::core::ffi::c_void)>,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_bind_double(
        arg1: *mut sqlite3_stmt,
        arg2: ::core::ffi::c_int,
        arg3: f64,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_bind_int64(
        arg1: *mut sqlite3_stmt,
        arg2: ::core::ffi::c_int,
        arg3: sqlite3_int64,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_bind_null(
        arg1: *mut sqlite3_stmt,
        arg2: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_bind_text(
        arg1: *mut sqlite3_stmt,
        arg2: ::core::ffi::c_int,
        arg3: *const ::core::ffi::c_char,
        arg4: ::core::ffi::c_int,
        arg5: ::core::option::Option<unsafe extern "C" fn(arg1: *mut ::core::ffi::c_void)>,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_bind_parameter_name(
        arg1: *mut sqlite3_stmt,
        arg2: ::core::ffi::c_int,
    ) -> *const ::core::ffi::c_char;
}
unsafe extern "C" {
    pub fn sqlite3_bind_parameter_index(
        arg1: *mut sqlite3_stmt,
        zName: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_clear_bindings(arg1: *mut sqlite3_stmt) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_column_count(pStmt: *mut sqlite3_stmt) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_column_name(
        arg1: *mut sqlite3_stmt,
        N: ::core::ffi::c_int,
    ) -> *const ::core::ffi::c_char;
}
unsafe extern "C" {
    pub fn sqlite3_step(arg1: *mut sqlite3_stmt) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_column_blob(
        arg1: *mut sqlite3_stmt,
        iCol: ::core::ffi::c_int,
    ) -> *const ::core::ffi::c_void;
}
unsafe extern "C" {
    pub fn sqlite3_column_double(arg1: *mut sqlite3_stmt, iCol: ::core::ffi::c_int) -> f64;
}
unsafe extern "C" {
    pub fn sqlite3_column_int64(arg1: *mut sqlite3_stmt, iCol: ::core::ffi::c_int)
    -> sqlite3_int64;
}
unsafe extern "C" {
    pub fn sqlite3_column_text(
        arg1: *mut sqlite3_stmt,
        iCol: ::core::ffi::c_int,
    ) -> *const ::core::ffi::c_uchar;
}
unsafe extern "C" {
    pub fn sqlite3_column_bytes(
        arg1: *mut sqlite3_stmt,
        iCol: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_column_type(
        arg1: *mut sqlite3_stmt,
        iCol: ::core::ffi::c_int,
    ) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_finalize(pStmt: *mut sqlite3_stmt) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_reset(pStmt: *mut sqlite3_stmt) -> ::core::ffi::c_int;
}
unsafe extern "C" {
    pub fn sqlite3_db_handle(arg1: *mut sqlite3_stmt) -> *mut sqlite3;
}
unsafe extern "C" {
    pub fn sqlite3_db_readonly(
        db: *mut sqlite3,
        zDbName: *const ::core::ffi::c_char,
    ) -> ::core::ffi::c_int;
}
