//! The handful of Win32 declarations this crate needs, kept local instead of pulling in
//! a large bindings crate.

use std::ffi::c_void;

pub type Handle = isize;
pub const INVALID_HANDLE_VALUE: Handle = -1;
pub const TH32CS_SNAPPROCESS: u32 = 0x2;

pub const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
pub const AF_INET: u32 = 2;
pub const AF_INET6: u32 = 23;
pub const TCP_TABLE_OWNER_PID_ALL: i32 = 5;
pub const UDP_TABLE_OWNER_PID: i32 = 1;

#[repr(C)]
pub struct ProcessEntry32W {
    pub dw_size: u32,
    pub cnt_usage: u32,
    pub th32_process_id: u32,
    pub th32_default_heap_id: usize,
    pub th32_module_id: u32,
    pub cnt_threads: u32,
    pub th32_parent_process_id: u32,
    pub pc_pri_class_base: i32,
    pub dw_flags: u32,
    pub sz_exe_file: [u16; 260],
}

#[link(name = "kernel32")]
extern "system" {
    pub fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
    pub fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
    pub fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
    pub fn CloseHandle(handle: Handle) -> i32;
    pub fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
}

#[link(name = "iphlpapi")]
extern "system" {
    pub fn GetExtendedTcpTable(
        table: *mut c_void,
        size: *mut u32,
        order: i32,
        family: u32,
        class: i32,
        reserved: u32,
    ) -> u32;
    pub fn GetExtendedUdpTable(
        table: *mut c_void,
        size: *mut u32,
        order: i32,
        family: u32,
        class: i32,
        reserved: u32,
    ) -> u32;
}
