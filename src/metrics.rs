use lazy_static::lazy_static;

use prometheus::{
    self, register_histogram_vec, register_int_counter_vec, register_int_gauge,
    register_int_gauge_vec, Encoder, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec,
    TextEncoder,
};
use warp::{http, Filter};

lazy_static! {
    pub static ref CLOUDFLARE_REQUEST_COUNTER: IntCounterVec = register_int_counter_vec!(
        "dns_balancer_cloudflare_requests",
        "Requests to cloudflare api by type",
        &["type"]
    )
    .unwrap();

    pub static ref TARGETS_AVAILABLE: IntGauge = register_int_gauge!(
        "dns_balancer_targets_available",
        "Amount of online targets"
    )
    .unwrap();

    pub static ref TARGETS_STATUS: IntGaugeVec = register_int_gauge_vec!(
        "dns_balancer_targets_status",
        "Status per target: 1 Online - 0 Offline",
        &["target"]
    )
    .unwrap();

    pub static ref HEALTHCHECK_REQUEST_TIME: HistogramVec = register_histogram_vec!(
        "dns_balancer_healthcheck_request_time",
        "Used for quantiles over the average healthcheck request time",
        &["target"],
        prometheus::exponential_buckets(0.01, 1.8, 20).unwrap()
    )
    .unwrap();
}

pub fn metrics_filter() -> impl warp::Filter<Extract=(impl warp::Reply, ), Error=warp::Rejection> + Clone {
    warp::get()
        .and(warp::path("metrics"))
        .and_then(|| async move {
            // create prometheus output and reply
            let mut buffer = Vec::new();
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();

            encoder
                .encode(&metric_families, &mut buffer)
                .map_err(|_| warp::reject())?;

            match String::from_utf8(buffer.clone()).map_err(|_| warp::reject()) {
                Ok(output) => Ok(warp::reply::with_status(output, http::StatusCode::OK)),
                _ => Err(warp::reject()),
            }
        })
}
