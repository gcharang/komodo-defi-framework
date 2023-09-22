use crate::{CoinsContext, MmCoin, MmCoinEnum};
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
                    if coin.is_available.load(Ordering::Relaxed) {
                        Some(coin.inner.clone())
                    } else {
                        None
                    }
                })
                .collect();

            // Similar to above, we don't need to held the lock(which will block all other processes that depends
            // on this lock(like coin activation)) since we loop this over continuously.
            drop(coins_mutex);

            // Handle balance streaming concurrently for each coin
            for coin in coins {
                let ticker = coin.ticker().to_owned();

                if event_pool.contains(&ticker) {
                    continue;
                }

                match coin {
                    MmCoinEnum::UtxoCoin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::QtumCoin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::Qrc20Coin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::EthCoin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::ZCoin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::Bch(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::SlpToken(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::Tendermint(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::TendermintToken(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::LightningCoin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    MmCoinEnum::Test(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    #[cfg(all(
                        feature = "enable-solana",
                        not(target_os = "ios"),
                        not(target_os = "android"),
                        not(target_arch = "wasm32")
                    ))]
                    MmCoinEnum::SolanaCoin(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
                    #[cfg(all(
                        feature = "enable-solana",
                        not(target_os = "ios"),
                        not(target_os = "android"),
                        not(target_arch = "wasm32")
                    ))]
                    MmCoinEnum::SplToken(inner) => self.ctx.spawner().spawn(inner.handle_balance_stream()),
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
