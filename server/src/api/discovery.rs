//! 网络地址发现：启动时枚举本机网卡 IPv4 地址，
//! 供 web-ui 自动发现后端（同一服务端可能有多个可达地址）。

use axum::{extract::State, response::Json, routing::get, Router};
use serde::Serialize;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::Arc;

use crate::AppState;

const SERVER_PORT: u16 = 3000;

#[derive(Serialize)]
pub struct DiscoveryResponse {
    pub name: String,
    pub initialized: bool,
    pub port: u16,
    /// 本机所有非回环 IPv4 网卡地址（附加端口），回环地址排在最后。
    pub addresses: Vec<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/info", get(discovery_info))
}

async fn discovery_info(State(state): State<Arc<AppState>>) -> Json<DiscoveryResponse> {
    let initialized = Path::new("init.lock").exists();
    let mut name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "Alls Recorder".to_string());

    let db_guard = state.db.read().await;
    if let Some(pool) = db_guard.as_ref() {
        if let Ok(row) = crate::dbq_as!(
            pool, (serde_json::Value,), fetch_optional,
            "SELECT value FROM system_config WHERE key = 'server_name'",
            []
        )
        {
            if let Some((val,)) = row {
                if let Some(v) = val.as_str() {
                    if !v.trim().is_empty() {
                        name = v.to_string();
                    }
                }
            }
        }
    }

    Json(DiscoveryResponse {
        name,
        initialized,
        port: SERVER_PORT,
        addresses: build_addresses(),
    })
}

fn build_addresses() -> Vec<String> {
    let ips = local_ipv4_addresses();
    let mut out: Vec<String> = ips
        .iter()
        .map(|ip| format!("{}:{}", ip, SERVER_PORT))
        .collect();
    if !out.iter().any(|a| a.starts_with("127.")) {
        out.push(format!("127.0.0.1:{}", SERVER_PORT));
    }
    out
}

#[cfg(windows)]
fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Networking::WinSock::{AF_INET, AF_UNSPEC, SOCKADDR_INET};

    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let family = AF_UNSPEC.0 as u32;

    unsafe {
        let mut out: Vec<Ipv4Addr> = Vec::new();
        let mut size: u32 = 0;

        // 第一次调用获取所需缓冲区大小（预期返回 ERROR_BUFFER_OVERFLOW）
        GetAdaptersAddresses(family, flags, None, None, &mut size);

        if size == 0 {
            return out;
        }
        let mut buf = vec![0u8; size as usize];
        let result = GetAdaptersAddresses(
            family,
            flags,
            None,
            Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
            &mut size,
        );
        if result != 0 {
            tracing::warn!("GetAdaptersAddresses failed: {}", result);
            return out;
        }

        let mut adapter = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !adapter.is_null() {
            let a = &*adapter;
            if a.OperStatus == IfOperStatusUp {
                let mut unicast = a.FirstUnicastAddress;
                while !unicast.is_null() {
                    let addr = &(*unicast).Address;
                    if !addr.lpSockaddr.is_null() && (*addr.lpSockaddr).sa_family == AF_INET {
                        let si = &*(addr.lpSockaddr as *const SOCKADDR_INET);
                        let ip = Ipv4Addr::from(u32::from_be(si.Ipv4.sin_addr.S_un.S_addr));
                        if !ip.is_loopback() && !out.contains(&ip) {
                            out.push(ip);
                        }
                    }
                    unicast = (*unicast).Next;
                }
            }
            adapter = a.Next;
        }
        out
    }
}

#[cfg(not(windows))]
fn local_ipv4_addresses() -> Vec<Ipv4Addr> {
    // 非 Windows 平台用 UDP connect 技巧获取主出口 IP
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if let std::net::IpAddr::V4(ip) = addr.ip() {
                    if !ip.is_unspecified() {
                        return vec![ip];
                    }
                }
            }
        }
    }
    Vec::new()
}
