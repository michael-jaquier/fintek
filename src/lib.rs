pub mod metrics;

use reqwest::Error;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::fmt::{self, Display};

use std::{path::Path, sync::atomic::AtomicU64};
use tokio::fs::{self};
use tracing::info;
use tracing::instrument;
use tracing::trace;

#[derive(Debug)]
pub enum Markets {
    Stock(StockMarket),
    Forex(ForexMarket),
    Crypto(CryptoMarket),
}

impl Display for Markets {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Markets::Stock(m) => write!(f, "{:?}", m),
            Markets::Forex(m) => write!(f, "{:?}", m),
            Markets::Crypto(m) => write!(f, "{:?}", m),
        }
    }
}

#[derive(Debug)]
pub enum StockMarket {
    NYSE,
    NASDAQ,
}

#[derive(Debug)]
pub enum ForexMarket {
    EURUSD,
    GBPUSD,
    USDJPY,
}

#[derive(Debug)]
pub enum CryptoMarket {
    BTCUSD,
    ETHUSD,
    LTCUSD,
}

#[instrument(skip(api_key))]
pub async fn should_sleep(market: Markets, api_key: &str) -> Result<u64, Error> {
    let m = market.to_string();
    let url = format!(
        "https://api.twelvedata.com/market_state?exchange={}&apikey={}",
        market, api_key
    );
    let response = reqwest::get(&url).await?;
    let data = response.text().await?;
    let maybe_value: Value = serde_json::from_str(&data).unwrap_or_default();
    if let Some(array) = maybe_value.as_array() {
        for object in array {
            if let Some(is_market_open) = object["is_market_open"].as_bool() {
                if is_market_open {
                    trace!(market = %m, "Market is open");
                    return Ok(0);
                } else {
                    trace!(market = %m, "Market is closed");
                    let time_to_open = object["time_to_open"]
                        .as_str()
                        .unwrap_or_else(|| "0:0:0")
                        .split(':')
                        .collect::<Vec<_>>();
                    let hours: u64 = time_to_open[0].parse().ok().unwrap_or_default();
                    let minutes: u64 = time_to_open[1].parse().ok().unwrap_or_default();
                    let seconds: u64 = time_to_open[2].parse().ok().unwrap_or_default();
                    info!(market = %m, hours, minutes, seconds, "Time to open");
                    return Ok(hours * 3600 + minutes * 60 + seconds);
                }
            }
        }
    }

    Ok(0)
}

pub fn calculate_sleep_duration(
    num_tickers: usize,
    rate_limit1: u64,
    period_in_seconds1: u64,
    rate_limit2: u64,
    period_in_seconds2: u64,
) -> Option<u64> {
    if num_tickers == 0 {
        return None;
    }

    let calls_per_ticker1 = rate_limit1
        .checked_div(num_tickers as u64)
        .unwrap_or_default();
    let sleep_duration1 = period_in_seconds1
        .checked_sub(calls_per_ticker1)
        .unwrap_or_default();

    let calls_per_ticker2 = rate_limit2
        .checked_sub(num_tickers as u64)
        .unwrap_or_default();
    let sleep_duration2 = period_in_seconds2
        .checked_sub(calls_per_ticker2)
        .unwrap_or_default();

    // Choose the stricter rate limit
    let sleep_duration = std::cmp::min(sleep_duration1, sleep_duration2);

    Some(sleep_duration)
}

#[instrument(skip(api_key))]
pub async fn call_api(symbol: &str, api_key: &str) -> Result<(), Error> {
    let url = format!(
        "https://api.twelvedata.com/price?symbol={}&apikey={}",
        symbol, api_key
    );
    let response = reqwest::get(&url).await?;

    let data = response.text().await?;
    let v: Value = serde_json::from_str(&data).unwrap_or_else(|_| Value::Null);
    if let Some(price) = v["price"].as_str() {
        trace!(price, symbol, "Updating stock price");
        if let Some(parsed) = price.parse::<f64>().ok() {
            metrics::update_stock_price(parsed, symbol);
        }
    }
    Ok(())
}

#[instrument]
pub async fn read_tickers() -> Tickers {
    let path = Path::new("tickers");
    serde_json::from_str(&fs::read_to_string(&path).await.unwrap()).unwrap_or_default()
}

#[instrument]
pub async fn check_tickers() -> Option<Tickers> {
    static LAST_MODIFIED: AtomicU64 = AtomicU64::new(0);
    let metdata = fs::metadata("tickers")
        .await
        .expect("Failed to read metadata");
    let modified = metdata
        .modified()
        .expect("Failed to read modified")
        .elapsed()
        .map(|d| d.as_secs())
        .unwrap_or_default();
    if LAST_MODIFIED.load(std::sync::atomic::Ordering::Relaxed) != modified {
        LAST_MODIFIED.store(modified, std::sync::atomic::Ordering::Relaxed);
        info!(modified, "File modified updating tickers");
        Some(read_tickers().await)
    } else {
        None
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Tickers {
    tickers: Vec<String>,
}

impl Default for Tickers {
    fn default() -> Self {
        Tickers { tickers: vec![] }
    }
}

impl Tickers {
    pub async fn init() -> Self {
        let exists = fs::try_exists("tickers").await;
        if exists.is_err() || !exists.unwrap() {
            create_tickers().await;
            read_tickers().await
        } else {
            read_tickers().await
        }
    }

    pub fn new(t: Vec<String>) -> Self {
        Tickers { tickers: t }
    }

    pub fn set_tickers(&mut self, tickers: Vec<String>) {
        self.tickers = tickers;
    }

    pub fn get_tickers(&self) -> &Vec<String> {
        &self.tickers
    }

    pub async fn dump_to_file(&self) {
        let serde_output = serde_json::to_string(self).unwrap();
        fs::write("tickers", serde_output)
            .await
            .expect("Failed to write to file");
    }
}

pub async fn create_tickers() {
    Tickers::default().dump_to_file().await
}
