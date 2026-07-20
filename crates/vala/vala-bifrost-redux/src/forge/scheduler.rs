use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::compact::{ForgeContext, run_compaction_tick};
use super::error::ForgeError;

pub struct ForgeScheduler {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl ForgeScheduler {
    pub fn start(context: ForgeContext, interval: Duration) -> Result<Self, ForgeError> {
        if interval.is_zero() {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge scheduler interval must be positive".to_owned(),
            });
        }
        let (stop, mut stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(error) = run_compaction_tick(&context).await {
                            tracing::error!(error = %error, "Forge compaction tick failed");
                        }
                    }
                    changed = stopped.changed() => {
                        if changed.is_err() || *stopped.borrow() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self { stop, task })
    }

    pub async fn shutdown(self) -> Result<(), ForgeError> {
        self.stop.send(true).map_err(|_| ForgeError::Shutdown)?;
        self.task.await.map_err(|error| ForgeError::Reconciliation {
            detail: error.to_string(),
        })
    }
}
