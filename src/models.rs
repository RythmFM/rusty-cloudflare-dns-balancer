use std::net::Ipv4Addr;
use warp::http::Method;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceTarget {
    pub target: Ipv4Addr,
    pub check: ServiceUri,
    pub zone: String,
    pub dns: String,
    pub response_threshold_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceUri {
    Icmp,
    TcpProbe(u16),
    Http(u16, Method, String),
    HttpSecure(u16, Method, String),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SerializedServiceTarget {
    pub ip: String,
    pub cf_zone: String,
    pub cf_dns: String,
    pub check: SerializedServiceUri,
    pub response_threshold_ms: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SerializedServiceUri {
    pub r#type: String,
    pub port: Option<u16>,
    pub method: Option<String>,
    pub route: Option<String>,
}
