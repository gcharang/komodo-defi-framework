use mm2_number::{BigDecimal, BigRational};
use mm2_rpc_data::legacy::{HistoricalOrder, MakerMatchForRpc, OrderConfirmationsSettings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

use crate::activation_scheme_db::{get_activation_scheme, init_activation_scheme};
use crate::adex_config::AdexConfigImpl;
use crate::adex_proc::ResponseHandlerImpl;
use crate::cli::Cli;

const FAKE_SERVER_COOLDOWN_TIMEOUT_MS: u64 = 10;
const FAKE_SERVER_WARMUP_TIMEOUT_MS: u64 = 100;

#[tokio::test]
async fn test_get_version() {
    tokio::spawn(fake_mm2_server(7784, "src/tests/version.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7784");
    let args = vec!["adex-cli", "version"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(
        "Version: 1.0.1-beta_824ca36f3\nDatetime: 2023-04-06T22:35:43+05:00\n",
        result
    );
}

#[tokio::test]
async fn test_get_orderbook() {
    tokio::spawn(fake_mm2_server(7785, "src/tests/orderbook.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7785");
    let args = vec!["adex-cli", "orderbook", "RICK", "MORTY"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(RICK_AND_MORTY_ORDERBOOK, result);
}

#[tokio::test]
async fn test_get_orderbook_with_uuids() {
    tokio::spawn(fake_mm2_server(7786, "src/tests/orderbook.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7786");
    let args = vec!["adex-cli", "orderbook", "RICK", "MORTY", "--uuids"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(RICK_AND_MORTY_ORDERBOOK_WITH_UUIDS, result);
}

#[tokio::test]
async fn test_get_orderbook_with_publics() {
    tokio::spawn(fake_mm2_server(7787, "src/tests/orderbook.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7787");
    let args = vec!["adex-cli", "orderbook", "RICK", "MORTY", "--publics"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(RICK_AND_MORTY_ORDERBOOK_WITH_PUBLICS, result);
}

#[tokio::test]
async fn test_get_enabled() {
    tokio::spawn(fake_mm2_server(7788, "src/tests/get_enabled.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7788");
    let args = vec!["adex-cli", "get-enabled"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(ENABLED_COINS, result);
}

#[tokio::test]
async fn test_get_balance() {
    tokio::spawn(fake_mm2_server(7789, "src/tests/balance.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7789");
    let args = vec!["adex-cli", "balance", "RICK"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(RICK_BALANCE, result);
}

#[tokio::test]
async fn test_enable() {
    tokio::spawn(fake_mm2_server(7790, "src/tests/enable.http"));
    test_activation_scheme().await;
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7790");
    let args = vec!["adex-cli", "enable", "ETH"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(ENABLE_OUTPUT, result);
}

async fn test_activation_scheme() {
    init_activation_scheme().await.unwrap();
    let scheme = get_activation_scheme();
    let kmd_scheme = scheme.get_activation_method("KMD");
    assert!(kmd_scheme.is_some());
    let kmd_scheme = kmd_scheme.unwrap();
    assert_eq!(kmd_scheme.get("method").unwrap().as_str().unwrap(), "electrum");
    assert_eq!(kmd_scheme.get("servers").unwrap().as_array().unwrap().iter().count(), 3);
}

#[tokio::test]
async fn test_buy_morty_for_rick() {
    tokio::spawn(fake_mm2_server(7791, "src/tests/buy.http"));
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_WARMUP_TIMEOUT_MS)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7791");
    let args = vec!["adex-cli", "buy", "MORTY", "RICK", "0.01", "0.5"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!("4685e133-dfb3-4b31-8d4c-0ffa79933c8e\n", result);
}

#[derive(Serialize, Deserialize)]
pub struct MakerOrderForRpcc {
    pub base: String,
    pub rel: String,
    pub price: BigDecimal,
    pub price_rat: BigRational,
    pub max_base_vol: BigDecimal,
    pub max_base_vol_rat: BigRational,
    pub min_base_vol: BigDecimal,
    pub min_base_vol_rat: BigRational,
    pub created_at: u64,
    pub updated_at: Option<u64>,
    pub matches: HashMap<Uuid, MakerMatchForRpc>,
    pub started_swaps: Vec<Uuid>,
    pub uuid: Uuid,
    pub conf_settings: Option<OrderConfirmationsSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes_history: Option<Vec<HistoricalOrder>>,
    pub base_orderbook_ticker: Option<String>,
    pub rel_orderbook_ticker: Option<String>,
}

#[tokio::test]
async fn test_order_status() {
    tokio::spawn(fake_mm2_server(7792, "src/tests/taker_status.http"));
    tokio::time::sleep(Duration::from_micros(100)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7792");
    let args = vec!["adex-cli", "order-status", "b7611502-eae8-4855-8bd7-16d992f952bf"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();

    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(TAKER_STATUS_OUTPUT, result);
}

#[tokio::test]
async fn test_my_orders() {
    tokio::spawn(fake_mm2_server(7793, "src/tests/my_orders.http"));
    tokio::time::sleep(Duration::from_micros(100)).await;
    let mut buffer: Vec<u8> = vec![];
    let response_handler = ResponseHandlerImpl {
        writer: (&mut buffer as &mut dyn Write).into(),
    };
    let config = AdexConfigImpl::new("dummy", "http://127.0.0.1:7793");
    let args = vec!["adex-cli", "my-orders"];
    Cli::execute(args.iter().map(|arg| arg.to_string()), &config, &response_handler)
        .await
        .unwrap();
    let result = String::from_utf8(buffer).unwrap();
    assert_eq!(MY_ORDERS_OUTPUT, result);
}

async fn fake_mm2_server(port: u16, response_path: &'static str) {
    let server = TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("Failed to bind tcp server");

    if let Ok((stream, _)) = server.accept().await {
        tokio::spawn(handle_connection(stream, response_path));
    }
}

async fn handle_connection(mut stream: TcpStream, response_path: &'static str) {
    let mut file = File::open(response_path).unwrap();
    let mut buffer: Vec<u8> = vec![];
    file.read_to_end(&mut buffer).unwrap();
    let (reader, mut writer) = stream.split();
    reader.readable().await.unwrap();
    writer.write_all(&buffer).await.unwrap();
    tokio::time::sleep(Duration::from_millis(FAKE_SERVER_COOLDOWN_TIMEOUT_MS)).await;
}

const RICK_AND_MORTY_ORDERBOOK: &str = r"     Volume: RICK Price: MORTY  
             0.23 1.00000000    
        340654.03 1.00000000    
             2.00 0.99999999    
             2.00 0.99999999    
             2.00 0.99999999    
- --------------- ------------- 
             0.96 1.02438024    
             1.99 1.00000001    
             1.99 1.00000001    
             1.99 1.00000001    
         32229.14 1.00000000    
             0.22 1.00000000    
";

const RICK_AND_MORTY_ORDERBOOK_WITH_UUIDS: &str = r"     Volume: RICK Price: MORTY  Uuid                                 
             0.23 1.00000000    c7585a1b-6060-4319-9da6-c67321628a06 
        340654.03 1.00000000    d69fe2a9-51ca-4d69-96ad-b141a01d8bb4 
             2.00 0.99999999    a2337218-7f6f-46a1-892e-6febfb7f5403 
             2.00 0.99999999    c172c295-7fe3-4131-9c81-c3a7182f0617 
             2.00 0.99999999    fbbc44d2-fb50-4b4b-8ac3-d9857cae16b6 
- --------------- ------------- ------------------------------------ 
             0.96 1.02438024    c480675b-3352-4159-9b3c-55cb2b1329de 
             1.99 1.00000001    fdb0de9c-e283-48c3-9de6-8117fecf0aff 
             1.99 1.00000001    6a3bb75d-8e91-4192-bf50-d8190a69600d 
             1.99 1.00000001    b24b40de-e93d-4218-8d93-1940ceadce7f 
         32229.14 1.00000000    652a7e97-f42c-4f87-bc26-26bd1a0fea24 
             0.22 1.00000000    1082c93c-8c23-4944-b8f1-a92ec703b03a 
";

const RICK_AND_MORTY_ORDERBOOK_WITH_PUBLICS: &str = r"     Volume: RICK Price: MORTY  Public                                                             
             0.23 1.00000000    022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846 
        340654.03 1.00000000    0315d9c51c657ab1be4ae9d3ab6e76a619d3bccfe830d5363fa168424c0d044732 
             2.00 0.99999999    037310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5 
             2.00 0.99999999    037310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5 
             2.00 0.99999999    037310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5 
- --------------- ------------- ------------------------------------------------------------------ 
             0.96 1.02438024    02d6c3e22a419a4034272acb215f1d39cd6a0413cfd83ac0c68f482db80accd89a 
             1.99 1.00000001    037310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5 
             1.99 1.00000001    037310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5 
             1.99 1.00000001    037310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5 
         32229.14 1.00000000    0315d9c51c657ab1be4ae9d3ab6e76a619d3bccfe830d5363fa168424c0d044732 
             0.22 1.00000000    022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846 
";

const ENABLED_COINS: &str = r"Ticker   Address
MORTY    RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
RICK     RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
KMD      RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
ETH      0x224050fb7EB13Fa0D342F5b245f1237bAB531650
";

const RICK_BALANCE: &str = r"coin: RICK
balance: 0.5767226
unspendable: 0
address: RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
";

const ENABLE_OUTPUT: &str = r"coin: ETH
address: 0x224050fb7EB13Fa0D342F5b245f1237bAB531650
balance: 0.02
unspendable_balance: 0
required_confirmations: 3
requires_notarization: No
";

// TODO: last updated should not be 0, check it
const TAKER_STATUS_OUTPUT: &str = r"                uuid: 1ae94a08-47e3-4938-bebb-5df8ff74b8e0
      req.(base,rel): MORTY(0.01), RICK(0.01000001)
          req.action: Buy
  req.(sender, dest): 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d, 0000000000000000000000000000000000000000000000000000000000000000
        req.match_by: Any
   req.conf_settings: 111,true:555,true
          created_at: 23-05-11 19:28:46
          order_type: GoodTillCancelled
         cancellable: false
             matches: 
                      uuid: 600f62b3-5248-4905-9618-14f339cc7d30
       reserved.(base,rel): MORTY(0.01), RICK(0.0099999999)
   reserved.(taker, maker): 1ae94a08-47e3-4938-bebb-5df8ff74b8e0,600f62b3-5248-4905-9618-14f339cc7d30
   reserved.(sender, dest): 7310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5,0000000000000000000000000000000000000000000000000000000000000000
    reserved.conf_settings: 1,false:1,false
              last_updated: 0
     connect.(taker,maker): 1ae94a08-47e3-4938-bebb-5df8ff74b8e0,600f62b3-5248-4905-9618-14f339cc7d30
    connect.(sender, dest): 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d,7310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5
";

const MY_ORDERS_OUTPUT: &str = "        Taker orders: 
┌──────────────────────────┬──────────────────────────────────────────────────────────────────┬──────────────────────────┬──────────────────────────┬──────────────────────────┬───────────────────────────┐
│ action                   │ uuid, sender, dest                                               │ type,created_at          │ match_by                 │ base,rel                 │ cancellable               │
│ base(vol),rel(vol)       │                                                                  │ confirmation             │                          │ orderbook ticker         │                           │
├──────────────────────────┼──────────────────────────────────────────────────────────────────┼──────────────────────────┼──────────────────────────┼──────────────────────────┼───────────────────────────┤
│ Buy                      │ 2739152a-3f87-4f6d-a199-3659aa1e864f                             │ GoodTillCancelled        │ Any                      │ none                     │ true                      │
│ MORTY(0.10),RICK(0.09)   │ 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d │ 23-05-29 12:18:52        │                          │ none                     │                           │
│                          │ 0000000000000000000000000000000000000000000000000000000000000000 │ 1,false:1,false          │                          │                          │                           │
├──────────────────────────┼──────────────────────────────────────────────────────────────────┼──────────────────────────┼──────────────────────────┼──────────────────────────┼───────────────────────────┤
│ Buy                      │ ce90f89f-8074-4e9f-8649-7f7689c56fa9                             │ GoodTillCancelled        │ Any                      │ none                     │ false                     │
│ MORTY(0.10),RICK(0.11)   │ 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d │ 23-05-29 12:19:10        │                          │ none                     │                           │
│                          │ 0000000000000000000000000000000000000000000000000000000000000000 │ 1,false:1,false          │                          │                          │                           │
├──────────────────────────┴──────────────────────────────────────────────────────────────────┴──────────────────────────┴──────────────────────────┴──────────────────────────┴───────────────────────────┤
│ matches                                                                                                                                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│                       uuid: 09a0e11e-837e-4763-bc1f-1659573df9dd                                                                                                                                         │
│        reserved.(base,rel): MORTY(0.1), RICK(0.099999999)                                                                                                                                                │
│    reserved.(taker, maker): ce90f89f-8074-4e9f-8649-7f7689c56fa9,09a0e11e-837e-4763-bc1f-1659573df9dd                                                                                                    │
│    reserved.(sender, dest): 7310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5,0000000000000000000000000000000000000000000000000000000000000000                                            │
│     reserved.conf_settings: 1,false:1,false                                                                                                                                                              │
│               last_updated: 0                                                                                                                                                                            │
│      connect.(taker,maker): ce90f89f-8074-4e9f-8649-7f7689c56fa9,09a0e11e-837e-4763-bc1f-1659573df9dd                                                                                                    │
│     connect.(sender, dest): 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d,7310a8fb9fd8f198a1a21db830252ad681fccda580ed4101f3f6bfb98b34fab5                                            │
│                                                                                                                                                                                                          │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

        Maker orders: 
┌────────────┬───────┬──────────────────────────────────────┬────────────────────┬───────────────┬─────────────┬───────────┬───────┬─────────────────┬─────────────────┐
│ base,rel   │ price │ uuid                                 │ created at,        │ min base vol, │ cancellable │ available │ swaps │ conf_settings   │ history changes │
│            │       │                                      │ updated at         │ max base vol  │             │ amount    │       │                 │                 │
├────────────┼───────┼──────────────────────────────────────┼────────────────────┼───────────────┼─────────────┼───────────┼───────┼─────────────────┼─────────────────┤
│ RICK,MORTY │ 1.11  │ 28315c31-4fd7-4847-9873-352924252fbe │ 23-05-29 12:17:46, │ 0.000100,     │ true        │ 0.09      │ empty │ 1,false:1,false │ none            │
│            │       │                                      │ 23-05-29 12:17:46  │ 0.09          │             │           │       │                 │                 │
├────────────┼───────┼──────────────────────────────────────┼────────────────────┼───────────────┼─────────────┼───────────┼───────┼─────────────────┼─────────────────┤
│ RICK,MORTY │ 1.11  │ 7f097435-f482-415b-9bdf-6780f4be4828 │ 23-05-29 12:17:49, │ 0.000100,     │ true        │ 0.09      │ empty │ 1,false:1,false │ none            │
│            │       │                                      │ 23-05-29 12:17:49  │ 0.09          │             │           │       │                 │                 │
└────────────┴───────┴──────────────────────────────────────┴────────────────────┴───────────────┴─────────────┴───────────┴───────┴─────────────────┴─────────────────┘

";
