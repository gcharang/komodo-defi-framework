use std::collections::HashMap;

use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable, Timer},
             log, Future01CompatExt};
use futures::channel::oneshot::{self, Receiver, Sender};
use mm2_event_stream::{behaviour::{EventBehaviour, EventInitStatus},
                       EventStreamConfiguration};
use mm2_number::BigDecimal;

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

        let mut current_balances: HashMap<String, BigDecimal> = HashMap::new();
        loop {
            match self
                .as_ref()
                .scripthash_notification_receiver
                .as_ref()
                .unwrap()
                .lock()
                .await
                .try_next()
            {
                Ok(Some(_)) => {},
                _ => {
                    Timer::sleep(0.1).await;
                    continue;
                },
            };

            println!("CURRENT VALUES {:?}", current_balances);

            let new_balances = self.my_addresses_balances().await.unwrap();

            println!("NEW VALUES {:?}", new_balances);

            if new_balances == current_balances {
                continue;
            }

            // Get the differences
            let updated_parts: HashMap<String, BigDecimal> = new_balances
                .iter()
                .filter_map(|(key, new_value)| match current_balances.get(key) {
                    Some(current_value) if new_value != current_value => Some((key.clone(), new_value.clone())),
                    None => Some((key.clone(), new_value.clone())),
                    _ => None,
                })
                .collect();

            println!("UPDATED VALUES {:?}", updated_parts);

            current_balances = new_balances;
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
