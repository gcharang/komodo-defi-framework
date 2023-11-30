use async_trait::async_trait;
use common::{executor::{AbortSettings, SpawnAbortable},
             log, Future01CompatExt};
use futures::channel::oneshot::{self, Receiver, Sender};
use futures_util::StreamExt;
use keys::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_event_stream::{behaviour::{EventBehaviour, EventInitStatus},
                       Event, EventStreamConfiguration};
use std::collections::{BTreeMap, HashSet};

use super::utxo_standard::UtxoStandardCoin;
use crate::{utxo::{output_script, rpc_clients::electrum_script_hash, utxo_common::address_balance,
                   utxo_tx_history_v2::UtxoTxHistoryOps, ScripthashNotification, UtxoCoinFields},
            MarketCoinOps, MmCoin};

#[async_trait]
impl EventBehaviour for UtxoStandardCoin {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, _interval: f64, tx: oneshot::Sender<EventInitStatus>) {
        const RECEIVER_DROPPED_MSG: &str = "Receiver is dropped, which should never happen.";

        async fn subscribe_to_addresses(
            utxo: &UtxoCoinFields,
            addresses: HashSet<Address>,
        ) -> Result<BTreeMap<String, Address>, String> {
            let mut scripthash_to_address_map: BTreeMap<String, Address> = BTreeMap::new();

            for address in addresses {
                let script = output_script(&address, keys::Type::P2PKH);
                let script_hash = electrum_script_hash(&script);
                let scripthash = hex::encode(script_hash);

                scripthash_to_address_map.insert(scripthash.clone(), address);

                if let Err(e) = utxo
                    .rpc_client
                    .blockchain_scripthash_subscribe(scripthash)
                    .compat()
                    .await
                {
                    return Err(e.to_string());
                }
            }

            Ok(scripthash_to_address_map)
        }

        let ctx = match MmArc::from_weak(&self.as_ref().ctx) {
            Some(ctx) => ctx,
            None => {
                let msg = "MM context must have been initialized already.";
                tx.send(EventInitStatus::Failed(msg.to_owned()))
                    .expect(RECEIVER_DROPPED_MSG);
                panic!("{}", msg);
            },
        };

        let scripthash_notification_handler = match self.as_ref().scripthash_notification_handler.as_ref() {
            Some(t) => t,
            None => {
                let e = "Scripthash notification receiver can not be empty.";
                tx.send(EventInitStatus::Failed(e.to_string()))
                    .expect(RECEIVER_DROPPED_MSG);
                panic!("{}", e);
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

        let mut scripthash_to_address_map = match subscribe_to_addresses(self.as_ref(), addresses).await {
            Ok(t) => t,
            Err(e) => {
                tx.send(EventInitStatus::Failed(e.clone())).expect(RECEIVER_DROPPED_MSG);
                panic!("{}", e);
            },
        };

        tx.send(EventInitStatus::Success).expect(RECEIVER_DROPPED_MSG);

        while let Some(message) = scripthash_notification_handler.lock().await.next().await {
            let my_addresses = match self.my_addresses().await {
                Ok(t) => t,
                Err(e) => {
                    log::error!("{e}");
                    continue;
                },
            };

            let notified_scripthash = match message {
                ScripthashNotification::Triggered(t) => t,
                ScripthashNotification::ConnectionLost => {
                    match subscribe_to_addresses(self.as_ref(), my_addresses).await {
                        Ok(map) => scripthash_to_address_map = map, // update the map with fresh one
                        Err(e) => {
                            log::error!("{e}");
                        },
                    };

                    continue;
                },
            };

            let address = scripthash_to_address_map
                .get(&notified_scripthash)
                .cloned()
                .or_else(|| {
                    my_addresses.into_iter().find_map(|addr| {
                        let script = output_script(&addr, keys::Type::P2PKH);
                        let script_hash = electrum_script_hash(&script);
                        let scripthash = hex::encode(script_hash);

                        if notified_scripthash == scripthash {
                            scripthash_to_address_map.insert(notified_scripthash.clone(), addr.clone());
                            Some(addr)
                        } else {
                            None
                        }
                    })
                });

            let address = match address {
                Some(t) => t,
                None => {
                    log::debug!(
                        "Couldn't find the relevant address for {} scripthash.",
                        notified_scripthash
                    );
                    continue;
                },
            };

            let balance = match address_balance(&self, &address).await {
                Ok(t) => t,
                Err(e) => {
                    log::error!("{e}");
                    continue;
                },
            };

            let payload = json!({
                "ticker": self.ticker(),
                "address": address.to_string(),
                "balance": { "spendable": balance.spendable, "unspendable": balance.unspendable }
            });

            ctx.stream_channel_controller
                .broadcast(Event::new(
                    Self::EVENT_NAME.to_string(),
                    json!(vec![payload]).to_string(),
                ))
                .await;
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
