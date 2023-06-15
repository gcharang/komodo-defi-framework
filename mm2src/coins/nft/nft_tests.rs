const NFT_LIST_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/0x394d86994f954ed931b86791b62fe64f4c5dac37/nft?chain=POLYGON&format=decimal";
const NFT_HISTORY_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/0x394d86994f954ed931b86791b62fe64f4c5dac37/nft/transfers?chain=POLYGON&format=decimal&direction=both";
const NFT_METADATA_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/nft/0xed55e4477b795eaa9bb4bca24df42214e1a05c18/1111777?chain=POLYGON&format=decimal";
const TEST_WALLET_ADDR_EVM: &str = "0x394d86994f954ed931b86791b62fe64f4c5dac37";

#[cfg(any(test, target_arch = "wasm32"))]
mod for_db_tests {
    use crate::nft::nft_structs::{Chain, ContractType, Nft, NftTransferHistory, TransferStatus, UriMeta};
    use crate::nft::storage::{NftListStorageOps, NftStorageBuilder};
    use mm2_number::BigDecimal;
    use mm2_test_helpers::for_tests::mm_ctx_with_custom_db;
    use std::str::FromStr;

    cfg_wasm32! {
        use wasm_bindgen_test::*;

        wasm_bindgen_test_configure!(run_in_browser);
    }

    fn nft_list() -> Vec<Nft> {
        let nft = Nft {
            chain: Chain::Bsc,
            token_address: "0x5c7d6712dfaf0cb079d48981781c8705e8417ca0".to_string(),
            token_id: Default::default(),
            amount: BigDecimal::from_str("1").unwrap(),
            owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
            token_hash: "b34ddf294013d20a6d70691027625839".to_string(),
            block_number_minted: 25465916,
            block_number: 25919780,
            contract_type: ContractType::Erc1155,
            collection_name: None,
            symbol: None,
            token_uri: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.json".to_string()),
            metadata: Some("{\"name\":\"Tiki box\"}".to_string()),
            last_token_uri_sync: Some("2023-02-07T17:10:08.402Z".to_string()),
            last_metadata_sync: Some("2023-02-07T17:10:16.858Z".to_string()),
            minter_address: Some("ERC1155 tokens don't have a single minter".to_string()),
            possible_spam: Some(false),
            uri_meta: UriMeta {
                image: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.png".to_string()),
                token_name: None,
                description: Some("Born to usher in Bull markets.".to_string()),
                attributes: None,
                animation_url: None,
            },
        };

        let nft1 = Nft {
            chain: Chain::Bsc,
            token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
            token_id: BigDecimal::from_str("214300047252").unwrap(),
            amount: BigDecimal::from_str("1").unwrap(),
            owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
            token_hash: "c5d1cfd75a0535b0ec750c0156e6ddfe".to_string(),
            block_number_minted: 25721963,
            block_number: 28056726,
            contract_type: ContractType::Erc721,
            collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
            symbol: Some("BMBBBF".to_string()),
            token_uri: Some("https://public.nftstatic.com/static/nft/BSC/BMBBBF/214300047252".to_string()),
            metadata: Some(
                "{\"image\":\"https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png\"}"
                    .to_string(),
            ),
            last_token_uri_sync: Some("2023-02-16T16:35:52.392Z".to_string()),
            last_metadata_sync: Some("2023-02-16T16:36:04.283Z".to_string()),
            minter_address: Some("0xdbdeb0895f3681b87fb3654b5cf3e05546ba24a9".to_string()),
            possible_spam: Some(false),
            uri_meta: UriMeta {
                image: Some(
                    "https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string(),
                ),
                token_name: Some("Nebula Nodes".to_string()),
                description: Some("Interchain nodes".to_string()),
                attributes: None,
                animation_url: None,
            },
        };

        let nft2 = Nft {
            chain: Chain::Bsc,
            token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
            token_id: BigDecimal::from_str("214300044414").unwrap(),
            amount: BigDecimal::from_str("1").unwrap(),
            owner_of: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
            token_hash: "125f8f4e952e107c257960000b4b250e".to_string(),
            block_number_minted: 25810308,
            block_number: 28056721,
            contract_type: ContractType::Erc721,
            collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
            symbol: Some("BMBBBF".to_string()),
            token_uri: Some("https://public.nftstatic.com/static/nft/BSC/BMBBBF/214300044414".to_string()),
            metadata: Some(
                "{\"image\":\"https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png\"}"
                    .to_string(),
            ),
            last_token_uri_sync: Some("2023-02-19T19:12:09.471Z".to_string()),
            last_metadata_sync: Some("2023-02-19T19:12:18.080Z".to_string()),
            minter_address: Some("0xdbdeb0895f3681b87fb3654b5cf3e05546ba24a9".to_string()),
            possible_spam: Some(false),
            uri_meta: UriMeta {
                image: Some(
                    "https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string(),
                ),
                token_name: Some("Nebula Nodes".to_string()),
                description: Some("Interchain nodes".to_string()),
                attributes: None,
                animation_url: None,
            },
        };
        vec![nft, nft1, nft2]
    }

    #[allow(dead_code)]
    fn nft_tx_historty() -> Vec<NftTransferHistory> {
        let tx = NftTransferHistory {
            chain: Chain::Bsc,
            block_number: 25919780,
            block_timestamp: 1677166110,
            block_hash: "0xcb41654fc5cf2bf5d7fd3f061693405c74d419def80993caded0551ecfaeaae5".to_string(),
            transaction_hash: "0x9c16b962f63eead1c5d2355cc9037dde178b14b53043c57eb40c27964d22ae6a".to_string(),
            transaction_index: 57,
            log_index: 139,
            value: Default::default(),
            contract_type: ContractType::Erc1155,
            transaction_type: "Single".to_string(),
            token_address: "0x5c7d6712dfaf0cb079d48981781c8705e8417ca0".to_string(),
            token_id: Default::default(),
            collection_name: None,
            image: Some("https://tikimetadata.s3.amazonaws.com/tiki_box.png".to_string()),
            token_name: None,
            from_address: "0x4ff0bbc9b64d635a4696d1a38554fb2529c103ff".to_string(),
            to_address: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
            status: TransferStatus::Receive,
            amount: BigDecimal::from_str("1").unwrap(),
            verified: 1,
            operator: Some("0x4ff0bbc9b64d635a4696d1a38554fb2529c103ff".to_string()),
            possible_spam: Some(false),
        };

        let tx1 = NftTransferHistory {
            chain: Chain::Bsc,
            block_number: 28056726,
            block_timestamp: 1683627432,
            block_hash: "0x3d68b78391fb3cf8570df27036214f7e9a5a6a45d309197936f51d826041bfe7".to_string(),
            transaction_hash: "0x1e9f04e9b571b283bde02c98c2a97da39b2bb665b57c1f2b0b733f9b681debbe".to_string(),
            transaction_index: 198,
            log_index: 495,
            value: Default::default(),
            contract_type: ContractType::Erc721,
            transaction_type: "Single".to_string(),
            token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
            token_id: BigDecimal::from_str("214300047252").unwrap(),
            collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
            image: Some("https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string()),
            token_name: Some("Nebula Nodes".to_string()),
            from_address: "0x6fad0ec6bb76914b2a2a800686acc22970645820".to_string(),
            to_address: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
            status: TransferStatus::Receive,
            amount: BigDecimal::from_str("1").unwrap(),
            verified: 1,
            operator: None,
            possible_spam: Some(false),
        };

        let tx2 = NftTransferHistory {
            chain: Chain::Bsc,
            block_number: 28056721,
            block_timestamp: 1683627417,
            block_hash: "0x326db41c5a4fd5f033676d95c590ced18936ef2ef6079e873b23af087fd966c6".to_string(),
            transaction_hash: "0x981bad702cc6e088f0e9b5e7287ff7a3487b8d269103cee3b9e5803141f63f91".to_string(),
            transaction_index: 83,
            log_index: 201,
            value: Default::default(),
            contract_type: ContractType::Erc721,
            transaction_type: "Single".to_string(),
            token_address: "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string(),
            token_id: BigDecimal::from_str("214300044414").unwrap(),
            collection_name: Some("Binance NFT Mystery Box-Back to Blockchain Future".to_string()),
            image: Some("https://public.nftstatic.com/static/nft/res/4df0a5da04174e1e9be04b22a805f605.png".to_string()),
            token_name: Some("Nebula Nodes".to_string()),
            from_address: "0x6fad0ec6bb76914b2a2a800686acc22970645820".to_string(),
            to_address: "0xf622a6c52c94b500542e2ae6bcad24c53bc5b6a2".to_string(),
            status: TransferStatus::Receive,
            amount: BigDecimal::from_str("1").unwrap(),
            verified: 1,
            operator: None,
            possible_spam: Some(false),
        };
        vec![tx, tx1, tx2]
    }

    pub(crate) async fn test_add_nfts_impl() {
        let ctx = mm_ctx_with_custom_db();
        let storage = NftStorageBuilder::new(&ctx).build().unwrap();
        let chain = Chain::Bsc;
        NftListStorageOps::init(&storage, &chain).await.unwrap();
        let is_initialized = NftListStorageOps::is_initialized(&storage, &chain).await.unwrap();
        assert!(is_initialized);
        let scanned_block = 28056726;
        let nft_list = nft_list();
        storage.add_nfts_to_list(&chain, nft_list, scanned_block).await.unwrap();
        let token_add = "0xfd913a305d70a60aac4faac70c739563738e1f81".to_string();
        let token_id = BigDecimal::from_str("214300044414").unwrap();
        let nft = storage.get_nft(&chain, token_add, token_id).await.unwrap().unwrap();
        assert_eq!(nft.block_number, 28056721);
        let last_scanned_block = storage.get_last_scanned_block(&chain).await.unwrap().unwrap();
        let last_block = storage.get_last_block_number(&chain).await.unwrap().unwrap();
        assert_eq!(last_block, last_scanned_block);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use crate::nft::nft_structs::{NftTransferHistoryWrapper, NftWrapper, UriMeta};
    use crate::nft::nft_tests::for_db_tests::*;
    use crate::nft::nft_tests::{NFT_HISTORY_URL_TEST, NFT_LIST_URL_TEST, NFT_METADATA_URL_TEST, TEST_WALLET_ADDR_EVM};
    use crate::nft::send_request_to_uri;
    use common::block_on;

    #[test]
    fn test_moralis_nft_list() {
        let response = block_on(send_request_to_uri(NFT_LIST_URL_TEST)).unwrap();
        let nfts_list = response["result"].as_array().unwrap();
        for nft_json in nfts_list {
            let nft_wrapper: NftWrapper = serde_json::from_str(&nft_json.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, nft_wrapper.owner_of);
        }
    }

    #[test]
    fn test_moralis_nft_transfer_history() {
        let response = block_on(send_request_to_uri(NFT_HISTORY_URL_TEST)).unwrap();
        let mut transfer_list = response["result"].as_array().unwrap().clone();
        assert!(!transfer_list.is_empty());
        let first_tx = transfer_list.remove(transfer_list.len() - 1);
        let transfer_wrapper: NftTransferHistoryWrapper = serde_json::from_str(&first_tx.to_string()).unwrap();
        assert_eq!(TEST_WALLET_ADDR_EVM, transfer_wrapper.to_address);
    }

    #[test]
    fn test_moralis_nft_metadata() {
        let response = block_on(send_request_to_uri(NFT_METADATA_URL_TEST)).unwrap();
        let nft_wrapper: NftWrapper = serde_json::from_str(&response.to_string()).unwrap();
        assert_eq!(41237364, *nft_wrapper.block_number_minted);
        let token_uri = nft_wrapper.token_uri.unwrap();
        let uri_response = block_on(send_request_to_uri(token_uri.as_str())).unwrap();
        serde_json::from_str::<UriMeta>(&uri_response.to_string()).unwrap();
    }

    #[test]
    fn test_add_nfts() { block_on(test_add_nfts_impl()) }
}

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use crate::nft::nft_structs::{NftTransferHistoryWrapper, NftWrapper};
    use crate::nft::nft_tests::for_db_tests::*;
    use crate::nft::nft_tests::{NFT_HISTORY_URL_TEST, NFT_LIST_URL_TEST, NFT_METADATA_URL_TEST, TEST_WALLET_ADDR_EVM};
    use crate::nft::send_request_to_uri;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_moralis_nft_list() {
        let response = send_request_to_uri(NFT_LIST_URL_TEST).await.unwrap();
        let nfts_list = response["result"].as_array().unwrap();
        for nft_json in nfts_list {
            let nft_wrapper: NftWrapper = serde_json::from_str(&nft_json.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, nft_wrapper.owner_of);
        }
    }

    #[wasm_bindgen_test]
    async fn test_moralis_nft_transfer_history() {
        let response = send_request_to_uri(NFT_HISTORY_URL_TEST).await.unwrap();
        let mut transfer_list = response["result"].as_array().unwrap().clone();
        assert!(!transfer_list.is_empty());
        let first_tx = transfer_list.remove(transfer_list.len() - 1);
        let transfer_wrapper: NftTransferHistoryWrapper = serde_json::from_str(&first_tx.to_string()).unwrap();
        assert_eq!(TEST_WALLET_ADDR_EVM, transfer_wrapper.to_address);
    }

    #[wasm_bindgen_test]
    async fn test_moralis_nft_metadata() {
        let response = send_request_to_uri(NFT_METADATA_URL_TEST).await.unwrap();
        let nft_wrapper: NftWrapper = serde_json::from_str(&response.to_string()).unwrap();
        assert_eq!(41237364, *nft_wrapper.block_number_minted);
    }

    #[wasm_bindgen_test]
    async fn test_add_nfts() { test_add_nfts_impl().await }
}
