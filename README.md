# Rusty CloudFlare DNS Balancer

This services scopes to be an external health-checker which checks a specified set 
of targets and manages [CloudFlare](https://cloudflare.com)'s DNS settings.

If a target is down, it is removed from the specified DNS entry.
When it is back up, it will be added again.

# Limitations

* This only works if the DNS entries have the Cloud/CF-Proxy enabled, since otherwise DNS
caches would destroy the purpose of this balancer.
* ICMP only works on linux machines

# Configuration

| Env Name | Default Value | Description                                                |
|----------|---------------|------------------------------------------------------------|
| RUST_LOG |               | The log level used for stdout. Recommended: info           |
| CF_TOKEN |               | The API Token used to interact with the CloudFlare API     |
| SERVICE_TARGETS |        | The services which are supposed to be monitored.           |
| CHECK_INTERVAL |      30 | The interval between checks on the targets in seconds      |
| PROMETHEUS_ENABLED | false | Whether a prometheus webserver with /metrics endpoint should be started |
| PROMETHEUS_HOST | 0.0.0.0 | The host on which the prometheus server will listen       |
| PROMETHEUS_PORT |   8080 | The port on which the prometheus server will listen        |

The service targets are an array of the following struct(s):
```rust
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
```

An example value for this would look like that:
```json
[
  {
    "ip": "1.2.3.4",
    "cf_zone": "067bd5dbafe54a4270adc9a1742cb8ae",
    "cf_dns": "testfailover.example.org",
    "check": {
      "type": "TcpProbe",
      "port": 443
    }
  },
  {
    "ip": "5.6.7.8",
    "cf_zone": "067bd5dbafe54a4270adc9a1742cb8ae",
    "cf_dns": "testfailover.example.org",
    "response_threshold_ms": 500,
    "check": {
      "type": "https",
      "port": 443,
      "method": "GET",
      "route": "/"
    }
  }
]
```

# Side Notes
Updating the dependencies requires to update the recipe.

Update the recipe using `cargo chef prepare`.

# Todo

* IPv6 Support
* Status Code checks for HTTP(S)