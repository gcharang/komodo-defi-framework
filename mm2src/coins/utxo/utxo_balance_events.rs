use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable, Timer},
             log, Future01CompatExt};
use futures::channel::oneshot::{self, Receiver, Sender};
use mm2_event_stream::{behaviour::{EventBehaviour, EventInitStatus},
                       EventStreamConfiguration};

use super::utxo_standard::UtxoStandardCoin;
use crate::{utxo::{output_script, rpc_clients::electrum_script_hash, utxo_tx_history_v2::UtxoTxHistoryOps},
            MarketCoinOps, MmCoin};

#[async_trait]
impl EventBehaviour for UtxoStandardCoin {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, _interval: f64, tx: oneshot::Sender<EventInitStatus>) {
        let addresses = match self.my_addresses().await {
            Ok(t) => t,
            Err(e) => {
                tx.send(EventInitStatus::Failed(e.to_string()))
                    .expect("Receiver is dropped, which should never happen.");
                panic!("{}", e);
            },
        };

        for address in addresses {
            let script = output_script(&address, keys::Type::P2PKH);
            let script_hash = electrum_script_hash(&script);
            let scripthash = hex::encode(script_hash);

            if let Err(e) = self
                .as_ref()
                .rpc_client
                .blockchain_scripthash_subscribe(scripthash)
                .compat()
                .await
            {
                tx.send(EventInitStatus::Failed(e.to_string()))
                    .expect("Receiver is dropped, which should never happen.");
                panic!("{}", e);
            }
        }

        tx.send(EventInitStatus::Success)
            .expect("Receiver is dropped, which should never happen.");

        loop {
            if let Ok(Some(_)) = self
                .as_ref()
                .scripthash_notification_receiver
                .as_ref()
                .unwrap()
                .lock()
                .await
                .try_next()
            {
                println!("Received scripthash notification.");
            }

            Timer::sleep(0.5).await;
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
