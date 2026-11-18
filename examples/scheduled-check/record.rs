//! Record five checkout requests that each took the given number of
//! milliseconds, for the every-minute schedule to judge.

use std::error::Error;

use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;

/// Record the samples and send them before exiting.
///
/// # Errors
/// Returns a missing or unparsable latency argument, the bundle's load
/// error, or the client's Bifrost and submission errors.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // The checkout Service, pulled at deploy time.
    let state = WyrdState::from_path(".wyrd-bundle")?;
    state.start_bifrost().await?; // samples are sent to Bifrost, where the schedule reads them

    let latency: f64 = std::env::args()
        .nth(1)
        .ok_or("pass a latency in ms")?
        .parse()?;
    let run = state.run();
    for _ in 0..5 {
        run.observe()
            .submit("latency-budget", json!({ "latency": latency }))
            .await?;
    }
    state.shutdown().await?; // send the samples before the program exits
    println!("recorded 5 requests at {latency} ms");
    Ok(())
}
