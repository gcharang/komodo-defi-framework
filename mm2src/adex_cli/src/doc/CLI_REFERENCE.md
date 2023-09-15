```toc
```


`komodefi-cli` provides a CLI interface and facilitates interoperating to komodo defi platform via the `mm2` service. It's a multi-purpose utility that facilitates using komodo platform to be used as a multi-chain Wallet and DEX at the same time.

## Manage mm2 service

`komodefi-cli`  should be configured in a proper way to be able to connect to and interract with the running `mm2` service via the Komodo RPC API. The `mm2` can be started manually or by using the `komodefi-cli` that facilitates configuring and managing it as a process.

### init

The `init` command is aiming to facilitate creating of the `MM2.json` configuration file and getting the `coins` configuration. The `komodefi-cli` implements it in an interactive step-by-step mode and produces `MM2.json` as a result. `coins` configuration is got from the https://raw.githubusercontent.com/KomodoPlatform/coins/master/coins. The `init` also provides alternative paths setting by additional options.

```sh
komodefi-cli init --help  
Initialize a predefined coin set and configuration to start mm2 instance with  
  
Usage: komodefi-cli init [OPTIONS]  
  
Options:  
     --mm-coins-path <MM_COINS_PATH>  Coin set file path [default: coins] [aliases: coins]  
     --mm-conf-path <MM_CONF_PATH>    mm2 configuration file path [default: MM2.json] [aliases: conf]  
 -h, --help                           Print help
```

**Example**:

```sh
RUST_LOG=error komodefi-cli init  
> What is the network `mm2` is going to be a part, netid: 7777  
> What is the seed phrase: upgrade hunt engage mountain cheap hood attitude bleak flag wild feature aim  
> Allow weak password: No  
> What is the rpc_password: D6~$jETp  
> What is dbdir None  
> What is rpcip: None  
> What is the rpcport: None  
> What is rpc_local_only:    
> What is i_am_a_seed:    
? What is the next seednode: Tap enter to skip    
[Optional. If operating on a test or private netID, the IP address of at least one seed node is required (on the main network, these are already hardcoded)]
...
```

resulting `MM2.json`:

```json
{  
 "gui": "komodefi-cli",  
 "netid": 7777,  
 "rpc_password": "D6~$jETp",  
 "passphrase": "upgrade hunt engage mountain cheap hood attitude bleak flag wild feature aim",  
 "allow_weak_password": false
}
```

### mm2 start

The `start` command is used to start `mm2` instance. Options are used to be set as an environment variables of the `mm2 `: `MM_CONF_PATH`, `MM_COINS_PATH` and `MM_LOG` accordingly.

```sh
komodefi-cli mm2 start -help  
Start mm2 instance  
  
Usage: komodefi-cli mm2 start [OPTIONS]  
  
Options:  
     --mm-conf-path <MM_CONF_PATH>    mm2 configuration file path [aliases: conf]  
     --mm-coins-path <MM_COINS_PATH>  Coin set file path [aliases: coins]  
     --mm-log <MM_LOG>                Log file path [aliases: log]  
 -h, --help                           Print help
```

**Example**:

```sh
komodefi-cli mm2 start --mm-log mm2.log  
Set env MM_LOG as: mm2.log  
Started child process: "mm2", pid: 459264
```
### mm2 check

The `check` command gets the state of the running `mm2` instance

```sh
komodefi-cli mm2 check --help  
Check if mm2 is running  
  
Usage: komodefi-cli mm2 check  
  
Options:  
 -h, --help  Print help
```

**Example**:

```sh
komodefi-cli mm2 check    
Found mm2 is running, pid: 459264
```

```sh
komodefi-cli mm2 check    
Process not found: mm2
```
### mm2 version

The `version` command requests the version of `mm2` using the [`version` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/version.html)

```sh
komodefi-cli mm2 version --help  
Get version of intermediary mm2 service  
  
Usage: komodefi-cli mm2 version  
  
Options:  
 -h, --help  Print help
```

**Example**:

```sh
komodefi-cli mm2 version  
Version: 1.0.6-beta_9466e53c6  
Datetime: 2023-09-11T16:19:16+05:00
```

### mm2 stop

The `stop` command requests stopping of mm2 using the [`stop` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/stop.html)

```sh
komodefi-cli mm2 stop --help  
Stop mm2 using API  
  
Usage: komodefi-cli mm2 stop  
  
Options:  
 -h, --help  Print help
```

**Example**:

```sh
komodefi-cli mm2 stop    
Sending stop command  
Service stopped: Success
```

### mm2 kill

The `kill` command kills the process of `mm2` if it is currently running

```sh
komodefi-cli mm2 kill --help  
Kill mm2 process  
  
Usage: komodefi-cli mm2 kill  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli mm2 kill    
Process killed: mm2:505606
```

## Configuring `komodefi-cli`

Config commands are `set` and `get`.  These are aimed to set and get password and uri to connect to and be granted to request methods from the running `mm2` instance. Password is setting in an interactive mode. The certain path of the configuration depends on the operating system the `komodefi-cli` is running on and could be different.

```sh
komodefi-cli config set --help  
Set komodo komodefi cli configuration  
  
Usage: komodefi-cli config set <--password|--uri <URI>>  
  
Options:  
 -p, --password   Set if you are going to set up a password  
 -u, --uri <URI>  KomoDeFi RPC API Uri. http://localhost:7783 [aliases: url]  
 -h, --help       Print help
```

**Examples:**

Setting configuration:

```sh
komodefi-cli config set -u https://localhost:7783 -p  
? Enter RPC API password:  
? Confirmation:
```

resulting `komodefi_cfg.json`:

```
cat ~/.config/komodefi-cli/komodefi_cfg.json    
{  
 "rpc_password": "D6~$jETp",  
 "rpc_uri": "https://localhost:7783"  
}
```

Getting configuration:

```sh
release/komodefi-cli config get    
mm2 RPC URL: https://localhost:7783  
mm2 RPC password: *************
```

## Setting up coin index

To use the komodo platform as both a multi-chain wallet and DEX, there are commands to include and exclude certain coins from the wallet index. RPC API methods can mainly be requested to be used with only enabled coins.

```sh
komodefi-cli coin  
Coin commands: enable, disable etc.  
  
Usage: komodefi-cli coin <COMMAND>  
  
Commands:  
 enable               Put a coin to the trading index  
 disable              Deactivates enabled coin and also cancels all active orders that use the selected coin.  
 get-enabled          List activated coins [aliases: enabled]  
 set-required-conf    Set the number of confirmations to wait for the selected coin [aliases: set-conf]  
 set-required-nota    Whether to wait for a dPoW notarization of the given atomic swap transactions [aliases: set-nota]  
 coins-to-kick-start  Return the coins that should be activated to continue the interrupted swaps [aliases: to-kick]  
 help                 Print this message or the help of the given subcommand(s)  
  
Options:  
 -h, --help  Print help
```
### coin enable

The `enable` command includes the given coin in the komodo wallet index. Depending on the given coin the different RPC API method could be requested. For the [ZHTLC related method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-20-dev/zhtlc_coins.html#task-enable-z-coin-init) - `--keep-progress` option is designed to request the status of the enabling task every N seconds. The `--tx-history` option overrides the predefined setting to make it able to request transactions history for the given coin.

```sh
komodefi-cli coin enable --help  
Put a coin to the trading index  
  
Usage: komodefi-cli coin enable [OPTIONS] <COIN>  
  
Arguments:  
 <COIN>  Coin to be included into the trading index  
  
Options:  
 -k, --keep-progress <KEEP_PROGRESS>  Whether to keep progress on task based commands [default: 0] [aliases: track, keep, progress]  
 -H, --tx-history                     Whether to save tx history for the coin [aliases: history]  
 -h, --help                           Print help
```

*Notice: To override required conf and nota `set-required-conf` and `set-required-nota` commands could be used*

**Examples**

Enabling `DOC` using the ([legacy method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/coin_activation.html))

```sh
komodefi-cli coin enable --tx-history DOC  
Enabling coin: DOC  
coin: DOC  
address: RNfMoWT47LWht7wwixJdx9QP51mkakLVsU  
balance: 0  
unspendable_balance: 0  
required_confirmations: 1  
requires_notarization: No  
mature_confirmations: 100
```

Enabling `tBCH` ([v2.0 method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-20/enable_bch_with_tokens.html))

```sh
komodefi-cli coin enable tBCH  
Enabling coin: tBCH  
current_block: 1570304  
bch_addresses_infos:    
│ address, pubkey                                   │ method │ balance(sp,unsp) │ tickers │  
│ bchtest:qzvnf6l254ktt97fx657mqszmrvh5znsqvs26sxf6t│ Iguana │ 0.05:0           │ none    │  
│ 02264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d...│        │                  │         │  
  
slp_addresses_infos:    
│ address, pubkey                                   │ method │ balance(sp,unsp) │ tickers │  
│ slptest:qzvnf6l254ktt97fx657mqszmrvh5znsqvt7atu7gk│ Iguana │ {}               │ none    │  
│ 02264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d...│        │                  │         │
```

Enable ZHTLC `ZOMBIE` COIN ([`task_enable_z_coin_init` method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-20-dev/zhtlc_coins.html#task-enable-z-coin-init))

```sh
komodefi-cli coin enable ZOMBIE --track 5  
Enabling coin: ZOMBIE  
Enabling zcoin started, task_id: 0  
Getting enable zcoin task status  
In progress: Activating coin  
In progress: Activating coin  
In progress: Activating coin  
In progress: Activating coin  
In progress: Updating block cache, current_scanned_block: 58980, latest_block: 77153  
In progress: Building wallet db, current_scanned_block: 56397, latest_block: 77153  
In progress: Building wallet db, current_scanned_block: 60397, latest_block: 77153  
In progress: Building wallet db, current_scanned_block: 65397, latest_block: 77153  
In progress: Building wallet db, current_scanned_block: 69397, latest_block: 77153  
In progress: Building wallet db, current_scanned_block: 73397, latest_block: 77153  
status: OK  
current_block: 77154  
ticker: ZOMBIE  
iguana wallet:    
            address: zs1r0fzx9unydgfty74z5d4qkvjyaky0n73ms4cvhttj4234s6rf0hfju5faf6a5nzlwv5qgrr0pen  
            balance: 50.77:0
```

```sh
komodefi-cli coin enable ZOMBIE  
Enabling coin: ZOMBIE  
Enabling zcoin started, task_id: 1

komodefi-cli task status zcoin 1  
Getting enable zcoin task status  
status: OK  
current_block: 77154  
ticker: ZOMBIE  
iguana wallet:    
            address: zs1r0fzx9unydgfty74z5d4qkvjyaky0n73ms4cvhttj4234s6rf0hfju5faf6a5nzlwv5qgrr0pen  
            balance: 50.77:0
```
### coin disable

The `disable` command excludes the given coin from the komodo wallet index using the [disable_coin RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/disable_coin.html).

```sh
komodefi-cli coin disable --help  
Deactivates enabled coin and also cancels all active orders that use the selected coin.  
  
Usage: komodefi-cli coin disable <COIN>  
  
Arguments:  
 <COIN>  Coin to disable  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
Disabling coin: DOC  
coin: DOC  
cancelled_orders: 8339efe3-ee0a-436c-ad6a-88ef8130ba2b  
passivized: false
```
### coin get-enabled (enabled)

The `get-enabled` command lists coins that are already in the wallet index using the [get_enabled_coins RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/get_enabled_coins.html). *Alias: `enabled`*

```sh
komodefi-cli coin get-enabled --help  
List activated coins  
  
Usage: komodefi-cli coin get-enabled  
  
Options:  
 -h, --help  Print help
```

**Example**:

```
komodefi-cli coin enabled  
Getting list of enabled coins ...  
Ticker   Address  
ZOMBIE   zs1r0fzx9unydgfty74z5d4qkvjyaky0n73ms4cvhttj4234s6rf0hfju5faf6a5nzlwv5qgrr0pen  
tBCH     bchtest:qzvnf6l254ktt97fx657mqszmrvh5znsqvs26sxf6t  
MARTY    RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM  
DOC      RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
```
### coin coins-to-kick-start (to-kick)

The `coins-to-kick-start` command lists coins that are involved in swaps that are not done and needed to be anbled to complete them. This command is proceed using the [`coins_needed_to_kick_start` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/coins_needed_for_kick_start.html) *Alias: `to-kick`*

```sh
komodefi-cli coin to-kick --help  
Return the coins that should be activated to continue the interrupted swaps  
  
Usage: komodefi-cli coin coins-to-kick-start  
  
Options:  
 -h, --help  Print help
 ```

**Example:**

```sh
komodefi-cli coin to-kick    
Getting coins needed for kickstart  
coins: RICK, MORTY
```
### coin set-required-conf (set-conf)

The `set-required-conf` command temporary overrides the predefined `required-conf` setting of the given coin using the [`set_required_confirmations` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/set_required_confirmations.html) *Alias: `set-conf`*

```sh
komodefi-cli coin set-conf --help  
Set the number of confirmations to wait for the selected coin  
  
Usage: komodefi-cli coin set-required-conf <COIN> <CONFIRMATIONS>  
  
Arguments:  
 <COIN>           Ticker of the selected coin  
 <CONFIRMATIONS>  Number of confirmations to require [aliases: conf]  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli coin set-conf DOC 3  
Setting required confirmations: DOC, confirmations: 3  
coin: DOC  
confirmations: 3
```

### coin set-required-nota (set-nota)

The `set-required-nota` command temporary overrides the predefined `required-nota` setting of the given coin using the [`set_required_notarization` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/set_requires_notarization.html). *Alias: `set-nota`*

```sh
komodefi-cli coin set-nota --help  
Whether to wait for a dPoW notarization of the given atomic swap transactions  
  
Usage: komodefi-cli coin set-required-nota [OPTIONS] <COIN>  
  
Arguments:  
 <COIN>  Ticker of the selected coin  
  
Options:  
 -n, --requires-notarization  Whether the node should wait for dPoW notarization of atomic swap transactions [aliases: requires-nota]  
 -h, --help                   Print help
```

**Examples:**

Set `requried_nota`:

```sh
komodefi-cli coin set-nota --requires-nota DOC  
Setting required nota: DOC, requires_nota: true  
coin: DOC  
requires_notarization: true
```

Reset `required_nota`:

```sh
komodefi-cli coin set-nota DOC  
Setting required nota: DOC, requires_nota: false  
coin: DOC  
requires_notarization: false
```

## Wallet commands

The further group of commands are aimed to provide base multi-chain wallet functionality. It includes commands like balance, withdraw, tx-history etc.

```sh
komodefi-cli wallet  
Wallet commands: balance, withdraw etc.  
  
Usage: komodefi-cli wallet <COMMAND>  
  
Commands:  
 my-balance            Get coin balance [aliases: balance]  
 withdraw              Generates, signs, and returns a transaction that transfers the amount of coin to the address indicated in the to argument  
 send-raw-transaction  Broadcasts the transaction to the network of selected coin [aliases: send-raw, send]  
 get-raw-transaction   Returns the full signed raw transaction hex for any transaction that is confirmed or within the mempool [aliases: get-raw, raw-tx, get]  
 tx-history            Returns the blockchain transactions involving the Komodo DeFi Framework node's coin address [aliases: history]  
 show-priv-key         Returns the private key of the specified coin in a format compatible with coin wallets [aliases: private, private-key]  
 validate-address      Checks if an input string is a valid address of the specified coin [aliases: validate]  
 kmd-rewards-info      Informs about the active user rewards that can be claimed by an address's unspent outputs [aliases: rewards]  
 convert-address       Converts an input address to a specified address format [aliases: convert]  
 convert-utxo-address  Takes a UTXO address as input, and returns the equivalent address for another UTXO coin (e.g. from BTC address to RVN address) [aliases: convert-utxo]  
 help                  Print this message or the help of the given subcommand(s)  
  
Options:  
 -h, --help  Print help
```

### wallet my-balance (balace)

The `my-balance` command gets balance of the given coin using the [`my_balance` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/my_balance.html). *Alias: `balance`*.

```sh
komodefi-cli wallet balance --help  
Get coin balance  
  
Usage: komodefi-cli wallet my-balance <COIN>  
  
Arguments:  
 <COIN>  Coin to get balance  
  
Options:  
 -h, --help  Print help
```

**Examples:**

```sh
komodefi-cli wallet balance DOC  
Getting balance, coin: DOC  
coin: DOC  
balance: 949832.3746621  
unspendable: 0  
address: RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
```

### wallet withdraw

The `withdraw` command return a signed transaction binary data that is supposed to be sent with `send-raw-transaction`. The withdrawal is done using the [`withdraw` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/withdraw.html). `--amount` and  `--max` options can not be used together. `--fee` options provides setting up custom fee for utxo,  eth, qrc and cosmos. `--derivation-path` (*currently not supported*) and (`--hd-account-id`, `--hd-is-change`, `--hd-address-index`) are parameters that allow withdrawing from the certain [bip32 compatible derived address ](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki) if `enable_hd` option is enabled as a part of current `mm2` configuration. `--bare-output` option prevents an extra output to be able to call methods in sequentially.

```sh
release/komodefi-cli wallet withdraw --help  
Generates, signs, and returns a transaction that transfers the amount of coin to the address indicated in the to argument  
  
Usage: komodefi-cli wallet withdraw [OPTIONS] <COIN> <TO>  
  
Arguments:  
 <COIN>  Coin the user desires to withdraw  
 <TO>    Address the user desires to withdraw to  
  
Options:  
 -M, --max  
         Withdraw the maximum available amount  
 -a, --amount <AMOUNT>  
         Amount the user desires to withdraw  
 -f, --fee <FEE>  
         Transaction fee [possible-values: <utxo-fixed:amount|utxo-per-kbyte:amount|eth:gas_price:gas|qrc:gas_limit:gas_price|cosmos:gas_limit:gas_price>]  
     --derivation-path <DERIVATION_PATH>  
         Derivation path to determine the source of the derived value in more detail  
     --hd-account-id <HD_ACCOUNT_ID>  
         Account index of the same crypto currency  
     --hd-is-change <HD_IS_CHANGE>  
         Is change [possible values: true, false]  
     --hd-address-index <HD_ADDRESS_INDEX>  
         An incremental address index for the account  
 -b, --bare-output  
         Whether to output only tx_hex [aliases: bare]  
 -h, --help  
         Print help
```

**Example**:

```sh
komodefi-cli wallet withdraw DOC RQvvz29iHpn4iuHeN7qFXnLbc1eh31nRKY --amount 1  
Getting withdraw tx_hex  
coin: DOC  
from: RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM  
to: RQvvz29iHpn4iuHeN7qFXnLbc1eh31nRKY  
total_amount: 949831.3746521  
spent_by_me: 949831.3746521  
received_by_me: 949830.3746421  
my_balance_change: -1.00001  
block_height: 0  
timestamp: 23-09-13 14:32:29  
fee_details: {"type":"Utxo","coin":"DOC","amount":"0.00001"}  
internal_id:    
transaction_type: "StandardTransfer"  
tx_hash: 04bfc0470a07ab1390dffb98164543e368d100101020a4f9b20bf3f8193c2ea0  
tx_hex: 0400008085202f8901d4a7ffce25cdc0ca0cbfa23363e1159195a3fafd742bf8bfc6ba0f50a51b7160010000006a4730440220590868b953be9c655ebd97cf750fbd669171006848900c5ea9d0e151a1dfb28b02203b6ca6955d2f558c4b5fdbf5cc1a149cbe6a57685be34d9f7594b140ad  
698765012102264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff0200e1f505000000001976a914abad17c49173537a07e39d2c94dcfe64645b1ad488ac922e35f6625600001976a9149934ebeaa56cb597c936a9ed8202d8d97a0a700388acfdc70165000000  
000000000000000000000000
```

Bare output:

```sh
RUST_LOG=error komodefi-cli wallet withdraw --amount 1 --fee utxo-fixed:0.01  DOC RQvvz29iHpn4iuHeN7qFXnLbc1eh31nRKY -b  
0400008085202f8901d4a7ffce25cdc0ca0cbfa23363e1159195a3fafd742bf8bfc6ba0f50a51b7160010000006b483045022100a9bb524d413fea1a417a5d9ac1465c8995eadd642704c003ab5a2be22b533546022044d3f33582773254930f5054afa89d9ce085369a1b3c2c47302ab5dbf17cfa13  
012102264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff0200e1f505000000001976a914abad17c49173537a07e39d2c94dcfe64645b1ad488ac3af025f6625600001976a9149934ebeaa56cb597c936a9ed8202d8d97a0a700388ac7c850165000000000000  
000000000000000000
```

### wallet send-raw-transaction (send-raw, send)

The `send-raw-transaction` command sends a raw transaction to a given coin network using the [`send_raw_transaction` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/send_raw_transaction.html) The `--coin` and `--tx-hex` options specify the coin and transaction data, accordingly. The optional `--bare-output` (`bare`) parameter is intended to suppress additional output. *Aliases: `send-raw`, `send`*

```sh
komodefi-cli wallet send -h  
Broadcasts the transaction to the network of selected coin  
  
Usage: komodefi-cli wallet send-raw-transaction [OPTIONS] --coin <COIN> --tx-hex <TX_HEX>  
  
Options:  
 -c, --coin <COIN>      Name of the coin network on which to broadcast the transaction  
 -t, --tx-hex <TX_HEX>  Transaction bytes in hexadecimal format;  
 -b, --bare-output      Whether to output only tx_hash [aliases: bare]  
 -h, --help             Print help
 ```

**Example:**

```sh
komodefi-cli wallet send --coin DOC --tx-hex 0400008085202f8901d4a7ffce25cdc0ca0cbfa23363e1159195a3fafd742bf8bfc6ba0f50a51b7160010000006a47304402200ddbcda20d288d1c6075d01bb55158e18353f0e5  
e9789cddfb572ad0429e96f7022040d2c66dbab960c5746ceb16aec7d6f0f85364d939377cbfdc060ad8f24dd0be012102264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff0200e1f505000000001976a914abad17c49173537a07e39d2c94dcfe64645b1ad4  
88ac922e35f6625600001976a9149934ebeaa56cb597c936a9ed8202d8d97a0a700388ac3acb0165000000000000000000000000000000  
Sending raw transaction  
tx_hash: 58aca0e311513974e467187026b873621c68a4ddedb16175c558d3886dbc3dc4
```

Calling methods sequentially:

```sh
komodefi-cli wallet send -b --coin DOC --tx-hex  $(RUST_LOG=error komodefi-cli wallet withdraw DOC RQvvz29iHpn4iuHeN7qFXnLbc1eh31nRKY --amount 1 -b)
Sending raw transaction  
26f4f904c60d5d0a410fa6e00fb540e19e983f52045b2d29f97d046c58e9ecd0
```

This transaction could be found [in the DOC coin explorer](https://doc.explorer.dexstats.info/tx/26f4f904c60d5d0a410fa6e00fb540e19e983f52045b2d29f97d046c58e9ecd0)
### wallet get-raw-transaction (get-raw, raw-tx, get)

The `get-raw-transaction`  get raw transaction data from the given coin's network using the [`get_raw_transaction` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-20/get_raw_transaction.html). *Aliases: `get-raw`, `raw-tx`, `get`*

```sh
komodefi-cli wallet get -h  
Returns the full signed raw transaction hex for any transaction that is confirmed or within the mempool  
  
Usage: komodefi-cli wallet get-raw-transaction [OPTIONS] --coin <COIN> --tx-hash <TX_HASH>  
  
Options:  
 -c, --coin <COIN>        Coin the user desires to request for the transaction  
 -H, --tx-hash <TX_HASH>  Hash of the transaction [aliases: hash]  
 -b, --bare-output        Whether to output only tx_hex [aliases: bare]  
 -h, --help               Print help
```

**Example:**

```sh
komodefi-cli wallet get-raw-transaction -c DOC -H 26f4f904c60d5d0a410fa6e00fb540e19e983f52045b2d29f97d046c58e9ecd0 -b  
Getting raw transaction of coin: DOC, hash: 26f4f904c60d5d0a410fa6e00fb540e19e983f52045b2d29f97d046c58e9ecd0  
0400008085202f8901b3d8bfad51d9d44ba1d721180596cfe7e2f38969a582a3d358d8bc6cb80cb918010000006a47304402201fc52ae1078a0fd12b79b76170165919a8c36c6aae978e2ed8f204998f46095c02205e5b6dfe9c118b3acef2c28a7454e9d153fbdd89d0f6abc24fc2e2b966486fff01  
2102264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff0200e1f505000000001976a914abad17c49173537a07e39d2c94dcfe64645b1ad488acc26449ea625600001976a9149934ebeaa56cb597c936a9ed8202d8d97a0a700388ac01d1016500000000000000  
0000000000000000
```

### wallet tx-history (history)

If the given coin was enabled with `--tx-history` it's getting possible to request history of transactions using the [`my_tx_history` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/my_tx_history.html).  Requesting transaction set could be limited with `--limit` option or  not if `--max` is taking place. The resulting transactions are organized into pages, there is the `--page-number` (`--page`) options that allows to select the certain one. If one of `--from-tx-hash` or `--from-tx-id` is taking place the resulting set will be restricted with the given hash or id accordingly.
*Alias: history*

```sh
komodefi-cli wallet tx-history --help  
Returns the blockchain transactions involving the Komodo DeFi Framework node's coin address  
  
Usage: komodefi-cli wallet tx-history [OPTIONS] <--limit <LIMIT>|--max> <COIN>  
  
Arguments:  
 <COIN>  The name of the coin for the history request  
  
Options:  
 -l, --limit <LIMIT>                Limits the number of returned transactions  
 -m, --max                          Whether to return all available records  
 -H, --from-tx-hash <FROM_TX_HASH>  Skips records until it reaches this ID, skipping the from_id as well  
 -i, --from-tx-id <FROM_TX_ID>      For zcoin compatibility, skips records until it reaches this ID, skipping the from_id as well  
 -p, --page-number <PAGE_NUMBER>    The name of the coin for the history request  
 -h, --help                         Print help
```

**Example:**

```sh
komodefi-cli wallet tx-history DOC --limit 5 --page 3  
Getting tx history, coin: DOC  
limit: 5  
skipped: 10  
total: 19  
page_number: 3  
total_pages: 4  
current_block: 205601  
sync_status: Finished  
transactions:    
│ time: 23-07-24 16:54:29                                                                  │ │ coin: DOC                                                                                │ 
│ block: 132256                                                                            │ 
│ confirmations: 73346                                                                     │ 
│ transaction_type: StandardTransfer                                                       │ │ total_amount: 949836.47                                                                  │ 
│ spent_by_me: 949836.47                                                                   │ 
│ received_by_me: 949835.47                                                                │ 
│ my_balance_change: -1.00                                                                 │ 
│ fee_details: {"type":"Utxo","coin":"DOC","amount":"0.00001"}                             │ 
│ tx_hash: 671aaee83b6d870f168c4e0be93e2d2087c8eae324e105c1dcad240cfea73c03                │ 
│ from: RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM                                                 │ 
│ to: RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM, RQvvz29iHpn4iuHeN7qFXnLbc1eh31nRKY               │ 
│ internal_id: 671aaee83b6d870f168c4e0be93e2d2087c8eae324e105c1dcad240cfea73c03            │ 
│ tx_hex: 0400008085202f8901afdad3b683400c1ff4331514afdcbd1703647edcd1abaa3d93b1aecd6daaa3 │ 
│ e0010000006a473044022022d692771b413f4b197c2dba61453b97a4df994986fafbd8e8950fcc77e1519d02 │ 
│ 2012c990b48e11ee568907184236e1d5e2f03a5a18132fb322fe7522e33ddc6f39012102264fcd9401d797c5 │ 
│ 0fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff0200e1f505000000001976a914abad17 │ 
│ c49173537a07e39d2c94dcfe64645b1ad488ac54729d14635600001976a9149934ebeaa56cb597c936a9ed82 │ 
│ 02d8d97a0a700388acb0acbe64000000000000000000000000000000                                 │ 
│                                                                                          │ 
├──────────────────────────────────────────────────────────────────────────────────────────┤ 
│ time: 23-07-24 16:44:09                                                                  │
...
```

### wallet show-private-key (private)

The `show-private-key` command allows to get private key for the given coin that could be used to access account in a third-party wallet application. This command is proceed using the [`show_priv_key` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/show_priv_key.html).  *Aliases: `private`, `private-key`*

```sh
komodefi-cli wallet private --help  
Returns the private key of the specified coin in a format compatible with coin wallets  
  
Usage: komodefi-cli wallet show-priv-key <COIN>  
  
Arguments:  
 <COIN>  The name of the coin of the private key to show  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli wallet private-key DOC  
Getting private key, coin: DOC  
coin: DOC  
priv_key: UufMCuXDsLNWga8kz7ZyJKXzH86MZ7vbEzUa21A7EEYqqbtHwYQ5
```

### wallet validate-address (validate)

The `validate-address` validated wether the given coin address could exist and valid using the [`validateaddress` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/validateaddress.html). *Alias: `validate`*

```sh
release/komodefi-cli wallet validate-address --help  
Checks if an input string is a valid address of the specified coin  
  
Usage: komodefi-cli wallet validate-address <COIN> <ADDRESS>  
  
Arguments:  
 <COIN>     The coin to validate address for  
 <ADDRESS>  The input string to validate  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli wallet validate-address ZOMBIE zs1lgyuf2tudj7vgtx2pgtsqqdlcrva7a5rw4n4f7rswc6vnk8thc85dl4uvazpkv6y8yd25cjp72g  
Validating address, coin: ZOMBIE, address: zs1lgyuf2tudj7vgtx2pgtsqqdlcrva7a5rw4n4f7rswc6vnk8thc85dl4uvazpkv6y8yd25cjp72g  
valid: valid
```

### wallet kmd-rewards-info (rewards)

The `kmd-rewards-info` returns information about the active user rewards that can be claimed by an address's unspent outputs using the [`kmd_rewards_info` RPC API methods](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/kmd_rewards_info.html). *Alias: `rewards`*

```sh
komodefi-cli wallet rewards --help  
Informs about the active user rewards that can be claimed by an address's unspent outputs  
  
Usage: komodefi-cli wallet kmd-rewards-info  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli wallet rewards    
Getting kmd rewards info  
rewards_info:    
tx_hash: 498e23e1e0a3322192c4c4d7f531a562c12f44f86544013344397c3fc217963a  
height: 3515481  
output_index: 1  
amount: 1599.00  
locktime: 23-07-23 14:30:46  
accrued_rewards: 23-07-23 14:30:46  
accrued_rewards: 0.01356243  
accrue_start_at: 23-07-23 15:30:46  
accrue_stop_at: 23-08-23 14:30:46
```

### wallet convert-address (convert)

The `convert-address` command converts an input address to a specified address format using the [`convert_address` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/convertaddress.html). For example, this method can be used to convert a BCH address from legacy to cash address format and vice versa. Or this can be used to convert an ETH address from single to mixed case checksum format. *Alias: `convert`*

```sh
komodefi-cli wallet convert --help  
Converts an input address to a specified address format  
  
Usage: komodefi-cli wallet convert-address [OPTIONS] --coin <COIN> --from <FROM> --format <FORMAT>  
  
Options:  
 -c, --coin <COIN>  
         The name of the coin address context  
 -f, --from <FROM>  
         Input address  
 -F, --format <FORMAT>  
         Address format to which the input address should be converted [possible values: mixed-case, cash-address, standard]  
 -C, --cash-address-network <CASH_ADDRESS_NETWORK>  
         Network prefix for cashaddress format [possible values: bitcoin-cash, bch-test, bch-reg]  
 -h, --help  
         Print help
```

**Examples:**

Mixed case

```sh
komodefi-cli wallet convert --coin ETH --from 0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359 --format mixed-case  
Converting address  
address: 0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359
```

Standard

```sh
komodefi-cli wallet convert --coin BCH --from bitcoincash:qzxqqt9lh4feptf0mplnk58gnajfepzwcq9f2rxk55 --format standard  
Converting address  
address: 1DmFp16U73RrVZtYUbo2Ectt8mAnYScpqM
```

Cash address

```sh
komodefi-cli wallet convert --coin BCH --from 1DmFp16U73RrVZtYUbo2Ectt8mAnYScpqM --format cash-address -C bch-test  
Converting address  
address: bchtest:qzxqqt9lh4feptf0mplnk58gnajfepzwcqpmwyypng
```

### wallet convert-utxo-address (convert-utxo)

The `convert-utxo` command is similar to convert-address but is aimed to convert UTXO addresses to each other

```sh
komodefi-cli wallet convert-utxo-address <ADDRESS> <TO_COIN>  
  
Arguments:  
 <ADDRESS>  Input UTXO address  
 <TO_COIN>  Input address to convert from  
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli wallet convert-utxo RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM KMD  
Convert utxo address  
address: RPFGrvJWjSYN4qYvcXsECW1HoHbvQjowZM
```

## DEX funtionality

The next group of commands provides trading functionality, orders managing and trading commands are meant. General commands such as `sell`, `buy`, `set-price`, `update-maker-order` are left at the top level to make it easier to call them.


```sh
komodefi-cli orders --help  
Order listing commands: book, history, depth etc.  
  
Usage: komodefi-cli order <COMMAND>  
  
Commands:  
 orderbook        Get orderbook [aliases: book]  
 orderbook-depth  Get orderbook depth [aliases: depth]  
 order-status     Return the data of the order with the selected uuid created by the current node [aliases: status]  
 best-orders      Return the best priced trades available on the orderbook [aliases: best]  
 my-orders        Get my orders [aliases: my, mine]  
 orders-history   Return all orders whether active or inactive that match the selected filters [aliases: history, filter]  
 help             Print this message or the help of the given subcommand(s)  
  
Options:  
 -h, --help  Print help
```
### orders orderbook (book)

The `orderbook` command gets available bids and asks for the given base and rel coins using the [`orderbook` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/orderbook.html). You can specify which order should be shown and limit the output to the given ask and bids limits. *Alias: `book`*

```sh
komodefi-cli orders orderbook DOC MARTY --help  
Get orderbook  
  
Usage: komodefi-cli order orderbook [OPTIONS] <BASE> <REL>  
  
Arguments:  
 <BASE>  Base currency of a pair  
 <REL>   Related currency, also can be called "quote currency" according to exchange terms  
  
Options:  
 -u, --uuids                    Enable `uuid` column  
 -m, --min-volume               Enable `min_volume` column [aliases: min]  
 -M, --max-volume               Enable `max_volume` column [aliases: max]  
 -p, --publics                  Enable `public` column  
 -a, --address                  Enable `address` column  
 -A, --age                      Enable `age` column  
 -c, --conf-settings            Enable order confirmation settings column  
     --asks-limit <ASKS_LIMIT>  Orderbook asks count limitation [default: 20]  
     --bids-limit <BIDS_LIMIT>  Orderbook bids count limitation [default: 20]  
 -h, --help                     Print help
```


**Example:**

```sh
komodefi-cli orders book DOC MARTY --uuids --asks-limit 5 --bids-limit 5  
Getting orderbook, base: DOC, rel: MARTY  
     Volume: DOC Price: MARTY     Uuid                                    
*           1.00 3.00000000       4d7c187b-d61f-4349-b608-a6abe0b3f0ea    
            0.14 1.00000000       0e549623-fead-4645-9c6c-00877b50bac2    
     94974264.87 1.00000000       9406010e-54fd-404c-b252-1af473bf77e6    
- --------------- ---------------- ------------------------------------    
     94898462.11 1.00000000       5a14a592-feea-4eca-92d8-af79c0670a39    
            1.94 1.00000000       fbd51c38-f3a7-42c5-aa3a-c52938188086
```

### sell

The `sell` command sells a base coin for a rel one in a given volume and at a given price in rel using the [`sell` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/sell.html). That is possible to limit  a new order with the `--min-volume`.  If `--uuid` or `--public` options were set matching procedure would be restricted according to the given values. `--base-confs`, `--base-nota`, `--rel-confs`, `--rel-nota` are to override preset confirmation and notarization rules.

```sh
komodefi-cli sell  --help  
Put a selling request  
  
Usage: komodefi-cli sell [OPTIONS] <BASE> <REL> <VOLUME> <PRICE>  
  
Arguments:  
 <BASE>    Base currency of a pair  
 <REL>     Related currency of a pair  
 <VOLUME>  Amount of coins the user is willing to sell/buy of the base coin  
 <PRICE>   Price in rel the user is willing to receive/pay per one unit of the base coin  
  
Options:  
 -t, --order-type <ORDER_TYPE>  The GoodTillCancelled order is automatically converted to a maker order if not matched in 30 seconds, and this maker order stays in the orderbook until explicitly cancelled. On the other hand, a FillOrKi  
ll is cancelled if not matched within 30 seconds [default: good-till-cancelled] [aliases: type] [possible values: fill-or-kill, good-till-cancelled]  
 -m, --min-volume <MIN_VOLUME>  Amount of base coin that will be used as min_volume of GoodTillCancelled order after conversion to maker  
 -u, --uuid <MATCH_UUIDS>       The created order is matched using a set of uuid  
 -p, --public <MATCH_PUBLICS>   The created order is matched using a set of publics to select specific nodes  
     --base-confs <BASE_CONFS>  Number of required blockchain confirmations for base coin atomic swap transaction [aliases: bc]  
     --base-nota <BASE_NOTA>    Whether dPoW notarization is required for base coin atomic swap transaction [aliases: bn] [possible values: true, false]  
     --rel-confs <REL_CONFS>    Number of required blockchain confirmations for rel coin atomic swap transaction [aliases: rc]  
     --rel-nota <REL_NOTA>      Whether dPoW notarization is required for rel coin atomic swap transaction [aliases: rn] [possible values: true, false]  
 -s, --save-in-history          If true, each order's short record history is stored else the only order status will be temporarily stored while in progress [aliases: save]  
 -h, --help                     Print help
```

**Example**:

```sh
komodefi-cli sell DOC MARTY 1 3  
Selling: 1 DOC for: 3 MARTY at the price of 3 MARTY per DOC  
4d7c187b-d61f-4349-b608-a6abe0b3f0ea
```
### set-price

The `set-price` is almost the same as `sell` command, it's always considered as `sell`. When the `set-price` is called the maker order is created immidiatelly. It provides the `--cancel-prev` option alowing to cancel existing orders for the given pair. This command is implemented by requesting the [`setprice` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/setprice.html).

```sh
komodefi-cli set-price --help  
Place an order on the orderbook. The setprice order is always considered a sell  
  
Usage: komodefi-cli set-price [OPTIONS] <--max|--volume <VOLUME>> <BASE> <REL> <PRICE>  
  
Arguments:  
 <BASE>   The name of the coin the user desires to sell  
 <REL>    The name of the coin the user desires to receive  
 <PRICE>  The price in rel the user is willing to receive per one unit of the base coin  
  
Options:  
 -M, --max                      Use the entire coin balance for the order, taking 0.001 coins into reserve to account for fees  
 -v, --volume <VOLUME>          The maximum amount of base coin available for the order, ignored if max is true; the following values must be greater than or equal to the min_trading_vol of the corresponding coin  
 -m, --min-volume <MIN_VOLUME>  The minimum amount of base coin available for the order; it must be less or equal than volume param; the following values must be greater than or equal to the min_trading_vol of the corresponding coin  
 -c, --cancel-prev              Cancel all existing orders for the selected pair [aliases: cancel]  
     --base-confs <BASE_CONFS>  Number of required blockchain confirmations for base coin atomic swap transaction [aliases: bc]  
     --base-nota <BASE_NOTA>    Whether dPoW notarization is required for base coin atomic swap transaction [aliases: bn] [possible values: true, false]  
     --rel-confs <REL_CONFS>    Number of required blockchain confirmations for rel coin atomic swap transaction [aliases: rc]  
     --rel-nota <REL_NOTA>      Whether dPoW notarization is required for rel coin atomic swap transaction [aliases: rn] [possible values: true, false]  
 -s, --save-in-history          If true, each order's short record history is stored in a local SQLite database table, and when the order is cancelled or fully matched, it's history will be saved as a json file [aliases: save]  
 -h, --help                     Print help
```

**Example:**

```sh
komodefi-cli set-price DOC MARTY 3 --volume 1 --cancel-prev    
Setting price for pair: DOC MARTY  
Maker order:    
               base: DOC  
                rel: MARTY  
              price: 3.00  
               uuid: f398ffe2-9c74-4340-8702-68eca1d167e8  
         created at: 23-09-14 13:59:27  
         updated at: 23-09-14 13:59:27  
       max_base_vol: 1.00  
       min_base_vol: 0.000100  
              swaps: empty  
      conf_settings: 1,false:1,false

komodefi-cli orders book DOC MARTY --uuids
Getting orderbook, base: DOC, rel: MARTY  
     Volume: DOC Price: MARTY     Uuid                                    
*           1.00 3.00000000       f398ffe2-9c74-4340-8702-68eca1d167e8    
...
```

### update-maker-order (update)

The `update-maker-order` updates existing order selected by its uuid with the given values using the [`update_maker_order` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/update_maker_order.html).  The order should be changed according to the given settings like it was done in the former commands. Volume is changed by the delta that is set with `--volume-delta` or is set as max. *Alias: `update`*

```sh
komodefi-cli update-maker-order --uuid f398ffe2-9c74-4340-8702-68eca1d167e8 --help  
Update order on the orderbook  
  
Usage: komodefi-cli update-maker-order [OPTIONS] --uuid <UUID>  
  
Options:  
 -u, --uuid <UUID>                  Uuid of the order the user desires to update  
 -p, --price <PRICE>                Price in rel the user is willing to receive per one unit of the base coin  
 -M, --max-volume                   Whether to use the entire coin balance for the order, taking 0.001 coins into reserve to account for fees  
 -d, --volume-delta <VOLUME_DELTA>  Volume added to or subtracted from the max_base_vol of the order to be updated, resulting in the new volume which is the maximum amount of base coin available for the order, ignored if max is true  
 -m, --min-volume <MIN_VOLUME>      Minimum amount of base coin available for the order; it must be less or equal than the new volume; the following values must be greater than or equal to the min_trading_vol of the corresponding coin  
     --base-confs <BASE_CONFS>      Number of required blockchain confirmations for base coin atomic swap transaction [aliases: bc]  
     --base-nota <BASE_NOTA>        Whether dPoW notarization is required for base coin atomic swap transaction [aliases: bn] [possible values: true, false]  
     --rel-confs <REL_CONFS>        Number of required blockchain confirmations for rel coin atomic swap transaction [aliases: rc]  
     --rel-nota <REL_NOTA>          Whether dPoW notarization is required for rel coin atomic swap transaction [aliases: rn] [possible values: true, false]  
 -h, --help                         Print help
```

**Example:**

```sh
komodefi-cli update-maker-order --uuid f398ffe2-9c74-4340-8702-68eca1d167e8 -p 10 -d=-2 --base-nota true --bc 5 --rn true --rc 6 -m 4  
Updating maker order  
Maker order:    
               base: DOC  
                rel: MARTY  
              price: 10.00  
               uuid: f398ffe2-9c74-4340-8702-68eca1d167e8  
         created at: 23-09-14 13:59:27  
         updated at: 23-09-14 14:33:17  
       max_base_vol: 9.00  
       min_base_vol: 4.00  
              swaps: empty  
      conf_settings: 5,true:6,true

komodo-atomicDEX-API]$ komodefi-cli orders book DOC MARTY --uuids -c     
Getting orderbook, base: DOC, rel: MARTY  
     Volume: DOC Price: MARTY     Uuid                                 Order conf 
*           9.00 10.00000000      f398ffe2-9c74-4340-8702-68eca1d167e8 1,false:1,false             0.14 1.00000000       0e549623-fead-4645-9c6c-00877b50bac2 1,false:1,false      94974264.87 1.00000000       26bdefff-5cfd-440c-83a8-04f3b82aa53b 1,false:1,false 
- -------------- ---------------- ------------------------------------ ----------------
     94898462.11 1.00000000       754f76e6-b639-4ce8-b08f-37fbbdc4faa6 1,false:1,false
...
```

*Notice: you can see the responded `conf-settings` for the given order f398ffe2-9c74-4340-8702-68eca1d167e8 are wrong*
### buy

The `buy` command buys a base coin for a rel one in a given volume and at a given price using the [`buy` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/buy.html). Like for the `sell` command that is possible to limit  a new order with the `--min-volume`.  If `--uuid` or `--public` options were set matching procedure would be restricted according to the given values. `--base-confs`, `--base-nota`, `--rel-confs`, `--rel-nota` are to override preset confirmation and notarization rules.

```sh
komodefi-cli buy --help  
Put a buying request  
  
Usage: komodefi-cli buy [OPTIONS] <BASE> <REL> <VOLUME> <PRICE>  
  
Arguments:  
 <BASE>    Base currency of a pair  
 <REL>     Related currency of a pair  
 <VOLUME>  Amount of coins the user is willing to sell/buy of the base coin  
 <PRICE>   Price in rel the user is willing to receive/pay per one unit of the base coin  
  
Options:  
 -t, --order-type <ORDER_TYPE>  The GoodTillCancelled order is automatically converted to a maker order if not matched in 30 seconds, and this maker order stays in the orderbook until explicitly cancelled. On the other hand, a FillOrKi  
ll is cancelled if not matched within 30 seconds [default: good-till-cancelled] [aliases: type] [possible values: fill-or-kill, good-till-cancelled]  
 -m, --min-volume <MIN_VOLUME>  Amount of base coin that will be used as min_volume of GoodTillCancelled order after conversion to maker  
 -u, --uuid <MATCH_UUIDS>       The created order is matched using a set of uuid  
 -p, --public <MATCH_PUBLICS>   The created order is matched using a set of publics to select specific nodes  
     --base-confs <BASE_CONFS>  Number of required blockchain confirmations for base coin atomic swap transaction [aliases: bc]  
     --base-nota <BASE_NOTA>    Whether dPoW notarization is required for base coin atomic swap transaction [aliases: bn] [possible values: true, false]  
     --rel-confs <REL_CONFS>    Number of required blockchain confirmations for rel coin atomic swap transaction [aliases: rc]  
     --rel-nota <REL_NOTA>      Whether dPoW notarization is required for rel coin atomic swap transaction [aliases: rn] [possible values: true, false]  
 -s, --save-in-history          If true, each order's short record history is stored else the only order status will be temporarily stored while in progress [aliases: save]  
 -h, --help                     Print help
```

**Example**

```sh
komodefi-cli buy DOC MARTY 1 0.1    
Buying: 1 DOC with: 0.1 MARTY at the price of 0.1 MARTY per DOC
2e791413-65ba-4a2a-a010-e1ef521f1f73

komodefi-cli orders book DOC MARTY --uuids --asks-limit 5 --bids-limit 5  
Getting orderbook, base: DOC, rel: MARTY  
     Volume: DOC Price: MARTY     Uuid                                    
...
- --------------- ---------------- ------------------------------------    
...
*           1.00 0.10000000       2e791413-65ba-4a2a-a010-e1ef521f1f73
```

### order order-status (status)

The `order-status` command gets the status and details of an order by the given uuid using the [`order_status` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/order_status.html). *Alias: `status`*

```sh
komodefi-cli order status --help  
Return the data of the order with the selected uuid created by the current node  
  
Usage: komodefi-cli order order-status <UUID>  
  
Arguments:  
 <UUID>     
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli order status f398ffe2-9c74-4340-8702-68eca1d167e8  
Getting order status: f398ffe2-9c74-4340-8702-68eca1d167e8  
               base: DOC  
                rel: MARTY  
              price: 10.00  
               uuid: f398ffe2-9c74-4340-8702-68eca1d167e8  
         created at: 23-09-14 13:59:27  
         updated at: 23-09-14 14:33:17  
       max_base_vol: 9.00  
       min_base_vol: 4.00  
              swaps: empty  
      conf_settings: 5,true:6,true  
        cancellable: true  
   available_amount: 9.00
```

### orders orderbook-depth (depth)

The `orderbook-depth` command gets a common orderbooks' view on the given set of pairs using the [`orderbook_depth` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/orderbook_depth.html). Each resulting value is a count of bids and asks in the related orderbook. *Alias: `depth`*

```sh
komodefi-cli orders depth --help  
Get orderbook depth  
  
Usage: komodefi-cli order orderbook-depth <BASE/REL>...  
  
Arguments:  
 <BASE/REL>...     
  
Options:  
 -h, --help  Print help
```

**Example:**

```sh
komodefi-cli orders depth DOC/MARTY BTC/KMD ETH/BTC  
Getting orderbook depth for pairs: DOC/MARTY, BTC/KMD, ETH/BTC  
           Bids Asks    
DOC/MARTY: 3    3       
  BTC/KMD: 8    5       
  ETH/BTC: 2    0
```
### orders best-orders (best)

The `best-orders` command scans all order books related to a given coin and lists the more profitable orders available in them that can meet a given `--volume` or limits them with a given `--number`.  This command is implemented by requesting the [`best_orders` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/best_orders.html).

```sh
komodefi-cli orders best-orders --help  
Return the best priced trades available on the orderbook  
  
Usage: komodefi-cli order best-orders [OPTIONS] <--volume <VOLUME>|--number <NUMBER>> <COIN> <ACTION>  
  
Arguments:  
 <COIN>    The coin to get best orders  
 <ACTION>  Whether to buy or sell the selected coin [possible values: buy, sell]  
  
Options:  
 -v, --volume <VOLUME>    The returned results will show the best prices for trades that can fill the requested volume  
 -n, --number <NUMBER>    The returned results will show a list of the best prices  
 -o, --show-orig-tickets  Whether to show the original tickers if they are configured for the queried coin [aliases: show-origin, original-tickers, origin]  
 -e, --exclude-mine       Exclude orders that is mine  
 -h, --help               Print help
```

**Example:**

```sh
Getting best orders: Sell BTC  
│      │ Price    │ Uuid    │ Base vol(min:max) │ Rel vol          │ Address │ Confirmation
| ARRR 
│      │ 144740.84│ e51e8-..│ 0.0081:0.14       │ 1180.58:20790.82 │ Shielded│ 1,false...  
│      │ 128743.20│ f1f7bb..│ 0.0080:0.0085     │ 1030.74:1098.19  │ Shielded│ 1,false...  
│ BAND-BEP20 
│      │ 25975.84 │ fb4e26..│ 0.0081:0.01       │ 211.60:493.22    │ 0xadb6..│ 1,false...  
│ BCH 
...
```

### orders my-orders (my, mine)

The `my-orders` command requests information about orders created by the current node (running `mm2` instance) using the [`my_orders` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/my_orders.html). *Aliases: `my`, `mine`.

**Example:**

```sh
komodefi-cli orders mine  
Getting my orders  
       Taker orders:
│ action                │ uuid, sender, dest │ type,created_at   │ match_by │ base,rel     │ | base(vol),rel(vol)    │                    │ confirmation      │          │ ticker       │ 
│ Sell                  │ 797c0456-7d99-.... │ GoodTillCancelled │ Any      │ none         │ │ MARTY(1.00),DOC(3.00) │ 264fcd9401d797c5...│ 23-09-14 16:11:44 │          │ none         │ 

       Maker orders:    
│ base,rel   │ price │ uuid   │ created at,        │ min base vol, │ cancellable │ ...
│            │       │        │ updated at         │ max base vol  │             │ 
│ RICK,MORTY │ 2.00  │ 1fc0d..│ 23-05-08 12:07:18, │ 0.000100,     │ true        │   
│            │       │        │ 23-05-08 12:07:18  │ 0.50          │             │
│ DOC,MARTY  │ 10.00 │ f398f..│ 23-09-14 13:59:27, │ 4.00,         │ true        │ 
│            │       │        | 23-09-14 14:33:17  │ 9.00          │             │ ...
```

### orders orders-history (history)

The `orders-history` command requests the hitstory of orders created by the current node (running `mm2` instance) using the [` orders_history_by_filter` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/orders_history_by_filter.html). Resulting orders are queried by the given filter.

```sh
komodefi-cli orders history --help  
Return all orders whether active or inactive that match the selected filters  
  
Usage: komodefi-cli order orders-history [OPTIONS] <--takers|--makers|--warnings|--all>  
  
Options:  
 -t, --takers                     Whether to show taker orders detailed history  
 -m, --makers                     Whether to show maker orders detailed history  
 -w, --warnings                   Whether to show warnings  
 -a, --all                        Whether to show common history data  
     --type <ORDER_TYPE>          Return only orders that match the type [possible values: taker, maker]  
     --action <INITIAL_ACTION>    Return only orders that match the initial action. Note that maker order initial_action is considered "Sell" [possible values: sell, buy]  
     --base <BASE>                Return only orders that match the order.base  
     --rel <REL>                  Return only orders that match the order.rel  
     --from-price <FROM_PRICE>    Return only orders whose price is more or equal the from_price  
     --to-price <TO_PRICE>        Return only orders whose price is less or equal the to_price  
     --from-volume <FROM_VOLUME>  Return only orders whose volume is more or equal the from_volume  
     --to-volume <TO_VOLUME>      Return only orders whose volume is less or equal the to_volume  
     --from-dt <FROM_DT>          Return only orders that match the order.created_at >= from_dt. Datetime fmt: "%y-%m-%dT%H:%M:%S"  
     --to-dt <TO_DT>              Return only orders that match the order.created_at <= to_dt. Datetime fmt: "%y-%m-%dT%H:%M:%S"  
     --was-taker                  Return only GoodTillCancelled orders that got converted from taker to maker  
     --status <STATUS>            Return only orders that match the status [possible values: created, updated, fulfilled, insuficcient-balance, cancelled, timed-out]  
 -h, --help                       Print help
```

**Example:**

Requesting all DOC based orders

```sh
komodefi-cli orders history --base DOC --all  
Getting order history  
Orders history:  
│uuid  │Type │Action│Base│Rel  │Volume│Price│Status   │Created │Updated │Was taker│  
│f398..│Maker│Sell  │DOC │MARTY│9.00  │10.00│Updated  │23-09...│23-09-..│false    │  
│96b1..│Maker│Sell  │DOC │MARTY│1.00  │3.00 │Cancelled│23-09-..│23-09-..│false    │
```

Requesting detailed makers

```sh
komodefi-cli orders history --base RICK --makers  
Getting order history  
Maker orders history detailed:

│ base,rel  │ price  │ uuid │ created at,│ min base vol, │ swaps │ conf_settings
│           │        │      │ updated at │ max base vol  │       │              ...
│ RICK,MORTY│ 0.95   │ f1b..│ 23-06-30 ..│ 0.00010,      │ fe60..│ 1,false:1,false 
│           │        │      │ 23-06-30 ..│ 1.00          │ e07b..│ 
│           │        │      │            │               │ dec7..│ 
│           │        │      │            │               │ c63b..│ 
│  matches
│                       uuid: e07bcf02-788f-407c-a182-63c506280ca4
│                   req.uuid: e07bcf02-788f-407c-a182-63c506280ca4
│             req.(base,rel): MORTY(0.0100), RICK(0.01)
│               req.match_by: Any 
│                 req.action: Sell 
│          req.conf_settings: 1,false:1,false 
│         req.(sender, dest): 144ee16a5960c50a930c26c0e01133de603eb41ce2e2e6...
│        reserved.(base,rel): RICK(0.01), MORTY(0.0100) 
│    reserved.(taker, maker): dec7fe39-16be-42cc-b3ba-2078ed5019b5,f1bf7c76...
│    reserved.(sender, dest): 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5...
│     reserved.conf_settings: 1,false:1,false 
│    connected.(taker,maker): dec7fe39-16be-42cc-b3ba-2078ed5019b5,f1bf7c76-806e-40cb-...
│   connected.(sender, dest): 264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5...
│      connect.(taker,maker): dec7fe39-16be-42cc-b3ba-2078ed5019b5,f1bf7c76-806e-40cb-...
│     connect.(sender, dest): 00000000000000000000000000000000000000000000000000000000...
│               last_updated: 23-06-30 09:54:13
...
```

## Swaps

Once an order is created and matched with another order, it is no longer part of the order book, the swap is created and execution begins

###  swaps active-swaps (active)

The `active-swaps` command lists swaps using the [`active_swaps` RPC API method](https://developers.komodoplatform.com/basic-docs/atomicdex-api-legacy/active_swaps.html)

```
komodefi-cli swaps active  --help  
Get all the swaps that are currently running  
  
Usage: komodefi-cli swaps {active-swaps|-a} [OPTIONS]  
  
Options:  
 -s, --include-status  Whether to include swap statuses in response; defaults to false  
 -u, --uuids-only      Whether to show only uuids of active swaps [aliases: uuids]  
 -h, --help            Print help
```

```sh
TakerSwap: 6b007706-d6e1-4565-8655-9eeb128d00e2  
my_order_uuid: 6b007706-d6e1-4565-8655-9eeb128d00e2  
gui: adex-cli  
mm_version: 1.0.6-beta_dabdaf33b  
taker_coin: DOC  
taker_amount: 1.00  
maker_coin: MARTY  
maker_amount: 1.00  
events: │ Started                           │ uuid: 6b007706-d6e1-4565-8655-9eeb128d00e2                                                                             │  
│ 23-07-25 12:20:07                 │ started_at: 70-01-20 13:31:27                                                                                          │  
│                                   │ taker_coin: DOC                                                                                                        │  
│                                   │ maker_coin: MARTY                                                                                                      │  
│                                   │ maker: 2d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846                                                │  
│                                   │ my_persistent_pub: 02264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d                                  │  
│                                   │ lock_duration: 7800                                                                                                    │  
│                                   │ maker_amount: 1.00                                                                                                     │  
│                                   │ taker_amount: 1.00                                                                                                     │  
│                                   │ maker_payment_confirmations: 1                                                                                         │  
│                                   │ maker_payment_requires_nota: false                                                                                     │  
│                                   │ taker_payment_confirmations: 1                                                                                         │  
│                                   │ taker_payment_requires_nota: false                                                                                     │  
│                                   │ tacker_payment_lock: 70-01-20 13:31:35                                                                                 │  
│                                   │ maker_payment_wait: 70-01-20 13:31:30                                                                                  │  
│                                   │ maker_coin_start_block: 147860                                                                                         │  
│                                   │ taker_coin_start_block: 133421                                                                                         │  
│                                   │ fee_to_send_taker_fee: coin: DOC, amount: 0.00001, paid_from_trading_vol: false                                        │  
│                                   │ taker_payment_trade_fee: coin: DOC, amount: 0.00001, paid_from_trading_vol: false                                      │  
│                                   │ maker_payment_spend_trade_fee: coin: MARTY, amount: 0.00001, paid_from_trading_vol: true                               │  
│                                   │ maker_coin_htlc_pubkey: 02264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d                             │  
│                                   │ taker_coin_htlc_pubkey: 02264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4d                             │  
│                                   │                                                                                                                        │  
│ Negotiated                        │ maker_payment_locktime: 70-01-20 13:31:43                                                                              │  
│ 23-07-25 12:20:23                 │ maker_pubkey: 000000000000000000000000000000000000000000000000000000000000000000                                       │  
│                                   │ secret_hash: a5cfc9787066562ba03c7538d024a88fd1a0fe12                                                                  │  
│                                   │ maker_coin_htlc_pubkey: 022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846                             │  
│                                   │ taker_coin_htlc_pubkey: 022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846                             │  
│                                   │                                                                                                                        │  
│ TakerFeeSent                      │ tx_hex: 0400008085202f890108742b73cfadf56dbc93d3fb8b33b54e5301869e2c950b200e67354b56e2d2ef010000006a473044022073605008 │  
│ 23-07-25 12:20:23                 │ 9328c8ec984036b4a248ba3a130d58e9601da3358ffef3482d40927002204a8aa7b83560ee22792457465432bf4f098b38f41d0a483f68920eb63b │  
│                                   │ 487d02012102264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff02bcf60100000000001976a914ca1e0474 │  
│                                   │ 5e8ca0c60d8c5881531d51bec470743f88ace0adaf08635600001976a9149934ebeaa56cb597c936a9ed8202d8d97a0a700388ac07bebf64000000 │  
│                                   │ 000000000000000000000000                                                                                               │  
│                                   │ tx_hash: c71f3793e976209674e2b00efb236c0fa8f0b1b552cb6cfe9068c6b731e570fd                                              │  
│                                   │                                                                                                                        │  
│ TakerPaymentInstructionsReceived  │ none                                                                                                                   │  
│ 23-07-25 12:20:24                 │                                                                                                                        │  
│                                   │                                                                                                                        │  
│ MakerPaymentReceived              │ tx_hex: 0400008085202f890754448b50295dedd36f8de60aeaeeb56a5efa9d1a4464185e329c3aae9fd17673020000006a4730440220651b3753 │  
│ 23-07-25 12:20:24                 │ 986a47026f36b082d870c3b2f7651684c0ed26637b64bfbc8722059302200ee7478e290327827daff8a2daf836e9446362169a4a8a4958f538c07f │  
│                                   │ 2093180121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846ffffffffe69a594c67460f781debbca0cfc731c29d │  
│                                   │ ddba613d65cf630acb00db6c93c9c0000000006b483045022100ef75d49925b7465bec5bc367f87fc7726d33aa17f472cd1ab6c13181d686139402 │  
│                                   │ 20447c529336a478f4b9d89cc1453ca1cc22f34c13c3b69f7440fcc7fe889493880121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09 │  
│                                   │ e359a3d4c850834846ffffffff990db7e9fa1f052aba359969b64b892cb76ff881ccd38cb75c09129e9065dbb3000000006a473044022000e7b9f1 │  
│                                   │ 3c99aa71ce1b8559c2a63cec9b808767a744196e9ed0bde0b5e481a40220053e683e1efc9191207f8feb5e42646301cd2b1f2a8f9e7616e29914eb │  
│                                   │ 9937c10121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846ffffffff41b284103365315e6fb79a7aa111295a03 │  
│                                   │ 9ccc96aa90d888915f9f0eabec549b000000006b483045022100b2b3cdc669c916e88615b5d2dc3c99186fc1369879bf08b681097986b6842b6302 │  
│                                   │ 2014a5c0b86d9732d4f202eb5a6d8590a37b6de013e17746c5a7be76b7c932f7390121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09 │  
│                                   │ e359a3d4c850834846ffffffffdd878ff57eb64187cb74390a4b959c4012d11b766c698f706634e3360e48a6f0000000006b483045022100a16683 │  
│                                   │ 4300118a432b2b4374e8d076be488b0f84109381f0862c050e73873885022050b3ca3475f1e266ab0b84f30f9030b2d4186a9939c305071691502c │  
│                                   │ de007d8c0121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846ffffffff509a8dc208434566c92ccfcc493df990 │  
│                                   │ da5edd7454c9527dd7df6d7c8aff49a8020000006b483045022100a02adf39ac6ad8e16603e0fc56948f7973223287f0fc0c9665ccd556b135193e │  
│                                   │ 022065ef4363429e83ae3703f6536a9f5294b178e7f1641bd24af7bbf3d72c0ada700121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a │  
│                                   │ 09e359a3d4c850834846ffffffffc3fbc1f34fd64e0dcb1db65a18789f2fbb170cc41a874f8788d904a24b3c2c5d020000006a47304402201e8190 │  
│                                   │ 95555707955dc508afc6db4acc7662c62460efa79a982f921bfd4afcb90220694cb9276b5228d544571cfdca849f6c18a6abf7169b983f9d47e88d │  
│                                   │ a43cd4b90121022d7424c741213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846ffffffff0300e1f5050000000017a914cea7345f │  
│                                   │ e6ada43ef95fd6cdfd8c339ef7d1c864870000000000000000166a14a5cfc9787066562ba03c7538d024a88fd1a0fe12d08c15d460ba11001976a9 │  
│                                   │ 14046922483fab8ca76b23e55e9d338605e2dbab6088ac07bebf64000000000000000000000000000000                                   │  
│                                   │ tx_hash: 3284af63a9fa0c4080fd5367c3c7c1ab1d00bb12ae47eb8fb0f5e2bd2da4736a                                              │  
│                                   │                                                                                                                        │  
│ MakerPaymentWaitConfirmStarted    │                                                                                                                        │  
│ 23-07-25 12:20:24                 │                                                                                                                        │  
│                                   │                                                                                                                        │  
│ MakerPaymentValidatedAndConfirmed │                                                                                                                        │  
│ 23-07-25 12:20:40                 │                                                                                                                        │  
│                                   │                                                                                                                        │  
│ TakerPaymentSent                  │ tx_hex: 0400008085202f8901fd70e531b7c66890fe6ccb52b5b1f0a80f6c23fb0eb0e274962076e993371fc7010000006a4730440220259ab8ec │  
│ 23-07-25 12:20:40                 │ 216b802f32092ef307017f183f4bd8a52bec420363abf7f070d444a8022061fce8a8b562e07b8ab41afd8953521ad7d22ffb0aa5c710f044554d89 │  
│                                   │ 833bb6012102264fcd9401d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dffffffff0300e1f5050000000017a914b491ff619f │  
│                                   │ 632ac1b7ef4e11f64404cff0e98adf870000000000000000166a14a5cfc9787066562ba03c7538d024a88fd1a0fe12f8c8b902635600001976a914 │  
│                                   │ 9934ebeaa56cb597c936a9ed8202d8d97a0a700388ac18bebf64000000000000000000000000000000                                     │  
│                                   │ tx_hash: 75cbad92b60fdb6be1fc7e73b6bac9b4b531c4f14d03b5201f8ff26f20ca1e5d                                              │  
│                                   │                                                                                                                        │  
│ TakerPaymentSpent                 │ tx_hex: 0400008085202f89015d1eca206ff28f1f20b5034df1c431b5b4c9bab6737efce16bdb0fb692adcb7500000000d8483045022100a0ec1d │  
│ 23-07-25 12:21:21                 │ 13d15a4f02a18a9adaa3442d8a9b956034c3e45b68bcbada8f877aef3b02206d59dcea375e86d5a014d51728c74a172c22a5b3cdc5dbe8daa70bb4 │  
│                                   │ b887a5a30120bed41dce1b0681670b3cad9d31c862bb166fcab656e23d4c00eef7dcac38cad4004c6b63046fdcbf64b1752102264fcd9401d797c5 │  
│                                   │ 0fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dac6782012088a914a5cfc9787066562ba03c7538d024a88fd1a0fe128821022d7424c7 │  
│                                   │ 41213a2b9b49aebdaa10e84419e642a8db0a09e359a3d4c850834846ac68ffffffff0118ddf505000000001976a914046922483fab8ca76b23e55e │  
│                                   │ 9d338605e2dbab6088ac6fdcbf64000000000000000000000000000000                                                             │  
│                                   │ tx_hash: 13de819b027b4ae98e730679b2b716f98bd1154f729303efd89615f152865586                                              │  
│                                   │ secret: bed41dce1b0681670b3cad9d31c862bb166fcab656e23d4c00eef7dcac38cad4                                               │  
│                                   │                                                                                                                        │  
│ MakerPaymentSpent                 │ tx_hex: 0400008085202f89016a73a42dbde2f5b08feb47ae12bb001dabc1c7c36753fd80400cfaa963af843200000000d74730440220641be55e │  
│ 23-07-25 12:21:21                 │ f769d759be59afe213d57eeeedf7d0f57bcf90835c8c3b7642d0e78902202a8f07ce745553107bea98a58cd50edb46782267fbeb4960c28073ad04 │  
│                                   │ 12cc380120bed41dce1b0681670b3cad9d31c862bb166fcab656e23d4c00eef7dcac38cad4004c6b6304e6fabf64b17521022d7424c741213a2b9b │  
│                                   │ 49aebdaa10e84419e642a8db0a09e359a3d4c850834846ac6782012088a914a5cfc9787066562ba03c7538d024a88fd1a0fe12882102264fcd9401 │  
│                                   │ d797c50fe2f1c7d5fe09bbc10f3838c1d8d6f793061fa5f38b2b4dac68ffffffff0118ddf505000000001976a9149934ebeaa56cb597c936a9ed82 │  
│                                   │ 02d8d97a0a700388ace6fabf64000000000000000000000000000000                                                               │  
│                                   │ tx_hash: 4f2cc7a83d7012c5d03fa64df188500db4bee51bbb9a6a0a1f06a50ca3409fdc                                              │  
│                                   │                                                                                                                        │  
"
```