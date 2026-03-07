#[tokio::main]
async fn main() {
    hls_monitor::cli::run().await;
}
