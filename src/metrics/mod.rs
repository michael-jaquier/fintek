use lazy_static::lazy_static;
use prometheus::Encoder;
use prometheus::GaugeVec;
use prometheus::Opts;
use std::net::SocketAddr;
use tracing::trace;
use tracing::{info, instrument};
use warp::Filter;

#[derive(Debug)]
pub struct Metric {
    pub name: String,
    pub value: f64,
}

lazy_static! {
    pub static ref REGISTRY: prometheus::Registry = prometheus::Registry::new();
    static ref STOCK_PRICE: GaugeVec =
        GaugeVec::new(Opts::new("stock_price", "Current stock price"), &["symbol"],).unwrap();
}

fn register_metrics() {
    REGISTRY
        .register(Box::new(STOCK_PRICE.clone()))
        .expect("Failed to register stock_price metric");
}

pub struct MetricServer;

impl MetricServer {
    #[instrument]
    pub async fn start(addr: SocketAddr) {
        info!(addr = %addr, "Starting metrics server");
        register_metrics();
        let route = metrics_route();
        warp::serve(route).run(addr).await;
    }
}

#[instrument]
fn metrics_route() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("metrics").map(move || -> String {
        let metric_families = REGISTRY.gather();
        let encoder = prometheus::TextEncoder::new();
        let mut buffer = vec![];
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    })
}

#[instrument]
pub fn update_stock_price(price: f64, symbol: &str) {
    trace!("Updating stock price");
    STOCK_PRICE.with_label_values(&[symbol]).set(price);
}
