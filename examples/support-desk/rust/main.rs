//! Deploy and run the support desk against the server the ambient
//! configuration names (for example `WYRD_SERVER_URL` and `WYRD_API_KEY`):
//!     cargo run -p wyrd-rust-examples --bin support_desk

/// The support-desk workflow this binary runs.
mod support_desk;

use std::time::Duration;

use wyrd_sdk::WyrdClient;

#[tokio::main]
async fn main() -> Result<(), support_desk::Error> {
    let client = WyrdClient::from_global()?;
    let bundle = tempfile::tempdir()?;
    let desk = support_desk::deploy(&client, &bundle.path().join("bundle")).await?;
    let served = support_desk::serve(&desk).await?;
    let verdicts =
        support_desk::wait_for_verdicts(&client, &desk, Duration::from_secs(300)).await?;
    println!("{verdicts:?}");
    for verdict in [true, false] {
        if let Some(request) = served.iter().find(|request| request.passed == verdict) {
            println!(
                "{:#?}",
                support_desk::explain(&client, &request.run_id).await?
            );
        }
    }
    Ok(())
}
