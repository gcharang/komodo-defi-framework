///*
/// Ethereum definitions
/// @embed
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EthereumDefinitions {
    /// encoded Ethereum network
    #[prost(bytes = "vec", optional, tag = "1")]
    pub encoded_network: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
    /// encoded Ethereum token
    #[prost(bytes = "vec", optional, tag = "2")]
    pub encoded_token: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
}
