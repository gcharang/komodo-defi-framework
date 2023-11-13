use super::*;
use crate::hd_wallet::{load_hd_accounts_from_storage, HDAccountAddressId, HDAccountsMutex, HDWalletCoinStorage,
                       HDWalletStorageError, DEFAULT_GAP_LIMIT};
#[cfg(target_arch = "wasm32")] use crate::EthMetamaskPolicy;
use common::executor::AbortedError;
use crypto::{trezor::TrezorError, Bip32Error, CryptoCtxError, HwError};
use enum_from::EnumFromTrait;
use mm2_err_handle::common_errors::WithInternal;
#[cfg(target_arch = "wasm32")]
use mm2_metamask::{from_metamask_error, MetamaskError, MetamaskRpcError, WithMetamaskRpcError};
//use rpc_task::rpc_common::{CancelRpcTaskError, CancelRpcTaskRequest, InitRpcTaskResponse, RpcTaskStatusError,
//    RpcTaskStatusRequest, RpcTaskUserActionError};
//use rpc_task::{RpcTask, RpcTaskError, RpcTaskHandle, RpcTaskManager, RpcTaskManagerShared, RpcTaskStatus, RpcTaskTypes};
use rpc_task::RpcTaskError;
//use crypto::hw_rpc_task::{HwConnectStatuses, HwRpcTaskAwaitingStatus, HwRpcTaskUserAction, HwRpcTaskUserActionRequest};

#[derive(Clone, Debug, Deserialize, Display, EnumFromTrait, PartialEq, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum EthActivationV2Error {
    InvalidPayload(String),
    InvalidSwapContractAddr(String),
    InvalidFallbackSwapContract(String),
    InvalidPathToAddress(String),
    #[display(fmt = "Expected either 'chain_id' or 'rpc_chain_id' to be set")]
    #[cfg(target_arch = "wasm32")]
    ExpectedRpcChainId,
    #[display(fmt = "Platform coin {} activation failed. {}", ticker, error)]
    ActivationFailed {
        ticker: String,
        error: String,
    },
    CouldNotFetchBalance(String),
    UnreachableNodes(String),
    #[display(fmt = "Enable request for ETH coin must have at least 1 node")]
    AtLeastOneNodeRequired,
    #[display(fmt = "Error deserializing 'derivation_path': {}", _0)]
    ErrorDeserializingDerivationPath(String),
    PrivKeyPolicyNotAllowed(PrivKeyPolicyNotAllowed),
    HDWalletStorageError(String),
    #[cfg(target_arch = "wasm32")]
    #[from_trait(WithMetamaskRpcError::metamask_rpc_error)]
    #[display(fmt = "{}", _0)]
    MetamaskError(MetamaskRpcError),
    #[from_trait(WithInternal::internal)]
    #[display(fmt = "Internal: {}", _0)]
    InternalError(String),
    CoinDoesntSupportTrezor,
    HwContextNotInitialized,
    #[display(fmt = "Initialization task has timed out {:?}", duration)]
    TaskTimedOut {
        duration: Duration,
    },
    HwError(HwRpcError),
    #[display(fmt = "Hardware wallet must be called within rpc task framework")]
    InvalidHardwareWalletCall,
}

impl From<MyAddressError> for EthActivationV2Error {
    fn from(err: MyAddressError) -> Self { Self::InternalError(err.to_string()) }
}

impl From<AbortedError> for EthActivationV2Error {
    fn from(e: AbortedError) -> Self { EthActivationV2Error::InternalError(e.to_string()) }
}

impl From<CryptoCtxError> for EthActivationV2Error {
    fn from(e: CryptoCtxError) -> Self { EthActivationV2Error::InternalError(e.to_string()) }
}

impl From<UnexpectedDerivationMethod> for EthActivationV2Error {
    fn from(e: UnexpectedDerivationMethod) -> Self { EthActivationV2Error::InternalError(e.to_string()) }
}

impl From<HDWalletStorageError> for EthActivationV2Error {
    fn from(e: HDWalletStorageError) -> Self { EthActivationV2Error::HDWalletStorageError(e.to_string()) }
}

impl From<HwError> for EthActivationV2Error {
    fn from(e: HwError) -> Self { EthActivationV2Error::InternalError(e.to_string()) }
}

impl From<Bip32Error> for EthActivationV2Error {
    fn from(e: Bip32Error) -> Self { EthActivationV2Error::InternalError(e.to_string()) }
}

impl From<TrezorError> for EthActivationV2Error {
    fn from(e: TrezorError) -> Self { EthActivationV2Error::InternalError(e.to_string()) }
}

impl From<RpcTaskError> for EthActivationV2Error {
    fn from(rpc_err: RpcTaskError) -> Self {
        match rpc_err {
            RpcTaskError::Timeout(duration) => EthActivationV2Error::TaskTimedOut { duration },
            internal_error => EthActivationV2Error::InternalError(internal_error.to_string()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<MetamaskError> for EthActivationV2Error {
    fn from(e: MetamaskError) -> Self { from_metamask_error(e) }
}

/// An alternative to `crate::PrivKeyActivationPolicy`, typical only for ETH coin.
#[derive(Clone, Deserialize)]
pub enum EthPrivKeyActivationPolicy {
    ContextPrivKey,
    #[cfg(target_arch = "wasm32")]
    Metamask,
    Trezor,
}

impl Default for EthPrivKeyActivationPolicy {
    fn default() -> Self { EthPrivKeyActivationPolicy::ContextPrivKey }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum EthRpcMode {
    Http,
    #[cfg(target_arch = "wasm32")]
    Metamask,
}

impl Default for EthRpcMode {
    fn default() -> Self { EthRpcMode::Http }
}

#[derive(Clone, Deserialize)]
pub struct EthActivationV2Request {
    #[serde(default)]
    pub nodes: Vec<EthNode>,
    #[serde(default)]
    pub rpc_mode: EthRpcMode,
    pub swap_contract_address: Address,
    pub fallback_swap_contract: Option<Address>,
    #[serde(default)]
    pub contract_supports_watchers: bool,
    pub gas_station_url: Option<String>,
    pub gas_station_decimals: Option<u8>,
    #[serde(default)]
    pub gas_station_policy: GasStationPricePolicy,
    pub mm2: Option<u8>,
    pub required_confirmations: Option<u64>,
    #[serde(default)]
    pub priv_key_policy: EthPrivKeyActivationPolicy,
    #[serde(flatten)]
    pub enable_params: EnabledCoinBalanceParams,
    #[serde(default)]
    pub path_to_address: HDAccountAddressId,
    pub gap_limit: Option<u32>,
}

#[derive(Clone, Deserialize)]
pub struct EthNode {
    pub url: String,
    #[serde(default)]
    pub gui_auth: bool,
}

#[derive(Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum Erc20TokenActivationError {
    InternalError(String),
    CouldNotFetchBalance(String),
    UnexpectedDerivationMethod(UnexpectedDerivationMethod),
}

impl From<AbortedError> for Erc20TokenActivationError {
    fn from(e: AbortedError) -> Self { Erc20TokenActivationError::InternalError(e.to_string()) }
}

impl From<MyAddressError> for Erc20TokenActivationError {
    fn from(err: MyAddressError) -> Self { Self::InternalError(err.to_string()) }
}

impl From<UnexpectedDerivationMethod> for Erc20TokenActivationError {
    fn from(e: UnexpectedDerivationMethod) -> Self { Erc20TokenActivationError::UnexpectedDerivationMethod(e) }
}

#[derive(Clone, Deserialize)]
pub struct Erc20TokenActivationRequest {
    pub required_confirmations: Option<u64>,
}

pub struct Erc20Protocol {
    pub platform: String,
    pub token_addr: Address,
}

#[cfg_attr(test, mockable)]
impl EthCoin {
    pub async fn initialize_erc20_token(
        &self,
        activation_params: Erc20TokenActivationRequest,
        protocol: Erc20Protocol,
        ticker: String,
    ) -> MmResult<EthCoin, Erc20TokenActivationError> {
        // TODO
        // Check if ctx is required.
        // Remove it to avoid circular references if possible
        let ctx = MmArc::from_weak(&self.ctx)
            .ok_or_else(|| String::from("No context"))
            .map_err(Erc20TokenActivationError::InternalError)?;

        let conf = coin_conf(&ctx, &ticker);

        let decimals = match conf["decimals"].as_u64() {
            None | Some(0) => get_token_decimals(&self.web3, protocol.token_addr)
                .await
                .map_err(Erc20TokenActivationError::InternalError)?,
            Some(d) => d as u8,
        };

        let web3_instances: Vec<Web3Instance> = self
            .web3_instances
            .iter()
            .map(|node| {
                let mut transport = node.web3.transport().clone();
                if let Some(auth) = transport.gui_auth_validation_generator_as_mut() {
                    auth.coin_ticker = ticker.clone();
                }
                let web3 = Web3::new(transport);
                Web3Instance {
                    web3,
                    is_parity: node.is_parity,
                }
            })
            .collect();

        let mut transport = self.web3.transport().clone();
        if let Some(auth) = transport.gui_auth_validation_generator_as_mut() {
            auth.coin_ticker = ticker.clone();
        }
        let web3 = Web3::new(transport);

        let required_confirmations = activation_params
            .required_confirmations
            .unwrap_or_else(|| conf["required_confirmations"].as_u64().unwrap_or(1))
            .into();

        // Create an abortable system linked to the `MmCtx` so if the app is stopped on `MmArc::stop`,
        // all spawned futures related to `ERC20` coin will be aborted as well.
        let abortable_system = ctx.abortable_system.create_subsystem()?;

        let token = EthCoinImpl {
            priv_key_policy: self.priv_key_policy.clone(),
            // We inherit the derivation method from the parent/platform coin
            // If we want a new wallet for each token we can add this as an option in the future
            // storage ticker will be the platform coin ticker
            // Todo: rescan all enabled addresses for this token and use gap limit as well to find other addresses that has this token balance
            // Todo: we should add an option for scan_for_new_addresses methods to scan only for this token or all enabled tokens and the platform coin
            derivation_method: self.derivation_method.clone(),
            coin_type: EthCoinType::Erc20 {
                platform: protocol.platform,
                token_addr: protocol.token_addr,
            },
            sign_message_prefix: self.sign_message_prefix.clone(),
            swap_contract_address: self.swap_contract_address,
            fallback_swap_contract: self.fallback_swap_contract,
            contract_supports_watchers: self.contract_supports_watchers,
            decimals,
            ticker,
            gas_station_url: self.gas_station_url.clone(),
            gas_station_decimals: self.gas_station_decimals,
            gas_station_policy: self.gas_station_policy.clone(),
            web3,
            web3_instances,
            history_sync_state: Mutex::new(self.history_sync_state.lock().unwrap().clone()),
            ctx: self.ctx.clone(),
            required_confirmations,
            chain_id: self.chain_id,
            trezor_coin: self.trezor_coin.clone(),
            logs_block_range: self.logs_block_range,
            nonce_lock: self.nonce_lock.clone(),
            erc20_tokens_infos: Default::default(),
            abortable_system,
        };

        Ok(EthCoin(Arc::new(token)))
    }
}

/// Activate eth coin from coin config and and private key build policy,
/// version 2 with no intrinsic tokens creation  
pub async fn eth_coin_from_conf_and_request_v2(
    ctx: &MmArc,
    ticker: &str,
    conf: &Json,
    req: EthActivationV2Request,
    priv_key_policy: EthPrivKeyBuildPolicy,
) -> MmResult<EthCoin, EthActivationV2Error> {
    if req.swap_contract_address == Address::default() {
        return Err(EthActivationV2Error::InvalidSwapContractAddr(
            "swap_contract_address can't be zero address".to_string(),
        )
        .into());
    }

    if let Some(fallback) = req.fallback_swap_contract {
        if fallback == Address::default() {
            return Err(EthActivationV2Error::InvalidFallbackSwapContract(
                "fallback_swap_contract can't be zero address".to_string(),
            )
            .into());
        }
    }

    let (priv_key_policy, derivation_method) =
        build_address_and_priv_key_policy(ctx, ticker, conf, priv_key_policy, &req.path_to_address, req.gap_limit)
            .await?;
    let enabled_address = match priv_key_policy {
        PrivKeyPolicy::Trezor {
            path_to_coin: _,
            ref activated_pubkey,
        } => {
            let my_pubkey = activated_pubkey
                .as_ref()
                .or_mm_err(|| EthActivationV2Error::InternalError("no pubkey from trezor".to_string()))?;
            let my_pubkey = pubkey_from_xpub_str(my_pubkey)
                .ok_or_else(|| EthActivationV2Error::InternalError("invalid xpub from trezor".to_string()))?;
            public_to_address(&my_pubkey)
        },
        _ => priv_key_policy
            .activated_key_or_err()
            .map_err(|e| EthActivationV2Error::PrivKeyPolicyNotAllowed(e.into_inner()))?
            .address(),
    };
    let enabled_address_str = display_eth_address(&enabled_address);

    let chain_id = conf["chain_id"].as_u64();

    let (web3, web3_instances) = match (req.rpc_mode, &priv_key_policy) {
        (
            EthRpcMode::Http,
            EthPrivKeyPolicy::Iguana(key_pair)
            | EthPrivKeyPolicy::HDWallet {
                activated_key: key_pair,
                ..
            },
        ) => build_http_transport(ctx, ticker.to_string(), enabled_address_str, key_pair, &req.nodes).await?,
        (EthRpcMode::Http, EthPrivKeyPolicy::Trezor { .. }) => {
            /*return MmError::err(EthActivationV2Error::PrivKeyPolicyNotAllowed(
                PrivKeyPolicyNotAllowed::HardwareWalletNotSupported,
            ));*/
            // for now in-memory privkey which must be always initialised if trezor policy is set
            let crypto_ctx = CryptoCtx::from_ctx(ctx)?;
            let secp256k1_key_pair = crypto_ctx.mm2_internal_key_pair();
            let eth_key_pair = eth::KeyPair::from_secret_slice(&secp256k1_key_pair.private_bytes())
                .map_to_mm(|_| EthActivationV2Error::InternalError("could not get internal keypair".to_string()))?;
            build_http_transport(ctx, ticker.to_string(), enabled_address_str, &eth_key_pair, &req.nodes).await?
        },
        #[cfg(target_arch = "wasm32")]
        (EthRpcMode::Metamask, EthPrivKeyPolicy::Metamask(_)) => {
            let chain_id = chain_id
                .or_else(|| conf["rpc_chain_id"].as_u64())
                .or_mm_err(|| EthActivationV2Error::ExpectedRpcChainId)?;
            build_metamask_transport(ctx, ticker.to_string(), chain_id).await?
        },
        #[cfg(target_arch = "wasm32")]
        (EthRpcMode::Http, EthPrivKeyPolicy::Metamask(_)) | (EthRpcMode::Metamask, _) => {
            let error = r#"priv_key_policy="Metamask" and rpc_mode="Metamask" should be used both"#.to_string();
            return MmError::err(EthActivationV2Error::ActivationFailed {
                ticker: ticker.to_string(),
                error,
            });
        },
    };

    // param from request should override the config
    let required_confirmations = req
        .required_confirmations
        .unwrap_or_else(|| {
            conf["required_confirmations"]
                .as_u64()
                .unwrap_or(DEFAULT_REQUIRED_CONFIRMATIONS as u64)
        })
        .into();

    let sign_message_prefix: Option<String> = json::from_value(conf["sign_message_prefix"].clone()).ok();

    let trezor_coin: Option<String> = json::from_value(conf["trezor_coin"].clone()).ok();

    let mut map = NONCE_LOCK.lock().unwrap();
    let nonce_lock = map.entry(ticker.to_string()).or_insert_with(new_nonce_lock).clone();

    // Create an abortable system linked to the `MmCtx` so if the app is stopped on `MmArc::stop`,
    // all spawned futures related to `ETH` coin will be aborted as well.
    let abortable_system = ctx.abortable_system.create_subsystem()?;

    let coin = EthCoinImpl {
        priv_key_policy,
        derivation_method: Arc::new(derivation_method),
        coin_type: EthCoinType::Eth,
        sign_message_prefix,
        swap_contract_address: req.swap_contract_address,
        fallback_swap_contract: req.fallback_swap_contract,
        contract_supports_watchers: req.contract_supports_watchers,
        decimals: ETH_DECIMALS,
        ticker: ticker.to_string(),
        gas_station_url: req.gas_station_url,
        gas_station_decimals: req.gas_station_decimals.unwrap_or(ETH_GAS_STATION_DECIMALS),
        gas_station_policy: req.gas_station_policy,
        web3,
        web3_instances,
        history_sync_state: Mutex::new(HistorySyncState::NotEnabled),
        ctx: ctx.weak(),
        required_confirmations,
        chain_id,
        trezor_coin,
        logs_block_range: conf["logs_block_range"].as_u64().unwrap_or(DEFAULT_LOGS_BLOCK_RANGE),
        nonce_lock,
        erc20_tokens_infos: Default::default(),
        abortable_system,
    };

    Ok(EthCoin(Arc::new(coin)))
}

// Todo: This function can be refactored to use builder pattern like UTXO
/// Processes the given `priv_key_policy` and generates corresponding `KeyPair`.
/// This function expects either [`PrivKeyBuildPolicy::IguanaPrivKey`]
/// or [`PrivKeyBuildPolicy::GlobalHDAccount`], otherwise returns `PrivKeyPolicyNotAllowed` error.
pub(crate) async fn build_address_and_priv_key_policy(
    ctx: &MmArc,
    ticker: &str,
    conf: &Json,
    priv_key_policy: EthPrivKeyBuildPolicy,
    path_to_address: &HDAccountAddressId,
    gap_limit: Option<u32>,
) -> MmResult<(EthPrivKeyPolicy, EthDerivationMethod), EthActivationV2Error> {
    match priv_key_policy {
        EthPrivKeyBuildPolicy::IguanaPrivKey(iguana) => {
            let key_pair = KeyPair::from_secret_slice(iguana.as_slice())
                .map_to_mm(|e| EthActivationV2Error::InternalError(e.to_string()))?;
            let address = key_pair.address();
            let derivation_method = DerivationMethod::SingleAddress(address);
            Ok((EthPrivKeyPolicy::Iguana(key_pair), derivation_method))
        },
        EthPrivKeyBuildPolicy::GlobalHDAccount(global_hd_ctx) => {
            // Consider storing `derivation_path` at `EthCoinImpl`.
            let path_to_coin = json::from_value(conf["derivation_path"].clone())
                .map_to_mm(|e| EthActivationV2Error::ErrorDeserializingDerivationPath(e.to_string()))?;
            let raw_priv_key = global_hd_ctx
                .derive_secp256k1_secret(
                    &path_to_address
                        .to_derivation_path(&path_to_coin)
                        .mm_err(|e| EthActivationV2Error::InvalidPathToAddress(e.to_string()))?,
                )
                .mm_err(|e| EthActivationV2Error::InternalError(e.to_string()))?;
            let activated_key = KeyPair::from_secret_slice(raw_priv_key.as_slice())
                .map_to_mm(|e| EthActivationV2Error::InternalError(e.to_string()))?;
            let bip39_secp_priv_key = global_hd_ctx.root_priv_key().clone();

            let hd_wallet_rmd160 = *ctx.rmd160();
            let hd_wallet_storage = HDWalletCoinStorage::init_with_rmd160(ctx, ticker.to_string(), hd_wallet_rmd160)
                .await
                .mm_err(EthActivationV2Error::from)?;
            let accounts = load_hd_accounts_from_storage(&hd_wallet_storage, &path_to_coin).await?;
            let gap_limit = gap_limit.unwrap_or(DEFAULT_GAP_LIMIT);
            let hd_wallet = EthHDWallet {
                hd_wallet_rmd160,
                hd_wallet_storage,
                derivation_path: path_to_coin.clone(),
                accounts: HDAccountsMutex::new(accounts),
                enabled_address: Some(*path_to_address),
                gap_limit,
            };
            let derivation_method = DerivationMethod::HDWallet(hd_wallet);
            Ok((
                EthPrivKeyPolicy::HDWallet {
                    path_to_coin,
                    activated_key,
                    bip39_secp_priv_key,
                },
                derivation_method,
            ))
        },
        EthPrivKeyBuildPolicy::Trezor => {
            let path_to_coin = json::from_value(conf["derivation_path"].clone())
                .map_to_mm(|e| EthActivationV2Error::ErrorDeserializingDerivationPath(e.to_string()))?;

            let trezor_coin: Option<String> = json::from_value(conf["trezor_coin"].clone()).ok();
            if trezor_coin.is_none() {
                return MmError::err(EthActivationV2Error::CoinDoesntSupportTrezor);
            }
            let crypto_ctx = CryptoCtx::from_ctx(ctx)?;
            let hw_ctx = crypto_ctx
                .hw_ctx()
                .or_mm_err(|| EthActivationV2Error::HwContextNotInitialized)?;
            let hd_wallet_rmd160 = hw_ctx.rmd160();
            let hd_wallet_storage = HDWalletCoinStorage::init_with_rmd160(ctx, ticker.to_string(), hd_wallet_rmd160)
                .await
                .mm_err(EthActivationV2Error::from)?;
            let accounts = load_hd_accounts_from_storage(&hd_wallet_storage, &path_to_coin).await?;
            // Todo: use fn gap_limit(&self) -> u32 { self.activation_params().gap_limit.unwrap_or(DEFAULT_GAP_LIMIT) } like UTXO
            let gap_limit = DEFAULT_GAP_LIMIT;
            // Todo: Maybe we can make a constructor for HDWallet struct
            let hd_wallet = EthHDWallet {
                hd_wallet_rmd160,
                hd_wallet_storage,
                derivation_path: path_to_coin.clone(),
                accounts: HDAccountsMutex::new(accounts),
                enabled_address: Some(*path_to_address),
                gap_limit,
            };
            let derivation_method = DerivationMethod::HDWallet(hd_wallet);
            let derivation_path = path_to_address.to_derivation_path(&path_to_coin)?;
            let mut trezor_session = hw_ctx.trezor().await?;
            let my_pubkey = trezor_session
                .get_eth_public_key(derivation_path, false)
                .await?
                .ack_all()
                .await?;
            Ok((
                EthPrivKeyPolicy::Trezor {
                    path_to_coin: Some(path_to_coin),
                    activated_pubkey: Some(my_pubkey),
                },
                derivation_method,
            ))
        },
        #[cfg(target_arch = "wasm32")]
        EthPrivKeyBuildPolicy::Metamask(metamask_ctx) => {
            let address = *metamask_ctx.check_active_eth_account().await?;
            let public_key_uncompressed = metamask_ctx.eth_account_pubkey_uncompressed();
            let public_key = compress_public_key(public_key_uncompressed)?;
            Ok((
                EthPrivKeyPolicy::Metamask(EthMetamaskPolicy {
                    public_key,
                    public_key_uncompressed,
                }),
                DerivationMethod::SingleAddress(address),
            ))
        },
    }
}

async fn build_http_transport(
    ctx: &MmArc,
    coin_ticker: String,
    address: String,
    key_pair: &KeyPair,
    eth_nodes: &[EthNode],
) -> MmResult<(Web3<Web3Transport>, Vec<Web3Instance>), EthActivationV2Error> {
    if eth_nodes.is_empty() {
        return MmError::err(EthActivationV2Error::AtLeastOneNodeRequired);
    }

    let mut http_nodes = vec![];
    for node in eth_nodes {
        let uri = node
            .url
            .parse()
            .map_err(|_| EthActivationV2Error::InvalidPayload(format!("{} could not be parsed.", node.url)))?;

        http_nodes.push(HttpTransportNode {
            uri,
            gui_auth: node.gui_auth,
        });
    }

    let mut rng = small_rng();
    http_nodes.as_mut_slice().shuffle(&mut rng);

    drop_mutability!(http_nodes);

    let mut web3_instances = Vec::with_capacity(http_nodes.len());
    let event_handlers = rpc_event_handlers_for_eth_transport(ctx, coin_ticker.clone());
    for node in http_nodes.iter() {
        let transport = build_single_http_transport(
            coin_ticker.clone(),
            address.clone(),
            key_pair,
            vec![node.clone()],
            event_handlers.clone(),
        );

        let web3 = Web3::new(transport);
        let version = match web3.web3().client_version().await {
            Ok(v) => v,
            Err(e) => {
                error!("Couldn't get client version for url {}: {}", node.uri, e);
                continue;
            },
        };
        web3_instances.push(Web3Instance {
            web3,
            is_parity: version.contains("Parity") || version.contains("parity"),
        })
    }

    if web3_instances.is_empty() {
        return Err(
            EthActivationV2Error::UnreachableNodes("Failed to get client version for all nodes".to_string()).into(),
        );
    }

    let transport = build_single_http_transport(coin_ticker, address, key_pair, http_nodes, event_handlers);
    let web3 = Web3::new(transport);

    Ok((web3, web3_instances))
}

fn build_single_http_transport(
    coin_ticker: String,
    address: String,
    key_pair: &KeyPair,
    nodes: Vec<HttpTransportNode>,
    event_handlers: Vec<RpcTransportEventHandlerShared>,
) -> Web3Transport {
    use crate::eth::web3_transport::http_transport::HttpTransport;

    let mut http_transport = HttpTransport::with_event_handlers(nodes, event_handlers);
    http_transport.gui_auth_validation_generator = Some(GuiAuthValidationGenerator {
        coin_ticker,
        secret: key_pair.secret().clone(),
        address,
    });
    Web3Transport::from(http_transport)
}

#[cfg(target_arch = "wasm32")]
async fn build_metamask_transport(
    ctx: &MmArc,
    coin_ticker: String,
    chain_id: u64,
) -> MmResult<(Web3<Web3Transport>, Vec<Web3Instance>), EthActivationV2Error> {
    let event_handlers = rpc_event_handlers_for_eth_transport(ctx, coin_ticker.clone());

    let eth_config = web3_transport::metamask_transport::MetamaskEthConfig { chain_id };
    let web3 = Web3::new(Web3Transport::new_metamask(eth_config, event_handlers)?);

    // Check if MetaMask supports the given `chain_id`.
    // Please note that this request may take a long time.
    check_metamask_supports_chain_id(coin_ticker, &web3, chain_id).await?;

    // MetaMask doesn't use Parity nodes. So `MetamaskTransport` doesn't support `parity_nextNonce` RPC.
    // An example of the `web3_clientVersion` RPC - `MetaMask/v10.22.1`.
    let web3_instances = vec![Web3Instance {
        web3: web3.clone(),
        is_parity: false,
    }];

    Ok((web3, web3_instances))
}

/// This method is based on the fact that `MetamaskTransport` tries to switch the `ChainId`
/// if the MetaMask is targeted to another ETH chain.
#[cfg(target_arch = "wasm32")]
async fn check_metamask_supports_chain_id(
    ticker: String,
    web3: &Web3<Web3Transport>,
    expected_chain_id: u64,
) -> MmResult<(), EthActivationV2Error> {
    use jsonrpc_core::ErrorCode;

    /// See the documentation:
    /// https://docs.metamask.io/guide/rpc-api.html#wallet-switchethereumchain
    const CHAIN_IS_NOT_REGISTERED_ERROR: ErrorCode = ErrorCode::ServerError(4902);

    match web3.eth().chain_id().await {
        Ok(chain_id) if chain_id == U256::from(expected_chain_id) => Ok(()),
        // The RPC client should have returned ChainId with which it has been created on [`Web3Transport::new_metamask`].
        Ok(unexpected_chain_id) => {
            let error = format!("Expected '{expected_chain_id}' ChainId, found '{unexpected_chain_id}'");
            MmError::err(EthActivationV2Error::InternalError(error))
        },
        Err(web3::Error::Rpc(rpc_err)) if rpc_err.code == CHAIN_IS_NOT_REGISTERED_ERROR => {
            let error = format!("Ethereum chain_id({expected_chain_id}) is not supported");
            MmError::err(EthActivationV2Error::ActivationFailed { ticker, error })
        },
        Err(other) => {
            let error = other.to_string();
            MmError::err(EthActivationV2Error::ActivationFailed { ticker, error })
        },
    }
}

#[cfg(target_arch = "wasm32")]
fn compress_public_key(uncompressed: H520) -> MmResult<H264, EthActivationV2Error> {
    let public_key = PublicKey::from_slice(uncompressed.as_bytes())
        .map_to_mm(|e| EthActivationV2Error::InternalError(e.to_string()))?;
    let compressed = public_key.serialize();
    Ok(H264::from(compressed))
}
