#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app_core::run().await
}
