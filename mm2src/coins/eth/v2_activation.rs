use super::*;
use crate::hd_wallet::{load_hd_accounts_from_storage, HDAccountAddressId, HDAccountsMutex, HDWalletCoinStorage,
                       HDWalletStorageError, DEFAULT_GAP_LIMIT};
#[cfg(target_arch = "wasm32")] use crate::EthMetamaskPolicy;
use common::executor::AbortedError;
use crypto::CryptoCtxError;
use enum_from::EnumFromTrait;
use mm2_err_handle::common_errors::WithInternal;
#[cfg(target_arch = "wasm32")]
use mm2_metamask::{from_metamask_error, MetamaskError, MetamaskRpcError, WithMetamaskRpcError};

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
    #[serde(default)]
    pub path_to_address: HDAccountAddressId,
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
}

impl From<AbortedError> for Erc20TokenActivationError {
    fn from(e: AbortedError) -> Self { Erc20TokenActivationError::InternalError(e.to_string()) }
}

impl From<MyAddressError> for Erc20TokenActivationError {
    fn from(err: MyAddressError) -> Self { Self::InternalError(err.to_string()) }
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
            // Todo: we should inherit the derivation method from the parent coin? we should add an option to have a new arc if we want to use a different hd wallet for a specific or all erc20 tokens
            // Todo: check storage for HD in this case? what about balances, db, etc..? what if ETH is enabled without tokens as a standalone hd wallet after erc20 tokens were added to the HD wallet storage?
            // Todo: will probably need to do rescanning of all addresses or some other thing when enabling a token after enabling eth?
            // Todo: what about Address poisoning attacks when scanning for non-empty addresses? usually these attacks are sent to already used addresses anyways.
            // Todo: how about scanning for tokens feature? will that be too much? Maybe another API?
            // Todo: eth is needed to send from the same account some erc20 tokens, so having the same HD wallet for all erc20 tokens is good
            derivation_method: self.derivation_method.clone(),
            my_address: self.my_address,
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

    let (my_address, priv_key_policy, derivation_method) =
        build_address_and_priv_key_policy(ctx, ticker, conf, priv_key_policy, &req.path_to_address).await?;
    let my_address_str = checksum_address(&format!("{:02x}", my_address));

    let chain_id = conf["chain_id"].as_u64();

    let (web3, web3_instances) = match (req.rpc_mode, &priv_key_policy) {
        (
            EthRpcMode::Http,
            EthPrivKeyPolicy::Iguana(key_pair)
            | EthPrivKeyPolicy::HDWallet {
                activated_key: key_pair,
                ..
            },
        ) => build_http_transport(ctx, ticker.to_string(), my_address_str, key_pair, &req.nodes).await?,
        (EthRpcMode::Http, EthPrivKeyPolicy::Trezor) => {
            return MmError::err(EthActivationV2Error::PrivKeyPolicyNotAllowed(
                PrivKeyPolicyNotAllowed::HardwareWalletNotSupported,
            ));
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
        my_address,
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

/// Processes the given `priv_key_policy` and generates corresponding `KeyPair`.
/// This function expects either [`PrivKeyBuildPolicy::IguanaPrivKey`]
/// or [`PrivKeyBuildPolicy::GlobalHDAccount`], otherwise returns `PrivKeyPolicyNotAllowed` error.
pub(crate) async fn build_address_and_priv_key_policy(
    ctx: &MmArc,
    ticker: &str,
    conf: &Json,
    priv_key_policy: EthPrivKeyBuildPolicy,
    path_to_address: &HDAccountAddressId,
    // Todo: refactor this to use builder pattern like UTXO or at least make the return not a tuple that has 3 elements
    // Todo: make DerivationMethod<Address, EthHDWallet> a type
) -> MmResult<(Address, EthPrivKeyPolicy, DerivationMethod<Address, EthHDWallet>), EthActivationV2Error> {
    match priv_key_policy {
        EthPrivKeyBuildPolicy::IguanaPrivKey(iguana) => {
            let key_pair = KeyPair::from_secret_slice(iguana.as_slice())
                .map_to_mm(|e| EthActivationV2Error::InternalError(e.to_string()))?;
            let address = key_pair.address();
            let derivation_method = DerivationMethod::SingleAddress(address);
            Ok((address, EthPrivKeyPolicy::Iguana(key_pair), derivation_method))
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
            Ok((
                activated_key.address(),
                EthPrivKeyPolicy::HDWallet {
                    path_to_coin,
                    activated_key,
                    bip39_secp_priv_key,
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
                address,
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
