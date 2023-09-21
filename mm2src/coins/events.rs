use crate::MmCoin;
use async_trait::async_trait;

#[async_trait]
pub trait BalanceEvent: MmCoin {
    async fn stream(&self);
}
