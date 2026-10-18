//! Which processes are running, and which TCP/UDP sockets each one owns.
//!
//! Uses the IP Helper connection tables, which any normal user can read.

use std::collections::HashSet;
use std::ffi::c_void;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ptr;

use anyhow::{bail, Result};

use crate::win;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpConn {
    pub pid: u32,
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub state: u32,
}

impl TcpConn {
    pub const LISTEN: u32 = 2;
    pub const ESTABLISHED: u32 = 5;

    pub fn state_name(&self) -> &'static str {
        match self.state {
            1 => "CLOSED",
            2 => "LISTEN",
            3 => "SYN_SENT",
            4 => "SYN_RCVD",
            5 => "ESTABLISHED",
            6 => "FIN_WAIT1",
            7 => "FIN_WAIT2",
            8 => "CLOSE_WAIT",
            9 => "CLOSING",
            10 => "LAST_ACK",
            11 => "TIME_WAIT",
            12 => "DELETE_TCB",
            _ => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpEndpoint {
    pub pid: u32,
    pub local: SocketAddr,
}

pub fn processes() -> Result<Vec<ProcessInfo>> {
    // SAFETY: standard Toolhelp snapshot walk; the entry struct is zeroed and sized first.
    unsafe {
        let snap = win::CreateToolhelp32Snapshot(win::TH32CS_SNAPPROCESS, 0);
        if snap == win::INVALID_HANDLE_VALUE {
            bail!(
                "CreateToolhelp32Snapshot failed: {}",
                std::io::Error::last_os_error()
            );
        }
        let mut out = Vec::new();
        let mut entry: win::ProcessEntry32W = std::mem::zeroed();
        entry.dw_size = std::mem::size_of::<win::ProcessEntry32W>() as u32;
        let mut ok = win::Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry
                .sz_exe_file
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.sz_exe_file.len());
            out.push(ProcessInfo {
                pid: entry.th32_process_id,
                name: String::from_utf16_lossy(&entry.sz_exe_file[..len]),
            });
            ok = win::Process32NextW(snap, &mut entry);
        }
        win::CloseHandle(snap);
        Ok(out)
    }
}

/// Processes whose executable name contains `needle` (case-insensitive).
pub fn find_processes(needle: &str) -> Result<Vec<ProcessInfo>> {
    let needle = needle.to_lowercase();
    Ok(processes()?
        .into_iter()
        .filter(|p| p.pid != 0 && p.name.to_lowercase().contains(&needle))
        .collect())
}

pub fn tcp_connections() -> Result<Vec<TcpConn>> {
    let mut out = Vec::new();

    let v4 = fetch_table(|buf, size| unsafe {
        win::GetExtendedTcpTable(buf, size, 0, win::AF_INET, win::TCP_TABLE_OWNER_PID_ALL, 0)
    })?;
    // MIB_TCPROW_OWNER_PID: state, localAddr, localPort, remoteAddr, remotePort, pid
    for r in rows(&v4, 24) {
        out.push(TcpConn {
            state: u32_at(r, 0),
            local: SocketAddr::new(v4_at(r, 4).into(), port_at(r, 8)),
            remote: SocketAddr::new(v4_at(r, 12).into(), port_at(r, 16)),
            pid: u32_at(r, 20),
        });
    }

    let v6 = fetch_table(|buf, size| unsafe {
        win::GetExtendedTcpTable(buf, size, 0, win::AF_INET6, win::TCP_TABLE_OWNER_PID_ALL, 0)
    })?;
    // MIB_TCP6ROW_OWNER_PID: localAddr[16], localScope, localPort, remoteAddr[16],
    // remoteScope, remotePort, state, pid
    for r in rows(&v6, 56) {
        out.push(TcpConn {
            local: SocketAddr::new(v6_at(r, 0).into(), port_at(r, 20)),
            remote: SocketAddr::new(v6_at(r, 24).into(), port_at(r, 44)),
            state: u32_at(r, 48),
            pid: u32_at(r, 52),
        });
    }
    Ok(out)
}

pub fn udp_endpoints() -> Result<Vec<UdpEndpoint>> {
    let mut out = Vec::new();

    let v4 = fetch_table(|buf, size| unsafe {
        win::GetExtendedUdpTable(buf, size, 0, win::AF_INET, win::UDP_TABLE_OWNER_PID, 0)
    })?;
    // MIB_UDPROW_OWNER_PID: localAddr, localPort, pid
    for r in rows(&v4, 12) {
        out.push(UdpEndpoint {
            local: SocketAddr::new(v4_at(r, 0).into(), port_at(r, 4)),
            pid: u32_at(r, 8),
        });
    }

    let v6 = fetch_table(|buf, size| unsafe {
        win::GetExtendedUdpTable(buf, size, 0, win::AF_INET6, win::UDP_TABLE_OWNER_PID, 0)
    })?;
    // MIB_UDP6ROW_OWNER_PID: localAddr[16], localScope, localPort, pid
    for r in rows(&v6, 28) {
        out.push(UdpEndpoint {
            local: SocketAddr::new(v6_at(r, 0).into(), port_at(r, 20)),
            pid: u32_at(r, 24),
        });
    }
    Ok(out)
}

/// Local addresses the given processes use for internet (non-loopback) TCP connections.
/// Used to pick which network adapter to capture on.
pub fn local_ips_for(pids: &HashSet<u32>, tcp: &[TcpConn]) -> HashSet<IpAddr> {
    tcp.iter()
        .filter(|c| pids.contains(&c.pid))
        .filter(|c| c.state == TcpConn::ESTABLISHED && !c.remote.ip().is_loopback())
        .map(|c| c.local.ip())
        .collect()
}

/// Calls a GetExtended*Table function, growing the buffer until the table fits.
fn fetch_table(call: impl Fn(*mut c_void, *mut u32) -> u32) -> Result<Vec<u8>> {
    // u32 storage keeps the buffer 4-byte aligned, as the API expects.
    let mut buf: Vec<u32> = Vec::new();
    let mut size: u32 = 0;
    for _ in 0..10 {
        let p = if buf.is_empty() {
            ptr::null_mut()
        } else {
            buf.as_mut_ptr().cast::<c_void>()
        };
        match call(p, &mut size) {
            0 => return Ok(buf.iter().flat_map(|w| w.to_ne_bytes()).collect()),
            win::ERROR_INSUFFICIENT_BUFFER => {
                // Leave headroom: connections can appear between the two calls.
                buf = vec![0u32; (size as usize).div_ceil(4) + 1024];
                size = (buf.len() * 4) as u32;
            }
            err => bail!("reading the connection table failed (Win32 error {err})"),
        }
    }
    bail!("the connection table kept growing while being read")
}

fn rows(table: &[u8], row_size: usize) -> impl Iterator<Item = &[u8]> {
    let count = table
        .get(0..4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap()) as usize)
        .unwrap_or(0);
    table
        .get(4..)
        .unwrap_or(&[])
        .chunks_exact(row_size)
        .take(count)
}

fn u32_at(row: &[u8], off: usize) -> u32 {
    u32::from_ne_bytes(row[off..off + 4].try_into().unwrap())
}

/// Ports are stored in network byte order in the low 16 bits of a DWORD.
fn port_at(row: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([row[off], row[off + 1]])
}

fn v4_at(row: &[u8], off: usize) -> Ipv4Addr {
    Ipv4Addr::new(row[off], row[off + 1], row[off + 2], row[off + 3])
}

fn v6_at(row: &[u8], off: usize) -> Ipv6Addr {
    let bytes: [u8; 16] = row[off..off + 16].try_into().unwrap();
    Ipv6Addr::from(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp_v4_row() {
        let mut table = vec![1, 0, 0, 0];
        table.extend_from_slice(&5u32.to_ne_bytes()); // ESTABLISHED
        table.extend_from_slice(&[192, 168, 1, 20]);
        table.extend_from_slice(&[0xC3, 0x50, 0, 0]); // 50000
        table.extend_from_slice(&[203, 0, 113, 7]);
        table.extend_from_slice(&[0x1E, 0x61, 0, 0]); // 7777
        table.extend_from_slice(&4242u32.to_ne_bytes());
        let r = rows(&table, 24).next().unwrap();
        assert_eq!(u32_at(r, 0), 5);
        assert_eq!(
            SocketAddr::new(v4_at(r, 4).into(), port_at(r, 8)),
            "192.168.1.20:50000".parse().unwrap()
        );
        assert_eq!(
            SocketAddr::new(v4_at(r, 12).into(), port_at(r, 16)),
            "203.0.113.7:7777".parse().unwrap()
        );
        assert_eq!(u32_at(r, 20), 4242);
    }

    #[test]
    fn row_count_limits_iteration() {
        let mut table = vec![0, 0, 0, 0];
        table.extend_from_slice(&[0u8; 24]);
        assert_eq!(rows(&table, 24).count(), 0);
    }

    #[test]
    fn live_tables_are_readable() {
        assert!(!processes().unwrap().is_empty());
        tcp_connections().unwrap();
        udp_endpoints().unwrap();
    }
}
