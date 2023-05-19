use crate::lightning::ln_conf::ChannelOptions;
use crate::lightning::ln_db::{DBPaymentsFilter, HTLCStatus, PaymentInfo, PaymentType};
use crate::lightning::ln_platform::h256_json_from_txid;
use crate::H256Json;
use lightning::chain::channelmonitor::Balance;
use lightning::ln::channelmanager::ChannelDetails;
use secp256k1v24::PublicKey;
use serde::{de, Serialize, Serializer};
use std::fmt;
use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use uuid::Uuid;

// TODO: support connection to onion addresses
#[derive(Debug, PartialEq)]
pub struct NodeAddress {
    pub pubkey: PublicKey,
    pub addr: SocketAddr,
}

impl Serialize for NodeAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}@{}", self.pubkey, self.addr))
    }
}

impl<'de> de::Deserialize<'de> for NodeAddress {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NodeAddressVisitor;

        impl<'de> de::Visitor<'de> for NodeAddressVisitor {
            type Value = NodeAddress;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result { write!(formatter, "pubkey@host:port") }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let mut pubkey_and_addr = v.split('@');
                let pubkey_str = pubkey_and_addr.next().ok_or_else(|| {
                    let err = format!("Could not parse node address from str {}", v);
                    de::Error::custom(err)
                })?;
                let addr_str = pubkey_and_addr.next().ok_or_else(|| {
                    let err = format!("Could not parse node address from str {}", v);
                    de::Error::custom(err)
                })?;
                let pubkey = PublicKey::from_str(pubkey_str).map_err(|e| {
                    let err = format!("Could not parse node pubkey from str {}, err {}", pubkey_str, e);
                    de::Error::custom(err)
                })?;
                let addr = addr_str
                    .to_socket_addrs()
                    .map(|mut r| r.next())
                    .map_err(|e| {
                        let err = format!("Could not parse socket address from str {}, err {}", addr_str, e);
                        de::Error::custom(err)
                    })?
                    .ok_or_else(|| {
                        let err = format!("Could not parse socket address from str {}", addr_str);
                        de::Error::custom(err)
                    })?;
                Ok(NodeAddress { pubkey, addr })
            }
        }

        deserializer.deserialize_str(NodeAddressVisitor)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicKeyForRPC(pub PublicKey);

impl From<PublicKeyForRPC> for PublicKey {
    fn from(p: PublicKeyForRPC) -> Self { p.0 }
}

impl Serialize for PublicKeyForRPC {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> de::Deserialize<'de> for PublicKeyForRPC {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PublicKeyForRPCVisitor;

        impl<'de> de::Visitor<'de> for PublicKeyForRPCVisitor {
            type Value = PublicKeyForRPC;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result { write!(formatter, "a public key") }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let pubkey = PublicKey::from_str(v).map_err(|e| {
                    let err = format!("Could not parse public key from str {}, err {}", v, e);
                    de::Error::custom(err)
                })?;
                Ok(PublicKeyForRPC(pubkey))
            }
        }

        deserializer.deserialize_str(PublicKeyForRPCVisitor)
    }
}

#[derive(Clone, Serialize)]
pub struct CounterpartyForwardingInfo {
    /// Base routing fee in millisatoshis.
    pub fee_base_msat: u32,
    /// Amount in millionths of a satoshi the channel will charge per transferred satoshi.
    pub fee_proportional_millionths: u32,
    /// The minimum difference in cltv_expiry between an ingoing HTLC and its outgoing counterpart for this counterparty.
    pub cltv_expiry_delta: u16,
}

#[derive(Clone, Serialize)]
pub struct CounterpartyDetails {
    /// The node_id of our counterparty
    pub node_id: PublicKeyForRPC,
    /// The value, in satoshis, that must always be held in the channel for our counterparty.
    /// This value ensures that if our counterparty broadcasts a revoked state, we can punish them by claiming at least this value on chain.
    /// This value is not included in inbound_capacity_msat as it can never be spent.
    pub unspendable_punishment_reserve: u64,
    /// Information on the fees and requirements that the counterparty requires when forwarding payments to us through this channel.
    pub forwarding_info: Option<CounterpartyForwardingInfo>,
    /// The smallest value HTLC (in msat) the remote peer will accept, for this channel.
    pub outbound_htlc_minimum_msat: Option<u64>,
    /// The largest value HTLC (in msat) the remote peer currently will accept, for this channel.
    pub outbound_htlc_maximum_msat: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct ChannelDetailsForRPC {
    /// An internal identifier for the channel that doesn't change throughout the channels lifetime.
    pub uuid: Uuid,
    /// The channel's ID, prior to funding transaction generation, this is a random 32 bytes,
    /// after funding transaction generation, this is the txid of the funding transaction xor the funding transaction output.
    /// Note that this means this value is *not* persistent - it can change once during the lifetime of the channel.
    pub channel_id: H256Json,
    /// The position of the funding transaction in the chain.
    /// None if the funding transaction has not yet been confirmed and the channel has not been fully opened.
    pub short_channel_id: Option<u64>,
    pub funding_tx: Option<H256Json>,
    pub funding_tx_output_index: Option<u16>,
    pub funding_tx_value_sats: u64,
    /// True if the channel was initiated (and thus funded) by us.
    pub is_outbound: bool,
    pub balance_msat: u64,
    // Todo: should include these in their own structs
    pub outbound_capacity_msat: u64,
    /// The available outbound capacity for sending a single HTLC to the remote peer.
    /// This is similar to outbound_capacity_msat but it may be further restricted by the current state and per-HTLC limit(s).
    // Todo: can this be used straight in protocol specific balance and in balance checks instead of calculating it?
    pub next_outbound_htlc_limit_msat: u64,
    /// The value, in satoshis, that must always be held in the channel for us.
    /// This value ensures that if we broadcast a revoked state, our counterparty can punish us by claiming at least this value on chain.
    /// This value is not included in outbound_capacity_msat as it can never be spent.
    /// This value will be None for outbound channels until the counterparty accepts the channel.
    pub unspendable_punishment_reserve: Option<u64>,
    // Todo: should include these in their own structs
    pub inbound_capacity_msat: u64,
    /// The smallest value HTLC (in msat) we will accept, for this channel.
    pub inbound_htlc_minimum_msat: Option<u64>,
    /// The largest value HTLC (in msat) we currently will accept, for this channel.
    pub inbound_htlc_maximum_msat: Option<u64>,
    /// Details about the channel's counterparty.
    pub counterparty_details: CounterpartyDetails,
    pub current_confirmations: Option<u32>,
    pub required_confirmations: Option<u32>,
    /// Channel is confirmed onchain, this means that funding_locked messages have been exchanged,
    /// the channel is not currently being shut down, and the required confirmation count has been reached.
    pub is_ready: bool,
    /// Channel is confirmed and channel_ready messages have been exchanged, the peer is connected,
    /// and the channel is not currently negotiating a shutdown.
    pub is_usable: bool,
    /// A publicly-announced channel.
    pub is_public: bool,
    /// The number of blocks (after our commitment transaction confirms) that we will need to
    /// wait until we can claim our funds after we force-close the channel.
    /// If our counterparty force-closes the channel and broadcasts a commitment transaction
    /// we do not have to wait any time to claim our non-HTLC-encumbered funds.
    pub force_close_spend_delay: Option<u16>,
    // Todo: Maybe I should rename it to ChannelConfig to be inline with rust-lightning
    /// Set of configurable parameters that affect channel operation
    /// Options may change at runtime or based on negotiation with our counterparty.
    /// To negotiate new channel options from our side, [`lightning::channels::update_channel`] can be used.
    channel_options: Option<ChannelOptions>,
}

impl From<ChannelDetails> for ChannelDetailsForRPC {
    fn from(details: ChannelDetails) -> ChannelDetailsForRPC {
        ChannelDetailsForRPC {
            uuid: Uuid::from_u128(details.user_channel_id),
            channel_id: details.channel_id.into(),
            short_channel_id: details.short_channel_id,
            funding_tx: details.funding_txo.map(|tx| h256_json_from_txid(tx.txid)),
            funding_tx_output_index: details.funding_txo.map(|tx| tx.index),
            funding_tx_value_sats: details.channel_value_satoshis,
            is_outbound: details.is_outbound,
            balance_msat: details.balance_msat,
            outbound_capacity_msat: details.outbound_capacity_msat,
            next_outbound_htlc_limit_msat: details.next_outbound_htlc_limit_msat,
            unspendable_punishment_reserve: details.unspendable_punishment_reserve,
            inbound_capacity_msat: details.inbound_capacity_msat,
            inbound_htlc_minimum_msat: details.inbound_htlc_minimum_msat,
            inbound_htlc_maximum_msat: details.inbound_htlc_maximum_msat,
            counterparty_details: CounterpartyDetails {
                node_id: PublicKeyForRPC(details.counterparty.node_id),
                unspendable_punishment_reserve: details.counterparty.unspendable_punishment_reserve,
                forwarding_info: details
                    .counterparty
                    .forwarding_info
                    .map(|fwd_info| CounterpartyForwardingInfo {
                        fee_base_msat: fwd_info.fee_base_msat,
                        fee_proportional_millionths: fwd_info.fee_proportional_millionths,
                        cltv_expiry_delta: fwd_info.cltv_expiry_delta,
                    }),
                outbound_htlc_minimum_msat: details.counterparty.outbound_htlc_minimum_msat,
                outbound_htlc_maximum_msat: details.counterparty.outbound_htlc_maximum_msat,
            },
            current_confirmations: details.confirmations,
            required_confirmations: details.confirmations_required,
            is_ready: details.is_channel_ready,
            is_usable: details.is_usable,
            is_public: details.is_public,
            force_close_spend_delay: details.force_close_spend_delay,
            channel_options: details.config.map(|config| ChannelOptions {
                proportional_fee_in_millionths_sats: Some(config.forwarding_fee_proportional_millionths),
                base_fee_msat: Some(config.forwarding_fee_base_msat),
                cltv_expiry_delta: Some(config.cltv_expiry_delta),
                max_dust_htlc_exposure_msat: Some(config.max_dust_htlc_exposure_msat),
                force_close_avoidance_max_fee_sats: Some(config.force_close_avoidance_max_fee_satoshis),
            }),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum PaymentTypeForRPC {
    #[serde(rename = "Outbound Payment")]
    OutboundPayment { destination: PublicKeyForRPC },
    #[serde(rename = "Inbound Payment")]
    InboundPayment,
}

impl From<PaymentType> for PaymentTypeForRPC {
    fn from(payment_type: PaymentType) -> Self {
        match payment_type {
            PaymentType::OutboundPayment { destination } => PaymentTypeForRPC::OutboundPayment {
                destination: PublicKeyForRPC(destination),
            },
            PaymentType::InboundPayment => PaymentTypeForRPC::InboundPayment,
        }
    }
}

impl From<PaymentTypeForRPC> for PaymentType {
    fn from(payment_type: PaymentTypeForRPC) -> Self {
        match payment_type {
            PaymentTypeForRPC::OutboundPayment { destination } => PaymentType::OutboundPayment {
                destination: destination.into(),
            },
            PaymentTypeForRPC::InboundPayment => PaymentType::InboundPayment,
        }
    }
}

#[derive(Serialize)]
pub struct PaymentInfoForRPC {
    payment_hash: H256Json,
    payment_type: PaymentTypeForRPC,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_in_msat: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fee_paid_msat: Option<i64>,
    status: HTLCStatus,
    created_at: i64,
    last_updated: i64,
}

impl From<PaymentInfo> for PaymentInfoForRPC {
    fn from(info: PaymentInfo) -> Self {
        PaymentInfoForRPC {
            payment_hash: info.payment_hash.0.into(),
            payment_type: info.payment_type.into(),
            description: info.description,
            amount_in_msat: info.amt_msat,
            fee_paid_msat: info.fee_paid_msat,
            status: info.status,
            created_at: info.created_at,
            last_updated: info.last_updated,
        }
    }
}

#[derive(Deserialize)]
pub struct PaymentsFilterForRPC {
    pub payment_type: Option<PaymentTypeForRPC>,
    pub description: Option<String>,
    pub status: Option<HTLCStatus>,
    pub from_amount_msat: Option<u64>,
    pub to_amount_msat: Option<u64>,
    pub from_fee_paid_msat: Option<u64>,
    pub to_fee_paid_msat: Option<u64>,
    pub from_timestamp: Option<u64>,
    pub to_timestamp: Option<u64>,
}

impl From<PaymentsFilterForRPC> for DBPaymentsFilter {
    fn from(filter: PaymentsFilterForRPC) -> Self {
        let (is_outbound, destination) = if let Some(payment_type) = filter.payment_type {
            match payment_type {
                PaymentTypeForRPC::OutboundPayment { destination } => (Some(true), Some(destination.0.to_string())),
                PaymentTypeForRPC::InboundPayment => (Some(false), None),
            }
        } else {
            (None, None)
        };
        DBPaymentsFilter {
            is_outbound,
            destination,
            description: filter.description,
            status: filter.status.map(|s| s.to_string()),
            from_amount_msat: filter.from_amount_msat.map(|a| a as i64),
            to_amount_msat: filter.to_amount_msat.map(|a| a as i64),
            from_fee_paid_msat: filter.from_fee_paid_msat.map(|f| f as i64),
            to_fee_paid_msat: filter.to_fee_paid_msat.map(|f| f as i64),
            from_timestamp: filter.from_timestamp.map(|f| f as i64),
            to_timestamp: filter.to_timestamp.map(|f| f as i64),
        }
    }
}

/// Details about the balance(s) available for spending once the channel appears on chain.
#[derive(Serialize)]
pub enum ClaimableBalance {
    /// The channel is not yet closed (or the commitment or closing transaction has not yet
    /// appeared in a block). The given balance is claimable (less on-chain fees) if the channel is
    /// force-closed now.
    ClaimableOnChannelClose {
        /// The amount available to claim, in satoshis, excluding the on-chain fees which will be
        /// required to do so.
        claimable_amount_satoshis: u64,
    },
    /// The channel has been closed, and the given balance is ours but awaiting confirmations until
    /// we consider it spendable.
    ClaimableAwaitingConfirmations {
        /// The amount available to claim, in satoshis, possibly excluding the on-chain fees which
        /// were spent in broadcasting the transaction.
        claimable_amount_satoshis: u64,
        /// The height at which an [`Event::SpendableOutputs`] event will be generated for this
        /// amount.
        confirmation_height: u32,
    },
    /// The channel has been closed, and the given balance should be ours but awaiting spending
    /// transaction confirmation. If the spending transaction does not confirm in time, it is
    /// possible our counterparty can take the funds by broadcasting an HTLC timeout on-chain.
    ///
    /// Once the spending transaction confirms, before it has reached enough confirmations to be
    /// considered safe from chain reorganizations, the balance will instead be provided via
    /// [`Balance::ClaimableAwaitingConfirmations`].
    ContentiousClaimable {
        /// The amount available to claim, in satoshis, excluding the on-chain fees which will be
        /// required to do so.
        claimable_amount_satoshis: u64,
        /// The height at which the counterparty may be able to claim the balance if we have not
        /// done so.
        timeout_height: u32,
    },
    /// HTLCs which we sent to our counterparty which are claimable after a timeout (less on-chain
    /// fees) if the counterparty does not know the preimage for the HTLCs. These are somewhat
    /// likely to be claimed by our counterparty before we do.
    MaybeTimeoutClaimableHTLC {
        /// The amount available to claim, in satoshis, excluding the on-chain fees which will be
        /// required to do so.
        claimable_amount_satoshis: u64,
        /// The height at which we will be able to claim the balance if our counterparty has not
        /// done so.
        claimable_height: u32,
    },
    /// HTLCs which we received from our counterparty which are claimable with a preimage which we
    /// do not currently have. This will only be claimable if we receive the preimage from the node
    /// to which we forwarded this HTLC before the timeout.
    MaybePreimageClaimableHTLC {
        /// The amount potentially available to claim, in satoshis, excluding the on-chain fees
        /// which will be required to do so.
        claimable_amount_satoshis: u64,
        /// The height at which our counterparty will be able to claim the balance if we have not
        /// yet received the preimage and claimed it ourselves.
        expiry_height: u32,
    },
    /// The channel has been closed, and our counterparty broadcasted a revoked commitment
    /// transaction.
    ///
    /// Thus, we're able to claim all outputs in the commitment transaction, one of which has the
    /// following amount.
    CounterpartyRevokedOutputClaimable {
        /// The amount, in satoshis, of the output which we can claim.
        ///
        /// Note that for outputs from HTLC balances this may be excluding some on-chain fees that
        /// were already spent.
        claimable_amount_satoshis: u64,
    },
}

impl From<Balance> for ClaimableBalance {
    fn from(balance: Balance) -> Self {
        match balance {
            Balance::ClaimableOnChannelClose {
                claimable_amount_satoshis,
            } => ClaimableBalance::ClaimableOnChannelClose {
                claimable_amount_satoshis,
            },
            Balance::ClaimableAwaitingConfirmations {
                claimable_amount_satoshis,
                confirmation_height,
            } => ClaimableBalance::ClaimableAwaitingConfirmations {
                claimable_amount_satoshis,
                confirmation_height,
            },
            Balance::ContentiousClaimable {
                claimable_amount_satoshis,
                timeout_height,
            } => ClaimableBalance::ContentiousClaimable {
                claimable_amount_satoshis,
                timeout_height,
            },
            Balance::MaybeTimeoutClaimableHTLC {
                claimable_amount_satoshis,
                claimable_height,
            } => ClaimableBalance::MaybeTimeoutClaimableHTLC {
                claimable_amount_satoshis,
                claimable_height,
            },
            Balance::MaybePreimageClaimableHTLC {
                claimable_amount_satoshis,
                expiry_height,
            } => ClaimableBalance::MaybePreimageClaimableHTLC {
                claimable_amount_satoshis,
                expiry_height,
            },
            Balance::CounterpartyRevokedOutputClaimable {
                claimable_amount_satoshis,
            } => ClaimableBalance::CounterpartyRevokedOutputClaimable {
                claimable_amount_satoshis,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json as json;

    #[test]
    fn test_node_address_serialize() {
        let node_address = NodeAddress {
            pubkey: PublicKey::from_str("038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9").unwrap(),
            addr: SocketAddr::new("203.132.94.196".parse().unwrap(), 9735),
        };
        let expected = r#""038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9@203.132.94.196:9735""#;
        let actual = json::to_string(&node_address).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_node_address_deserialize() {
        let node_address =
            r#""038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9@203.132.94.196:9735""#;
        let expected = NodeAddress {
            pubkey: PublicKey::from_str("038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9").unwrap(),
            addr: SocketAddr::new("203.132.94.196".parse().unwrap(), 9735),
        };
        let actual: NodeAddress = json::from_str(node_address).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_public_key_for_rpc_serialize() {
        let public_key_for_rpc = PublicKeyForRPC(
            PublicKey::from_str("038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9").unwrap(),
        );
        let expected = r#""038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9""#;
        let actual = json::to_string(&public_key_for_rpc).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_public_key_for_rpc_deserialize() {
        let public_key_for_rpc = r#""038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9""#;
        let expected = PublicKeyForRPC(
            PublicKey::from_str("038863cf8ab91046230f561cd5b386cbff8309fa02e3f0c3ed161a3aeb64a643b9").unwrap(),
        );
        let actual = json::from_str(public_key_for_rpc).unwrap();
        assert_eq!(expected, actual);
    }
}
