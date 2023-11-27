use anyhow::Result;
use common::log::{error, info};
use serde::Deserialize;
use std::fs::File;
use std::io::Write;
use zip::ZipArchive;

const BINARY_HOST_URL: &str = "https://api.github.com/repos/KomodoPlatform/komodo-defi-framework/releases";

#[cfg(not(target_os = "macos"))]
const DOWNLOAD_NAME: &str = "Linux-Release.";
#[cfg(target_os = "macos")]
const DOWNLOAD_NAME: &str = "Darwin-Release.";
#[cfg(target_os = "windows")]
const DOWNLOAD_NAME: &str = "Win64.";

#[cfg(not(target_os = "windows"))]
const BINARY_NAME: &str = "mm2";
#[cfg(target_os = "windows")]
const BINARY_NAME: &str = "mm2.exe";

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct Releases {
    assets: Vec<Asset>,
}

/// Let komodefi_cli download and extract release binary for use.
pub(crate) async fn download_binary_and_extract_to_bin_folder() -> Result<()> {
    // Create a reusable reqwest client with common headers
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(reqwest::header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7".parse().unwrap());
            headers
        })
        .build()?;
    let response = client.get(BINARY_HOST_URL).send().await?;
    let releases: Vec<Releases> = response.json().await?;

    // Extract the download URL for the latest release
    if let Some(download_url) = releases.get(0).and_then(|release| {
        release
            .assets
            .iter()
            .find(|asset| asset.name.contains(DOWNLOAD_NAME))
            .map(|asset| &asset.browser_download_url)
    }) {
        // Download the ZIP file
        let zip_data = client.get(download_url).send().await?.bytes().await?;
        // Create directories if they don't exist
        let bin_dir = std::env::current_dir()?.join("bin");
        if !bin_dir.exists() {
            std::fs::create_dir_all(&bin_dir)?;
        }
        // Save the ZIP file
        let zip_path = bin_dir.join("downloaded_file.zip");
        let mut zip_file = File::create(&zip_path)?;
        zip_file.write_all(&zip_data)?;
        // Extract only mm2 binary file from the folder
        extract_file_from_zip(&zip_path, BINARY_NAME)?;
        info!("Binary downloaded and extracted to the bin folder!");
        Ok(())
    } else {
        error!("No matching release found");
        Err(anyhow::anyhow!("No matching release found"))
    }
}

/// Extract binary file from zip file
fn extract_file_from_zip(zip_path: &std::path::Path, file_name: &str) -> Result<(), anyhow::Error> {
    let file = File::open(zip_path)?;
    let reader = std::io::BufReader::new(file);

    let mut archive = ZipArchive::new(reader)?;

    // Find the index of the desired file in the archive
    let file_index = archive
        .file_names()
        .position(|name| name == file_name)
        .ok_or_else(|| anyhow::anyhow!("File not found in the ZIP archive"))?;

    let mut file = archive.by_index(file_index)?;

    // Create directories if they don't exist
    let bin_dir = std::env::current_dir()?.join("bin");
    if !bin_dir.exists() {
        std::fs::create_dir_all(&bin_dir)?;
    }

    let file_path = bin_dir.join(file_name);
    let mut extracted_file = File::create(&file_path)?;

    std::io::copy(&mut file, &mut extracted_file)?;

    Ok(())
}
