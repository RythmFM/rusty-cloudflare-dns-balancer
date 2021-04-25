use crate::models::{ServiceTarget, ServiceUri};
use crate::metrics::{CLOUDFLARE_REQUEST_COUNTER, TARGETS_AVAILABLE, HEALTHCHECK_REQUEST_TIME, TARGETS_STATUS};
use cloudflare::framework::async_api::Client;
use tokio::task::JoinHandle;
use tokio::task::spawn_blocking;
use tokio::time::Duration;
use log::{debug, info, warn};
use warp::http::Method;
use std::time::SystemTime;
#[cfg(not(target_env = "msvc"))]
use oping::Ping;
use std::ops::Sub;
use std::net::{IpAddr, SocketAddr, Ipv4Addr, TcpStream};
use cloudflare::endpoints::dns::{ListDnsRecordsParams, ListDnsRecords, DnsContent, DnsRecord, CreateDnsRecord, CreateDnsRecordParams, DeleteDnsRecord};
use cloudflare::framework::response::{ApiSuccess, ApiResponse};
use std::collections::HashMap;

pub(crate) struct HealthChecker {
    cloudflare_client: Client,
    http_client: reqwest::Client,
    targets: Vec<ServiceTarget>,
    unavailable: Vec<Ipv4Addr>,
}

impl HealthChecker {
    pub fn new(cloudflare_client: Client, targets: Vec<ServiceTarget>) -> HealthChecker {
        let http_client = reqwest::ClientBuilder::new()
            .user_agent("rusty-cloudflare-dns-balancer")
            .build().unwrap();
        HealthChecker {
            cloudflare_client,
            http_client,
            targets,
            unavailable: Vec::new(),
        }
    }

    pub fn run(mut self, interval: Duration) -> JoinHandle<()> {
        let http_client = self.http_client.clone();
        tokio::spawn(async move {
            // Init
            let mut dns_regions = Vec::new();
            for target in &self.targets {
                let zone = target.zone.clone();
                if !dns_regions.contains(&(zone.clone(), target.dns.clone())) {
                    dns_regions.push((zone.clone(), target.dns.clone()))
                }
            }
            let mut existing_entries = HashMap::new();
            for (zone, dns) in dns_regions {
                if let Ok(response) = self.list_dns(zone.as_str(), dns.clone()).await {
                    response.result.iter()
                        .map(|dns| {
                            match dns.content {
                                DnsContent::A { content } => { Some(content) }
                                _ => { None }
                            }
                        })
                        .for_each(|opt_ip| {
                            if let Some(ip) = opt_ip {
                                existing_entries.insert((zone.clone(), dns.clone()), ip);
                            }
                        });
                }
            }

            for target in &self.targets {
                if !existing_entries.contains_key(&(target.zone.clone(), target.dns.clone())) {
                    info!("{} -> {} DNS entry did not exist at startup, assuming it is unavailable",
                          target.dns, target.target.to_string());
                    self.unavailable.push(target.target);
                }
            }


            // loop checks
            loop {
                info!("Running health check");
                let start = SystemTime::now();
                let handles = self.targets.iter()
                    .map(|target| {
                        let top_target = target.clone();
                        let http_client = http_client.clone();
                        let target = top_target.clone();
                        let handle = tokio::spawn(async move {
                            let base_addr = target.target;
                            let timeout_ms = target.response_threshold_ms.unwrap_or(1000);
                            let timeout = Duration::from_millis(timeout_ms as u64);
                            let service_uri = target.check;
                            let request_start = SystemTime::now();
                            let up = match service_uri {
                                ServiceUri::Icmp => {
                                    debug!("Checking ICMP {}", base_addr.to_string());
                                    #[cfg(not(target_env = "msvc"))]
                                        let result = spawn_blocking(move || {
                                        let mut ping = Ping::new();
                                        ping.set_timeout(timeout.as_secs_f64()).ok();
                                        ping.add_host(base_addr.to_string().as_str()).ok();
                                        let success = ping.send().map(|iter| {
                                            let mut success = false;
                                            for _item in iter {
                                                // todo: make sure pings were successful
                                                success = true
                                            }
                                            success
                                        }).unwrap_or(false);
                                        success
                                    }).await.unwrap_or(false);
                                    #[cfg(target_env = "msvc")]
                                        let result = false;
                                    result
                                }
                                ServiceUri::TcpProbe(port) => {
                                    debug!("Checking TCP Probe {}:{}", base_addr.to_string(), port);
                                    let addr = SocketAddr::new(IpAddr::from(base_addr), port);
                                    spawn_blocking(move || {
                                        let start = SystemTime::now();
                                        match TcpStream::connect_timeout(&addr, timeout) {
                                            Ok(_) => {
                                                debug!("TCP Probe succeeded");
                                                start.elapsed()
                                                    .map(|duration| duration.as_millis() < timeout.as_millis())
                                                    .unwrap_or(false)
                                            }
                                            Err(_) => {
                                                debug!("TCP Probe failed");
                                                false
                                            }
                                        }
                                    }).await.unwrap_or(false)
                                }
                                ServiceUri::Http(port, method, route) => {
                                    let mut uri = "http://".to_owned();
                                    uri.push_str(base_addr.to_string().as_str());
                                    uri.push_str(":");
                                    uri.push_str(port.to_string().as_str());
                                    if !route.starts_with("/") {
                                        uri.push_str("/");
                                    }
                                    uri.push_str(route.as_str());
                                    HealthChecker::http_check(http_client.clone(), method, uri, timeout).await
                                }
                                ServiceUri::HttpSecure(port, method, route) => {
                                    let mut uri = "https://".to_owned();
                                    uri.push_str(base_addr.to_string().as_str());
                                    uri.push_str(":");
                                    uri.push_str(port.to_string().as_str());
                                    if !route.starts_with("/") {
                                        uri.push_str("/");
                                    }
                                    uri.push_str(route.as_str());
                                    HealthChecker::http_check(http_client.clone(), method, uri, timeout).await
                                }
                            };
                            let target_host = base_addr.to_string();
                            let target_label = [target_host.as_str()];
                            if let Ok(request_duration) = request_start.elapsed() {
                                HEALTHCHECK_REQUEST_TIME
                                    .with_label_values(&target_label)
                                    .observe(request_duration.as_secs_f64());
                            }
                            TARGETS_STATUS
                                .with_label_values(&target_label)
                                .set(if up { 1 } else { 0 });
                            if up {
                                info!("Target {} is up", base_addr.to_string());
                            } else {
                                warn!("Target {} is down", base_addr.to_string());
                            }
                            up
                        });
                        (top_target.clone(), handle)
                    })
                    .collect::<Vec<(ServiceTarget, JoinHandle<bool>)>>();
                for (target, handle) in handles {
                    match handle.await {
                        Ok(up) => {
                            if up {
                                self.handle_target_up(target.clone()).await;
                            } else {
                                self.handle_target_down(target.clone()).await;
                            }
                        }
                        Err(err) => {
                            warn!("An error occurred when trying to join child handle: {}", err);
                            self.handle_target_down(target.clone()).await;
                        }
                    }
                }
                let elapsed = start.elapsed().unwrap_or(Duration::from_millis(0));
                info!("Completed after {}s", elapsed.as_secs_f32());
                let sleep_duration = if elapsed.lt(&interval) {
                    interval.sub(elapsed)
                } else {
                    Duration::from_secs(0)
                };
                TARGETS_AVAILABLE.set((self.targets.len() - self.unavailable.len()) as i64);
                debug!("Sleeping for another {}s before next health check", sleep_duration.as_secs_f32());
                tokio::time::sleep(sleep_duration).await;
            }
        })
    }

    async fn handle_target_up(&mut self, target: ServiceTarget) {
        if self.unavailable.contains(&target.target) {
            self.cloudflare_add_target(&target).await;
            // retain all targets which are not this target
            self.unavailable.retain(|other| !target.target.eq(other));
            info!("Target {} is available again", target.target.to_string());
        }
    }

    async fn handle_target_down(&mut self, target: ServiceTarget) {
        if !self.unavailable.contains(&target.target) {
            let dns_targets = self.targets.iter()
                .filter(|inner| inner.dns.eq(&target.dns))
                .collect::<Vec<&ServiceTarget>>();
            let dns_unavailable = dns_targets.iter()
                .filter(|inner| self.unavailable.contains(&inner.target))
                .collect::<Vec<&&ServiceTarget>>();
            let available = dns_targets.len() - dns_unavailable.len();
            if available > 1 {
                self.cloudflare_remove_target(&target).await;
                self.unavailable.push(target.target);
                warn!("Target {} went unavailable", target.target.to_string());
            } else {
                warn!("Target {} is unavailable. Not removing from CF due to being the last target",
                      target.target.to_string());
            }
        } else {
            debug!("Target {} is still unavailable", target.target.to_string());
        }
    }

    async fn http_check(client: reqwest::Client, method: Method, uri: String, timeout: Duration) -> bool {
        debug!("Checking {} {}", method.as_str(), uri.as_str());
        let request = client.request(method, uri)
            .timeout(timeout)
            .build().unwrap();
        match client.execute(request).await {
            Ok(response) => {
                let status_code = response.status();
                debug!("Response Status: {}", status_code.as_u16());
                status_code.is_success()
            }
            Err(_) => {
                debug!("Response Status: 0 (Error)");
                false
            }
        }
    }

    async fn cloudflare_add_target(&self, target: &ServiceTarget) {
        let target = target.clone();
        if let Ok(response) = self.list_dns(target.zone.as_str(), target.dns.clone()).await {
            let response: ApiSuccess<Vec<DnsRecord>> = response;
            if response.errors.len() != 0 {
                response.errors.iter().for_each(|e| warn!("CF Api Error: {}", e));
                return;
            }
            let exists = response.result.iter()
                .map(|dns| dns.content.clone())
                .map(|dns| {
                    match dns {
                        DnsContent::A { content } => { Some(content) }
                        _ => { None }
                    }
                })
                .any(|opt_addr| {
                    opt_addr.map(|addr| addr.eq(&target.target))
                        .unwrap_or(false)
                });
            if !exists {
                let dns_name = target.dns.clone();
                self.cloudflare_client.request_handle(&CreateDnsRecord {
                    zone_identifier: target.zone.as_str(),
                    params: CreateDnsRecordParams {
                        ttl: None,
                        priority: None,
                        proxied: Some(true),
                        name: dns_name.as_str(),
                        content: DnsContent::A { content: target.target },
                    },
                }).await.ok();
                CLOUDFLARE_REQUEST_COUNTER
                    .with_label_values(&["create_dns"])
                    .inc();
                info!("Created cloudflare record for {} -> {}", dns_name, target.target.to_string());
            }
        }
    }

    async fn cloudflare_remove_target(&self, target: &ServiceTarget) {
        let target = target.clone();
        let result = self.list_dns(target.zone.as_str(), target.dns.clone()).await;
        match result {
            Ok(response) => {
                if response.errors.len() != 0 {
                    response.errors.iter().for_each(|e| warn!("CF Api Error: {}", e));
                    return;
                }
                let opt_entry = response.result.iter()
                    .find(|dns| {
                        match dns.content {
                            DnsContent::A { content } => { content.eq(&target.target) }
                            _ => { false }
                        }
                    });
                if let Some(entry) = opt_entry {
                    self.cloudflare_client.request_handle(&DeleteDnsRecord {
                        zone_identifier: target.zone.as_str(),
                        identifier: entry.id.as_str(),
                    }).await.ok();
                    CLOUDFLARE_REQUEST_COUNTER
                        .with_label_values(&["delete_dns"])
                        .inc();
                    info!("Deleted cloudflare record for {} -> {}", target.dns, target.target.to_string());
                } else {
                    warn!("No match on cloudflare for record {} -> {}", target.dns, target.target.to_string())
                }
            }
            Err(error) => {
                warn!("Error with CF Api: {}", error);
            }
        }
    }

    async fn list_dns(&self, zone: &str, dns: String) -> ApiResponse<Vec<DnsRecord>> {
        CLOUDFLARE_REQUEST_COUNTER
            .with_label_values(&["list_dns"])
            .inc();
        self.cloudflare_client.request_handle(&ListDnsRecords {
            zone_identifier: zone,
            params: ListDnsRecordsParams {
                record_type: None,
                name: Some(dns),
                page: None,
                per_page: Some(100),
                order: None,
                direction: None,
                search_match: None,
            },
        }).await
    }
}