const NFT_LIST_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/0x394d86994f954ed931b86791b62fe64f4c5dac37/nft?chain=POLYGON&format=decimal";
const NFT_HISTORY_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/0x394d86994f954ed931b86791b62fe64f4c5dac37/nft/transfers?chain=POLYGON&format=decimal";
const NFT_METADATA_URL_TEST: &str = "https://moralis-proxy.komodo.earth/api/v2/nft/0xed55e4477b795eaa9bb4bca24df42214e1a05c18/1111777?chain=POLYGON&format=decimal";
const TEST_WALLET_ADDR_EVM: &str = "0x394d86994f954ed931b86791b62fe64f4c5dac37";
const BLOCKLIST_WALLET_ENDPOINT: &str =
    "https://nft.antispam.dragonhound.info/api/blocklist/wallet/eth/0x3eb4b12127EdC81A4d2fD49658db07005bcAd065";
const BLOCKLIST_CONTRACT_SCAN_ENDPOINT: &str = "https://nft.antispam.dragonhound.info/api/blocklist/contract/scan";
const BLOCKLIST_DOMAIN_SCAN_ENDPOINT: &str = "https://nft.antispam.dragonhound.info/api/blocklist/domain/scan";

#[cfg(all(test, not(target_arch = "wasm32")))]
mod native_tests {
    use crate::eth::eth_addr_to_hex;
    use crate::nft::nft_structs::{Chain, MnemonicHQRes, NftFromMoralis, NftTransferHistoryFromMoralis,
                                  PhishingDomainReq, PhishingDomainRes, SpamContractReq, SpamContractRes, UriMeta};
    use crate::nft::nft_tests::{BLOCKLIST_CONTRACT_SCAN_ENDPOINT, BLOCKLIST_DOMAIN_SCAN_ENDPOINT,
                                BLOCKLIST_WALLET_ENDPOINT, NFT_HISTORY_URL_TEST, NFT_LIST_URL_TEST,
                                NFT_METADATA_URL_TEST, TEST_WALLET_ADDR_EVM};
    use crate::nft::storage::db_test_helpers::*;
    use crate::nft::{check_and_redact_if_spam, check_moralis_ipfs_bafy, check_nft_metadata_for_spam,
                     send_post_request_to_uri, send_request_to_uri};
    use common::block_on;
    use ethereum_types::Address;
    use std::str::FromStr;

    #[test]
    fn test_moralis_ipfs_bafy() {
        let uri =
            "https://ipfs.moralis.io:2053/ipfs/bafybeifnek24coy5xj5qabdwh24dlp5omq34nzgvazkfyxgnqms4eidsiq/1.json";
        let res_uri = check_moralis_ipfs_bafy(Some(uri));
        let expected = "https://ipfs.io/ipfs/bafybeifnek24coy5xj5qabdwh24dlp5omq34nzgvazkfyxgnqms4eidsiq/1.json";
        assert_eq!(expected, res_uri.unwrap());
    }

    #[test]
    fn test_invalid_moralis_ipfs_link() {
        let uri = "example.com/bafy?1=ipfs.moralis.io&e=https://";
        let res_uri = check_moralis_ipfs_bafy(Some(uri));
        assert_eq!(uri, res_uri.unwrap());
    }

    #[test]
    fn test_check_for_spam() {
        let mut spam_text = Some("https://arweave.net".to_string());
        assert!(check_and_redact_if_spam(&mut spam_text).unwrap());
        let url_redacted = "URL redacted for user protection";
        assert_eq!(url_redacted, spam_text.unwrap());

        let mut spam_text = Some("ftp://123path ".to_string());
        assert!(check_and_redact_if_spam(&mut spam_text).unwrap());
        let url_redacted = "URL redacted for user protection";
        assert_eq!(url_redacted, spam_text.unwrap());

        let mut spam_text = Some("/192.168.1.1/some.example.org?type=A".to_string());
        assert!(check_and_redact_if_spam(&mut spam_text).unwrap());
        let url_redacted = "URL redacted for user protection";
        assert_eq!(url_redacted, spam_text.unwrap());

        let mut spam_text = Some(r"C:\Users\path\".to_string());
        assert!(check_and_redact_if_spam(&mut spam_text).unwrap());
        let url_redacted = "URL redacted for user protection";
        assert_eq!(url_redacted, spam_text.unwrap());

        let mut valid_text = Some("Hello my name is NFT (The best ever!)".to_string());
        assert!(!check_and_redact_if_spam(&mut valid_text).unwrap());
        assert_eq!("Hello my name is NFT (The best ever!)", valid_text.unwrap());

        let mut nft = nft();
        assert!(check_nft_metadata_for_spam(&mut nft).unwrap());
        let meta_redacted = "{\"name\":\"URL redacted for user protection\",\"image\":\"https://tikimetadata.s3.amazonaws.com/tiki_box.png\"}";
        assert_eq!(meta_redacted, nft.common.metadata.unwrap())
    }

    #[test]
    fn test_moralis_requests() {
        let response_nft_list = block_on(send_request_to_uri(NFT_LIST_URL_TEST)).unwrap();
        let nfts_list = response_nft_list["result"].as_array().unwrap();
        for nft_json in nfts_list {
            let nft_moralis: NftFromMoralis = serde_json::from_str(&nft_json.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, eth_addr_to_hex(&nft_moralis.common.owner_of));
        }

        let response_transfer_history = block_on(send_request_to_uri(NFT_HISTORY_URL_TEST)).unwrap();
        let mut transfer_list = response_transfer_history["result"].as_array().unwrap().clone();
        assert!(!transfer_list.is_empty());
        let first_transfer = transfer_list.remove(transfer_list.len() - 1);
        let transfer_moralis: NftTransferHistoryFromMoralis =
            serde_json::from_str(&first_transfer.to_string()).unwrap();
        assert_eq!(
            TEST_WALLET_ADDR_EVM,
            eth_addr_to_hex(&transfer_moralis.common.to_address)
        );

        let response_meta = block_on(send_request_to_uri(NFT_METADATA_URL_TEST)).unwrap();
        let nft_moralis: NftFromMoralis = serde_json::from_str(&response_meta.to_string()).unwrap();
        assert_eq!(41237364, *nft_moralis.block_number_minted.unwrap());
        let token_uri = nft_moralis.common.token_uri.unwrap();
        let uri_response = block_on(send_request_to_uri(token_uri.as_str())).unwrap();
        serde_json::from_str::<UriMeta>(&uri_response.to_string()).unwrap();
    }

    #[test]
    fn test_antispam_api_requests() {
        let mnemonichq_value = block_on(send_request_to_uri(BLOCKLIST_WALLET_ENDPOINT)).unwrap();
        let mnemonichq_res: MnemonicHQRes = serde_json::from_value(mnemonichq_value).unwrap();
        assert!(mnemonichq_res
            .spam_contracts
            .contains(&Address::from_str("0x0ded8542fc8b2b4e781b96e99fee6406550c9b7c").unwrap()));
        assert_eq!(Chain::Eth, mnemonichq_res.network);

        let req_spam = SpamContractReq {
            network: Chain::Eth,
            addresses: "0x0ded8542fc8b2b4e781b96e99fee6406550c9b7c,0x8d1355b65da254f2cc4611453adfa8b7a13f60ee"
                .to_string(),
        };
        let req_json = serde_json::to_string(&req_spam).unwrap();
        let contract_scan_res = block_on(send_post_request_to_uri(BLOCKLIST_CONTRACT_SCAN_ENDPOINT, req_json)).unwrap();
        let spam_res: SpamContractRes = serde_json::from_slice(&contract_scan_res).unwrap();
        assert!(spam_res
            .result
            .get(&Address::from_str("0x0ded8542fc8b2b4e781b96e99fee6406550c9b7c").unwrap())
            .unwrap());
        assert!(spam_res
            .result
            .get(&Address::from_str("0x8d1355b65da254f2cc4611453adfa8b7a13f60ee").unwrap())
            .unwrap());

        let req_phishing = PhishingDomainReq {
            domains: "disposal-account-case-1f677.web.app,defi8090.vip".to_string(),
        };
        let req_json = serde_json::to_string(&req_phishing).unwrap();
        let domain_scan_res = block_on(send_post_request_to_uri(BLOCKLIST_DOMAIN_SCAN_ENDPOINT, req_json)).unwrap();
        let phishing_res: PhishingDomainRes = serde_json::from_slice(&domain_scan_res).unwrap();
        assert!(phishing_res.result.get("disposal-account-case-1f677.web.app").unwrap());
    }

    #[test]
    fn test_add_get_nfts() { block_on(test_add_get_nfts_impl()) }

    #[test]
    fn test_last_nft_block() { block_on(test_last_nft_block_impl()) }

    #[test]
    fn test_nft_list() { block_on(test_nft_list_impl()) }

    #[test]
    fn test_remove_nft() { block_on(test_remove_nft_impl()) }

    #[test]
    fn test_refresh_metadata() { block_on(test_refresh_metadata_impl()) }

    #[test]
    fn test_nft_amount() { block_on(test_nft_amount_impl()) }

    #[test]
    fn test_nft_spam_by_token_address() { block_on(test_update_nft_spam_by_token_address_impl()) }

    #[test]
    fn test_add_get_transfers() { block_on(test_add_get_transfers_impl()) }

    #[test]
    fn test_last_transfer_block() { block_on(test_last_transfer_block_impl()) }

    #[test]
    fn test_transfer_history() { block_on(test_transfer_history_impl()) }

    #[test]
    fn test_transfer_history_filters() { block_on(test_transfer_history_filters_impl()) }

    #[test]
    fn test_get_update_transfer_meta() { block_on(test_get_update_transfer_meta_impl()) }

    #[test]
    fn test_update_transfer_spam_by_token_address() { block_on(test_update_transfer_spam_by_token_address_impl()) }

    #[test]
    fn test_get_token_addresses() { block_on(test_get_token_addresses_impl()) }
}

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use crate::eth::eth_addr_to_hex;
    use crate::nft::nft_structs::{Chain, MnemonicHQRes, NftFromMoralis, NftTransferHistoryFromMoralis,
                                  PhishingDomainReq, PhishingDomainRes, SpamContractReq, SpamContractRes};
    use crate::nft::nft_tests::{BLOCKLIST_CONTRACT_SCAN_ENDPOINT, BLOCKLIST_DOMAIN_SCAN_ENDPOINT,
                                BLOCKLIST_WALLET_ENDPOINT, NFT_HISTORY_URL_TEST, NFT_LIST_URL_TEST,
                                NFT_METADATA_URL_TEST, TEST_WALLET_ADDR_EVM};
    use crate::nft::storage::db_test_helpers::*;
    use crate::nft::{send_post_request_to_uri, send_request_to_uri};
    use ethereum_types::Address;
    use std::str::FromStr;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn test_moralis_requests() {
        let response_nft_list = send_request_to_uri(NFT_LIST_URL_TEST).await.unwrap();
        let nfts_list = response_nft_list["result"].as_array().unwrap();
        for nft_json in nfts_list {
            let nft_moralis: NftFromMoralis = serde_json::from_str(&nft_json.to_string()).unwrap();
            assert_eq!(TEST_WALLET_ADDR_EVM, eth_addr_to_hex(&nft_moralis.common.owner_of));
        }

        let response_transfer_history = send_request_to_uri(NFT_HISTORY_URL_TEST).await.unwrap();
        let mut transfer_list = response_transfer_history["result"].as_array().unwrap().clone();
        assert!(!transfer_list.is_empty());
        let first_transfer = transfer_list.remove(transfer_list.len() - 1);
        let transfer_moralis: NftTransferHistoryFromMoralis =
            serde_json::from_str(&first_transfer.to_string()).unwrap();
        assert_eq!(
            TEST_WALLET_ADDR_EVM,
            eth_addr_to_hex(&transfer_moralis.common.to_address)
        );

        let response_meta = send_request_to_uri(NFT_METADATA_URL_TEST).await.unwrap();
        let nft_moralis: NftFromMoralis = serde_json::from_str(&response_meta.to_string()).unwrap();
        assert_eq!(41237364, *nft_moralis.block_number_minted.unwrap());
    }

    #[wasm_bindgen_test]
    async fn test_antispam_wallet_endpoint() {
        let res_value = send_request_to_uri(BLOCKLIST_WALLET_ENDPOINT).await.unwrap();
        let mnemonichq_res: MnemonicHQRes = serde_json::from_value(res_value).unwrap();
        assert!(mnemonichq_res
            .spam_contracts
            .contains(&Address::from_str("0x0ded8542fc8b2b4e781b96e99fee6406550c9b7c").unwrap()));
        assert_eq!(Chain::Eth, mnemonichq_res.network);
    }

    #[wasm_bindgen_test]
    async fn test_antispam_scan_endpoints() {
        let req_spam = SpamContractReq {
            network: Chain::Eth,
            addresses: "0x0ded8542fc8b2b4e781b96e99fee6406550c9b7c,0x8d1355b65da254f2cc4611453adfa8b7a13f60ee"
                .to_string(),
        };
        let req_json = serde_json::to_string(&req_spam).unwrap();
        let contract_scan_res = send_post_request_to_uri(BLOCKLIST_CONTRACT_SCAN_ENDPOINT, req_json)
            .await
            .unwrap();
        let spam_res: SpamContractRes = serde_json::from_slice(&contract_scan_res).unwrap();
        assert!(spam_res
            .result
            .get(&Address::from_str("0x0ded8542fc8b2b4e781b96e99fee6406550c9b7c").unwrap())
            .unwrap());
        assert!(spam_res
            .result
            .get(&Address::from_str("0x8d1355b65da254f2cc4611453adfa8b7a13f60ee").unwrap())
            .unwrap());

        let req_phishing = PhishingDomainReq {
            domains: "disposal-account-case-1f677.web.app,defi8090.vip".to_string(),
        };
        let req_json = serde_json::to_string(&req_phishing).unwrap();
        let domain_scan_res = send_post_request_to_uri(BLOCKLIST_DOMAIN_SCAN_ENDPOINT, req_json)
            .await
            .unwrap();
        let phishing_res: PhishingDomainRes = serde_json::from_slice(&domain_scan_res).unwrap();
        assert!(phishing_res.result.get("disposal-account-case-1f677.web.app").unwrap());
    }

    #[wasm_bindgen_test]
    async fn test_add_get_nfts() { test_add_get_nfts_impl().await }

    #[wasm_bindgen_test]
    async fn test_last_nft_block() { test_last_nft_block_impl().await }

    #[wasm_bindgen_test]
    async fn test_nft_list() { test_nft_list_impl().await }

    #[wasm_bindgen_test]
    async fn test_remove_nft() { test_remove_nft_impl().await }

    #[wasm_bindgen_test]
    async fn test_nft_amount() { test_nft_amount_impl().await }

    #[wasm_bindgen_test]
    async fn test_refresh_metadata() { test_refresh_metadata_impl().await }

    #[wasm_bindgen_test]
    async fn test_nft_spam_by_token_address() { test_update_nft_spam_by_token_address_impl().await }

    #[wasm_bindgen_test]
    async fn test_add_get_transfers() { test_add_get_transfers_impl().await }

    #[wasm_bindgen_test]
    async fn test_last_transfer_block() { test_last_transfer_block_impl().await }

    #[wasm_bindgen_test]
    async fn test_transfer_history() { test_transfer_history_impl().await }

    #[wasm_bindgen_test]
    async fn test_transfer_history_filters() { test_transfer_history_filters_impl().await }

    #[wasm_bindgen_test]
    async fn test_get_update_transfer_meta() { test_get_update_transfer_meta_impl().await }

    #[wasm_bindgen_test]
    async fn test_update_transfer_spam_by_token_address() { test_update_transfer_spam_by_token_address_impl().await }

    #[wasm_bindgen_test]
    async fn test_get_token_addresses() { test_get_token_addresses_impl().await }
}
