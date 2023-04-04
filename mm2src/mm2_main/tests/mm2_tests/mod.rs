mod bch_and_slp_tests;
mod best_orders_tests;
mod eth_tests;
mod lightning_tests;
mod lp_bot_tests;
mod mm2_tests_inner;
mod nucleus_swap;
mod orderbook_sync_tests;
mod tendermint_ibc_asset_tests;
mod tendermint_tests;
mod z_coin_tests;

// dummy test helping IDE to recognize this as test module
#[test]
#[allow(clippy::assertions_on_constants)]
fn dummy() { assert!(true) }
