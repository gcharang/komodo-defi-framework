use async_trait::async_trait;

use super::TendermintCoin;
use crate::events::BalanceEvent;

#[async_trait]
impl BalanceEvent for TendermintCoin {
    async fn stream(&self) {}
}
