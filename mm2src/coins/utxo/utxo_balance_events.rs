use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable},
             log};
use futures::channel::oneshot::{self, Receiver, Sender};
use mm2_event_stream::{behaviour::{EventBehaviour, EventInitStatus},
                       EventStreamConfiguration};

use super::utxo_standard::UtxoStandardCoin;
use crate::{MarketCoinOps, MmCoin};

#[async_trait]
impl EventBehaviour for UtxoStandardCoin {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, _interval: f64, _tx: oneshot::Sender<EventInitStatus>) {
        loop {
            // TODO
        }
    }

    async fn spawn_if_active(self, config: &EventStreamConfiguration) -> EventInitStatus {
        if let Some(event) = config.get_event(Self::EVENT_NAME) {
            log::info!(
                "{} event is activated for {}. `stream_interval_seconds`({}) has no effect on this.",
                Self::EVENT_NAME,
                self.ticker(),
                event.stream_interval_seconds
            );

            let (tx, rx): (Sender<EventInitStatus>, Receiver<EventInitStatus>) = oneshot::channel();
            let fut = self.clone().handle(event.stream_interval_seconds, tx);
            let settings =
                AbortSettings::info_on_abort(format!("{} event is stopped for {}.", Self::EVENT_NAME, self.ticker()));
            self.spawner().spawn_with_settings(fut, settings);

            rx.await.unwrap_or_else(|e| {
                EventInitStatus::Failed(format!("Event initialization status must be received: {}", e))
            })
        } else {
            EventInitStatus::Inactive
        }
    }
}
