use ::std::env;
use dotenv::dotenv;
use fintek::{
    check_tickers,
    metrics::{MetricServer},
    Markets, StockMarket, Tickers,
};
use reqwest::Error;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
#[tokio::main]
async fn main() -> Result<(), Error> {
    let filter = EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .without_time()
                .with_thread_ids(true)
                .with_target(true)
                .json()
                .with_current_span(true)
                .with_span_list(false),
        )
        .init();

    tokio::spawn(async move {
        MetricServer::start(([127, 0, 0, 1], 9091).into()).await;
    });
    dotenv().ok();
    let api_key = env::var("API_KEY").expect("API_KEY must be set");

    let mut tickers = Tickers::init().await;

    loop {
        let night_time = fintek::should_sleep(Markets::Stock(StockMarket::NYSE), &api_key)
            .await
            .unwrap_or_default();

        tokio::time::sleep(tokio::time::Duration::from_secs(night_time)).await;

        if let Some(new) = check_tickers().await {
            tickers = new;
        }

        let num_tickers = tickers.get_tickers().len();
        let sleep_duration =
            fintek::calculate_sleep_duration(num_tickers, 8, 60, 800, (6.5 * 60. * 60.) as u64);

        for ticker in tickers.get_tickers() {
            let _ = fintek::call_api(ticker, &api_key).await.map_err(|e| {
                tracing::error!(error = ?e, "Failed to call API");
                e
            });
            tokio::time::sleep(tokio::time::Duration::from_secs(sleep_duration)).await;
        }
    }
}

// {"price":"179.64000"}
