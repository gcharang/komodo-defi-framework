use crypto::{decrypt_mnemonic, encrypt_mnemonic, generate_mnemonic, CryptoCtx, CryptoInitError, MnemonicError};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use serde::de::DeserializeOwned;
use serde_json::{self as json};

#[cfg(not(target_arch = "wasm32"))]
#[path = "lp_wallet/mnemonics_storage.rs"]
mod mnemonics_storage;
#[cfg(not(target_arch = "wasm32"))]
use mnemonics_storage::{read_encrypted_passphrase, save_encrypted_passphrase};
#[cfg(target_arch = "wasm32")]
#[path = "lp_wallet/mnemonics_wasm_db.rs"]
mod mnemonics_wasm_db;
#[cfg(target_arch = "wasm32")]
use mnemonics_wasm_db::{read_encrypted_passphrase, save_encrypted_passphrase};

type WalletInitResult<T> = Result<T, MmError<WalletInitError>>;

#[derive(Debug, Deserialize, Display, Serialize)]
pub enum WalletInitError {
    #[display(fmt = "Error deserializing '{}' config field: {}", field, error)]
    ErrorDeserializingConfig {
        field: String,
        error: String,
    },
    #[display(fmt = "Wallets storage error: {}", _0)]
    WalletsStorageError(String),
    #[display(
        fmt = "Passphrase doesn't match the one from file, please create a new wallet if you want to use a new passphrase"
    )]
    PassphraseMismatch,
    #[display(fmt = "Error generating mnemonic: {}", _0)]
    GenerateMnemonicError(String),
    #[display(fmt = "Error initializing crypto context: {}", _0)]
    CryptoInitError(String),
    Internal(String),
}

impl From<MnemonicError> for WalletInitError {
    fn from(e: MnemonicError) -> Self { WalletInitError::GenerateMnemonicError(e.to_string()) }
}

impl From<CryptoInitError> for WalletInitError {
    fn from(e: CryptoInitError) -> Self { WalletInitError::CryptoInitError(e.to_string()) }
}

// Utility function for deserialization to reduce repetition
fn deserialize_config_field<T: DeserializeOwned>(ctx: &MmArc, field: &str) -> WalletInitResult<T> {
    json::from_value::<T>(ctx.conf[field].clone()).map_to_mm(|e| WalletInitError::ErrorDeserializingConfig {
        field: field.to_owned(),
        error: e.to_string(),
    })
}

// Utility function to handle passphrase encryption and saving
async fn encrypt_and_save_passphrase(
    ctx: &MmArc,
    wallet_name: &str,
    passphrase: &str,
    wallet_password: &str,
) -> WalletInitResult<()> {
    let encrypted_passphrase_data = encrypt_mnemonic(passphrase, wallet_password)?;
    save_encrypted_passphrase(ctx, wallet_name, &encrypted_passphrase_data)
        .await
        .mm_err(|e| WalletInitError::WalletsStorageError(e.to_string()))
}

/// Reads and decrypts the passphrase from a file associated with the given wallet name.
///
/// This function first reads the passphrase from the file. Since the passphrase is stored in an encrypted
/// format, it decrypts it before returning.
///
/// # Arguments
/// * `ctx` - The `MmArc` context containing the application state and configuration.
/// * `wallet_name` - The name of the wallet for which the passphrase is to be retrieved.
///
/// # Returns
/// `MmInitResult<String>` - The decrypted passphrase or an error if any operation fails.
///
/// # Errors
/// Returns specific `MmInitError` variants for different failure scenarios.
async fn read_and_decrypt_passphrase(
    ctx: &MmArc,
    wallet_name: &str,
    wallet_password: &str,
) -> WalletInitResult<Option<String>> {
    match read_encrypted_passphrase(ctx, wallet_name)
        .await
        .mm_err(|e| WalletInitError::WalletsStorageError(e.to_string()))?
    {
        Some(encrypted_passphrase) => {
            let mnemonic = decrypt_mnemonic(&encrypted_passphrase, wallet_password)?;
            Ok(Some(mnemonic.to_string()))
        },
        None => Ok(None),
    }
}

/// Initializes and manages the wallet passphrase.
///
/// This function handles several scenarios based on the configuration:
/// - Deserializes the passphrase and wallet name from the configuration.
/// - If both wallet name and passphrase are `None`, the function sets up the context for "no login mode"
///   This mode can be entered after the function's execution, allowing access to Komodo DeFi Framework
///   functionalities that don't require a passphrase (e.g., viewing the orderbook).
/// - If a wallet name is provided without a passphrase, it first checks for the existence of a
///   passphrase file associated with the wallet. If no file is found, it generates a new passphrase,
///   encrypts it, and saves it, enabling multi-wallet support.
/// - If a passphrase is provided (with or without a wallet name), it uses the provided passphrase
///   and handles encryption and storage as needed.
/// - Initializes the cryptographic context based on the `enable_hd` configuration.
///
/// # Arguments
/// * `ctx` - The `MmArc` context containing the application state and configuration.
///
/// # Returns
/// `MmInitResult<()>` - Result indicating success or failure of the initialization process.
///
/// # Errors
/// Returns `MmInitError` if deserialization fails or if there are issues in passphrase handling.
///
pub(crate) async fn initialize_wallet_passphrase(ctx: MmArc) -> WalletInitResult<()> {
    let passphrase = deserialize_config_field::<Option<String>>(&ctx, "passphrase")?;
    // New approach for passphrase, `wallet_name` is needed in the config to enable multi-wallet support.
    // In this case the passphrase will be generated if not provided.
    // The passphrase will then be encrypted and saved whether it was generated or provided.
    let wallet_name = deserialize_config_field::<Option<String>>(&ctx, "wallet_name")?;

    let passphrase = match (wallet_name, passphrase) {
        (None, None) => None,
        // Legacy approach for passphrase, no `wallet_name` is needed in the config, in this case the passphrase is not encrypted and saved.
        (None, Some(passphrase)) => Some(passphrase),
        // New mode, passphrase is not provided. Generate, encrypt and save passphrase.
        // passphrase is provided. encrypt and save passphrase.
        (Some(wallet_name), maybe_passphrase) => {
            let wallet_password = deserialize_config_field::<String>(&ctx, "wallet_password")?;
            match read_and_decrypt_passphrase(&ctx, &wallet_name, &wallet_password).await? {
                Some(passphrase_from_file) => match maybe_passphrase {
                    Some(passphrase) if passphrase == passphrase_from_file => Some(passphrase),
                    None => Some(passphrase_from_file),
                    _ => return MmError::err(WalletInitError::PassphraseMismatch),
                },
                None => {
                    let new_passphrase = match maybe_passphrase {
                        Some(passphrase) => passphrase,
                        None => generate_mnemonic(&ctx)?.to_string(),
                    };
                    encrypt_and_save_passphrase(&ctx, &wallet_name, &new_passphrase, &wallet_password).await?;
                    Some(new_passphrase)
                },
            }
        },
    };

    if let Some(passphrase) = passphrase {
        // This defaults to false to maintain backward compatibility.
        match ctx.conf["enable_hd"].as_bool().unwrap_or(false) {
            true => CryptoCtx::init_with_global_hd_account(ctx.clone(), &passphrase)?,
            false => CryptoCtx::init_with_iguana_passphrase(ctx.clone(), &passphrase)?,
        };
    }

    Ok(())
}
