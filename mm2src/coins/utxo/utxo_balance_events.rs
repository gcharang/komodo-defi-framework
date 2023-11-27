use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable, Timer},
             log, Future01CompatExt};
use futures::channel::oneshot::{self, Receiver, Sender};
use mm2_core::mm_ctx::MmArc;
use mm2_event_stream::{behaviour::{EventBehaviour, EventInitStatus},
                       Event, EventStreamConfiguration};
use mm2_number::BigDecimal;
use std::collections::HashMap;

use super::utxo_standard::UtxoStandardCoin;
use crate::{utxo::{output_script, rpc_clients::electrum_script_hash, utxo_tx_history_v2::UtxoTxHistoryOps},
            MarketCoinOps, MmCoin};

#[async_trait]
impl EventBehaviour for UtxoStandardCoin {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, _interval: f64, tx: oneshot::Sender<EventInitStatus>) {
        const RECEIVER_DROPPED_MSG: &str = "Receiver is dropped, which should never happen.";

        let ctx = match MmArc::from_weak(&self.as_ref().ctx) {
            Some(ctx) => ctx,
            None => {
                let msg = "MM context must have been initialized already.";
                tx.send(EventInitStatus::Failed(msg.to_owned()))
                    .expect(RECEIVER_DROPPED_MSG);
                panic!("{}", msg);
            },
        };

        let addresses = match self.my_addresses().await {
            Ok(t) => t,
            Err(e) => {
                tx.send(EventInitStatus::Failed(e.to_string()))
                    .expect(RECEIVER_DROPPED_MSG);
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
                    .expect(RECEIVER_DROPPED_MSG);
                panic!("{}", e);
            }
        }

        tx.send(EventInitStatus::Success).expect(RECEIVER_DROPPED_MSG);

        let mut current_balances: HashMap<String, BigDecimal> = HashMap::new();
        loop {
            match self
                .as_ref()
                .scripthash_notification_receiver
                .as_ref()
                .expect("Can not be `None`")
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

            let new_balances = match self.my_addresses_balances().await {
                Ok(t) => t,
                _ => continue,
            };

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

            // TODO: broadcast multiple updates at once
            for (address, balance) in updated_parts {
                let payload = json!({
                    "ticker": self.ticker(),
                    "address": address,
                    "balance": { "spendable": balance, "unspendable": BigDecimal::default()  }
                });

                ctx.stream_channel_controller
                    .broadcast(Event::new(Self::EVENT_NAME.to_string(), payload.to_string()))
                    .await;
            }

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
