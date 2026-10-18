//! Runtime binding to Npcap's `wpcap.dll`.
//!
//! The DLL is loaded at runtime from `%SystemRoot%\System32\Npcap`, so building needs no
//! Npcap SDK. The loader only searches that folder and System32, so a stray `wpcap.dll`
//! or `Packet.dll` next to the exe or in the working directory is never picked up.

use std::ffi::{c_char, c_int, c_long, c_uint, CStr, CString, OsString};
use std::net::IpAddr;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use libloading::os::windows::Library;

use crate::win;

const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x0000_0100;
const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
const PCAP_ERRBUF_SIZE: usize = 256;
const PCAP_IF_LOOPBACK: c_uint = 0x1;
const PCAP_NETMASK_UNKNOWN: c_uint = 0xffff_ffff;

/// Large enough for outbound packets captured before large-send offload splits them.
const SNAPLEN: c_int = 262_144;
const READ_TIMEOUT_MS: c_int = 100;
const KERNEL_BUFFER_BYTES: c_int = 16 << 20;

#[repr(C)]
pub struct PcapT {
    _opaque: [u8; 0],
}

#[repr(C)]
struct PcapIf {
    next: *mut PcapIf,
    name: *const c_char,
    description: *const c_char,
    addresses: *mut PcapAddr,
    flags: c_uint,
}

#[repr(C)]
struct PcapAddr {
    next: *mut PcapAddr,
    addr: *const u8,
    netmask: *const u8,
    broadaddr: *const u8,
    dstaddr: *const u8,
}

#[repr(C)]
struct Timeval {
    tv_sec: c_long,
    tv_usec: c_long,
}

#[repr(C)]
struct PktHdr {
    ts: Timeval,
    caplen: u32,
    len: u32,
}

#[repr(C)]
struct BpfProgram {
    bf_len: c_uint,
    bf_insns: *mut u8,
}

/// `struct pcap_stat`, with room for the extra Windows-only counters.
#[repr(C)]
#[derive(Default)]
struct PcapStat {
    ps_recv: c_uint,
    ps_drop: c_uint,
    ps_ifdrop: c_uint,
    _windows_extra: [c_uint; 5],
}

type FindAllDevs = unsafe extern "C" fn(*mut *mut PcapIf, *mut c_char) -> c_int;
type FreeAllDevs = unsafe extern "C" fn(*mut PcapIf);
type Create = unsafe extern "C" fn(*const c_char, *mut c_char) -> *mut PcapT;
type SetInt = unsafe extern "C" fn(*mut PcapT, c_int) -> c_int;
type HandleInt = unsafe extern "C" fn(*mut PcapT) -> c_int;
type Compile = unsafe extern "C" fn(*mut PcapT, *mut BpfProgram, *const c_char, c_int, c_uint) -> c_int;
type SetFilter = unsafe extern "C" fn(*mut PcapT, *mut BpfProgram) -> c_int;
type FreeCode = unsafe extern "C" fn(*mut BpfProgram);
type NextEx = unsafe extern "C" fn(*mut PcapT, *mut *mut PktHdr, *mut *const u8) -> c_int;
type GetErr = unsafe extern "C" fn(*mut PcapT) -> *const c_char;
type Close = unsafe extern "C" fn(*mut PcapT);
type Stats = unsafe extern "C" fn(*mut PcapT, *mut PcapStat) -> c_int;
type LibVersion = unsafe extern "C" fn() -> *const c_char;

pub struct Npcap {
    findalldevs: FindAllDevs,
    freealldevs: FreeAllDevs,
    create: Create,
    set_snaplen: SetInt,
    set_promisc: SetInt,
    set_timeout: SetInt,
    set_immediate_mode: SetInt,
    set_buffer_size: SetInt,
    activate: HandleInt,
    datalink: HandleInt,
    compile: Compile,
    setfilter: SetFilter,
    freecode: FreeCode,
    next_ex: NextEx,
    geterr: GetErr,
    close: Close,
    stats: Stats,
    lib_version: LibVersion,
    // Keeps the function pointers above valid; declared last so it drops last.
    _lib: Library,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub description: String,
    pub addrs: Vec<IpAddr>,
    pub loopback: bool,
}

impl Device {
    pub fn label(&self) -> &str {
        if self.description.is_empty() {
            &self.name
        } else {
            &self.description
        }
    }
}

macro_rules! sym {
    ($lib:expr, $name:literal) => {{
        // SAFETY: the declared type matches the libpcap prototype for this symbol.
        let s = unsafe { $lib.get(concat!($name, "\0").as_bytes()) }
            .with_context(|| format!("wpcap.dll does not export {}", $name))?;
        *s
    }};
}

impl Npcap {
    pub fn load() -> Result<Self> {
        let dll = npcap_dir()?.join("wpcap.dll");
        if !dll.exists() {
            bail!(
                "Npcap is not installed ({} not found). Install it from https://npcap.com",
                dll.display()
            );
        }
        // SAFETY: loading Npcap's own DLL from its system folder; search path restricted.
        let lib = unsafe {
            Library::load_with_flags(
                &dll,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .with_context(|| format!("loading {}", dll.display()))?;

        Ok(Self {
            findalldevs: sym!(lib, "pcap_findalldevs"),
            freealldevs: sym!(lib, "pcap_freealldevs"),
            create: sym!(lib, "pcap_create"),
            set_snaplen: sym!(lib, "pcap_set_snaplen"),
            set_promisc: sym!(lib, "pcap_set_promisc"),
            set_timeout: sym!(lib, "pcap_set_timeout"),
            set_immediate_mode: sym!(lib, "pcap_set_immediate_mode"),
            set_buffer_size: sym!(lib, "pcap_set_buffer_size"),
            activate: sym!(lib, "pcap_activate"),
            datalink: sym!(lib, "pcap_datalink"),
            compile: sym!(lib, "pcap_compile"),
            setfilter: sym!(lib, "pcap_setfilter"),
            freecode: sym!(lib, "pcap_freecode"),
            next_ex: sym!(lib, "pcap_next_ex"),
            geterr: sym!(lib, "pcap_geterr"),
            close: sym!(lib, "pcap_close"),
            stats: sym!(lib, "pcap_stats"),
            lib_version: sym!(lib, "pcap_lib_version"),
            _lib: lib,
        })
    }

    pub fn version(&self) -> String {
        // SAFETY: returns a static NUL-terminated string.
        cstr(unsafe { (self.lib_version)() })
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let mut err = [0 as c_char; PCAP_ERRBUF_SIZE];
        let mut head: *mut PcapIf = ptr::null_mut();
        // SAFETY: Npcap allocates the list; we walk it read-only and free it once.
        unsafe {
            if (self.findalldevs)(&mut head, err.as_mut_ptr()) != 0 {
                bail!("listing capture devices failed: {}", errbuf(&err));
            }
            let mut out = Vec::new();
            let mut cur = head;
            while !cur.is_null() {
                let d = &*cur;
                let mut addrs = Vec::new();
                let mut a = d.addresses;
                while !a.is_null() {
                    if let Some(ip) = sockaddr_ip((*a).addr) {
                        addrs.push(ip);
                    }
                    a = (*a).next;
                }
                out.push(Device {
                    name: cstr(d.name),
                    description: cstr(d.description),
                    addrs,
                    loopback: d.flags & PCAP_IF_LOOPBACK != 0,
                });
                cur = d.next;
            }
            if !head.is_null() {
                (self.freealldevs)(head);
            }
            Ok(out)
        }
    }

    /// Opens a live, non-promiscuous capture on `device` with a BPF `filter`.
    pub fn open(self: &Arc<Self>, device: &str, filter: &str) -> Result<Capture> {
        let name = CString::new(device)?;
        let mut err = [0 as c_char; PCAP_ERRBUF_SIZE];
        // SAFETY: plain libpcap create/configure/activate sequence on a fresh handle.
        let p = unsafe { (self.create)(name.as_ptr(), err.as_mut_ptr()) };
        if p.is_null() {
            bail!("pcap_create({device}) failed: {}", errbuf(&err));
        }
        let mut cap = Capture {
            np: Arc::clone(self),
            p,
            linktype: 0,
        };
        unsafe {
            (self.set_snaplen)(p, SNAPLEN);
            (self.set_promisc)(p, 0);
            (self.set_timeout)(p, READ_TIMEOUT_MS);
            (self.set_immediate_mode)(p, 1);
            (self.set_buffer_size)(p, KERNEL_BUFFER_BYTES);
            let rc = (self.activate)(p);
            // Positive values are warnings; the handle is still usable.
            if rc < 0 {
                bail!("starting capture on {device} failed ({rc}): {}", cap.last_error());
            }
            cap.linktype = (self.datalink)(p);
        }
        cap.set_filter(filter)?;
        Ok(cap)
    }
}

pub struct Capture {
    np: Arc<Npcap>,
    p: *mut PcapT,
    linktype: i32,
}

// SAFETY: a pcap handle may move between threads as long as only one uses it at a time,
// which `&mut self` on every call enforces.
unsafe impl Send for Capture {}

pub struct RawPacket<'a> {
    pub ts_us: u64,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureStats {
    pub received: u64,
    pub dropped: u64,
}

impl Capture {
    pub fn linktype(&self) -> i32 {
        self.linktype
    }

    /// Next packet, or `None` when the read timeout passes without one.
    pub fn next_packet(&mut self) -> Result<Option<RawPacket<'_>>> {
        let mut hdr: *mut PktHdr = ptr::null_mut();
        let mut data: *const u8 = ptr::null();
        // SAFETY: on success Npcap points hdr/data at a buffer valid until the next call,
        // which the returned borrow of `self` guarantees.
        let rc = unsafe { (self.np.next_ex)(self.p, &mut hdr, &mut data) };
        match rc {
            1 => unsafe {
                let h = &*hdr;
                let ts_us = h.ts.tv_sec as u32 as u64 * 1_000_000 + h.ts.tv_usec as u32 as u64;
                Ok(Some(RawPacket {
                    ts_us,
                    data: std::slice::from_raw_parts(data, h.caplen as usize),
                }))
            },
            0 => Ok(None),
            _ => bail!("capture read failed ({rc}): {}", self.last_error()),
        }
    }

    pub fn set_filter(&mut self, expr: &str) -> Result<()> {
        let c = CString::new(expr)?;
        let mut prog = BpfProgram {
            bf_len: 0,
            bf_insns: ptr::null_mut(),
        };
        // SAFETY: compile into a local program, install it, then free it.
        unsafe {
            if (self.np.compile)(self.p, &mut prog, c.as_ptr(), 1, PCAP_NETMASK_UNKNOWN) != 0 {
                bail!("invalid capture filter {expr:?}: {}", self.last_error());
            }
            let rc = (self.np.setfilter)(self.p, &mut prog);
            (self.np.freecode)(&mut prog);
            if rc != 0 {
                bail!("setting capture filter failed: {}", self.last_error());
            }
        }
        Ok(())
    }

    pub fn stats(&mut self) -> Result<CaptureStats> {
        let mut s = PcapStat::default();
        // SAFETY: PcapStat is at least as large as Npcap's struct pcap_stat.
        if unsafe { (self.np.stats)(self.p, &mut s) } != 0 {
            bail!("reading capture stats failed: {}", self.last_error());
        }
        Ok(CaptureStats {
            received: s.ps_recv as u64,
            dropped: s.ps_drop as u64 + s.ps_ifdrop as u64,
        })
    }

    fn last_error(&self) -> String {
        // SAFETY: pcap_geterr returns a string owned by the handle.
        cstr(unsafe { (self.np.geterr)(self.p) })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: the handle was created by pcap_create and is closed exactly once.
        unsafe { (self.np.close)(self.p) }
    }
}

fn npcap_dir() -> Result<PathBuf> {
    let mut buf = [0u16; 260];
    // SAFETY: buffer and its length are passed together.
    let n = unsafe { win::GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    if n == 0 || n >= buf.len() {
        bail!("GetSystemDirectoryW failed");
    }
    Ok(PathBuf::from(OsString::from_wide(&buf[..n])).join("Npcap"))
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: callers pass NUL-terminated strings owned by Npcap.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

fn errbuf(buf: &[c_char]) -> String {
    cstr(buf.as_ptr())
}

/// Reads the address out of a Windows SOCKADDR (AF_INET = 2, AF_INET6 = 23).
unsafe fn sockaddr_ip(p: *const u8) -> Option<IpAddr> {
    if p.is_null() {
        return None;
    }
    let family = u16::from_ne_bytes([*p, *p.add(1)]);
    match family {
        2 => {
            let b = std::slice::from_raw_parts(p.add(4), 4);
            Some(IpAddr::from([b[0], b[1], b[2], b[3]]))
        }
        23 => {
            let mut a = [0u8; 16];
            a.copy_from_slice(std::slice::from_raw_parts(p.add(8), 16));
            Some(IpAddr::from(a))
        }
        _ => None,
    }
}
