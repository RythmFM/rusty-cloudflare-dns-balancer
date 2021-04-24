use crate::models::{SerializedServiceTarget, ServiceTarget, SerializedServiceUri, ServiceUri};
use std::net::Ipv4Addr;
use std::str::FromStr;
use warp::http::Method;

pub struct Config {}

impl Config {
    pub fn read_service_targets(data: &str) -> Vec<ServiceTarget> {
        let parsed: Vec<SerializedServiceTarget> = serde_json::from_str(data)
            .expect("Invalid service_targets json");
        parsed.iter()
            .map(|ser| {
                let ser = ser.clone();
                ServiceTarget {
                    target: Ipv4Addr::from_str(ser.ip.as_str()).expect("Invalid IP"),
                    check: Config::parse_service_uri(ser.check),
                    zone: ser.cf_zone,
                    dns: ser.cf_dns,
                    response_threshold_ms: ser.response_threshold_ms,
                }
            })
            .collect()
    }

    fn parse_service_uri(ser: SerializedServiceUri) -> ServiceUri {
        match ser.r#type.to_lowercase().as_str() {
            "icmp" => {
                ServiceUri::Icmp
            }
            "tcpprobe" => {
                ServiceUri::TcpProbe(ser.port.expect("TcpProbe expects a port field"))
            }
            "http" => {
                ServiceUri::Http(
                    ser.port.expect("HTTP expects a port field"),
                    Method::from_str(ser.method.unwrap_or("GET".to_owned()).as_str()).expect("Invalid HTTP method"),
                    ser.route.unwrap_or("/".to_owned()),
                )
            }
            "https" => {
                ServiceUri::HttpSecure(
                    ser.port.expect("HTTP expects a port field"),
                    Method::from_str(ser.method.unwrap_or("GET".to_owned()).as_str()).expect("Invalid HTTP method"),
                    ser.route.unwrap_or("/".to_owned()),
                )
            }
            _ => {
                panic!("Invalid service type provided, please use Icmp, TcpProbe, Http or Https")
            }
        }
    }
}