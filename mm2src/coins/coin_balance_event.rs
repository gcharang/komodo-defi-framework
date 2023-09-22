use crate::{CoinsContext, MmCoinEnum};
use async_trait::async_trait;
use common::{executor::{SpawnFuture, Timer},
             log::info};
use mm2_core::mm_ctx::MmArc;
use mm2_event_stream::{behaviour::EventBehaviour, EventStreamConfiguration};
use std::sync::atomic::Ordering;

pub struct CoinBalanceEvent {
    ctx: MmArc,
}

impl CoinBalanceEvent {
    pub fn new(ctx: MmArc) -> Self { Self { ctx } }
}

#[async_trait]
impl EventBehaviour for CoinBalanceEvent {
    const EVENT_NAME: &'static str = "COIN_BALANCE";

    async fn handle(self, interval: f64) {
        let cctx = CoinsContext::from_ctx(&self.ctx).expect("Unexpected internal panic.");

        // Events that are already fired
        let mut event_pool: Vec<String> = vec![];

        loop {
            let coins_mutex = cctx.coins.lock().await;

            let coins: Vec<MmCoinEnum> = coins_mutex
                .values()
                .filter_map(|coin| {
                    // We loop this over and over, so it's not necessary to sequentially load the atomics all over
                    // the threads, since the cost of it is way too higher than the `AtomicOrdering::Relaxed`
                    if coin.is_available.load(Ordering::Relaxed) && coin.inner.is_platform_coin() {
                        Some(coin.inner.clone())
                    } else {
                        None
                    }
                })
                .collect();

            // Similar to above, we don't need to held the lock(which will block all other processes that depends
            // on this lock(like coin activation)) since we loop this over continuously.
            drop(coins_mutex);

            for coin in coins {
                let ticker = coin.ticker().to_owned();

                if event_pool.contains(&ticker) {
                    continue;
                }

                match coin {
                    MmCoinEnum::TendermintToken(_) => unreachable!(),
                    MmCoinEnum::Tendermint(_) => {
                        println!("TODO: here we will spawn a thread for tendermint balance handler which uses socket connection under the hood.");
                    },
                    MmCoinEnum::UtxoCoin(_) => todo!(),
                    MmCoinEnum::QtumCoin(_) => todo!(),
                    MmCoinEnum::Qrc20Coin(_) => todo!(),
                    MmCoinEnum::EthCoin(_) => todo!(),
                    MmCoinEnum::ZCoin(_) => todo!(),
                    MmCoinEnum::Bch(_) => todo!(),
                    MmCoinEnum::SlpToken(_) => todo!(),
                    MmCoinEnum::LightningCoin(_) => todo!(),
                    MmCoinEnum::Test(_) => todo!(),
                    #[cfg(all(
                        feature = "enable-solana",
                        not(target_os = "ios"),
                        not(target_os = "android"),
                        not(target_arch = "wasm32")
                    ))]
                    MmCoinEnum::SolanaCoin(_) | MmCoinEnum::SplToken(_) => todo!(),
                }

                event_pool.push(ticker);
            }

            Timer::sleep(interval).await;
        }
    }

    fn spawn_if_active(self, config: &EventStreamConfiguration) {
        if let Some(event) = config.get_event(Self::EVENT_NAME) {
            info!(
                "NETWORK event is activated with {} seconds interval.",
                event.stream_interval_seconds
            );
            self.ctx.spawner().spawn(self.handle(event.stream_interval_seconds));
        }
    }
}
