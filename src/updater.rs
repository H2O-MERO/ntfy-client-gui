use reqwest::header::USER_AGENT;
use semver::Version;
use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_notes: Option<String>,
    pub release_page_url: Option<String>,
    pub asset_download_url: Option<String>,
    pub asset_size: u64,
    pub asset_name: Option<String>,
    pub error: Option<String>,
}

impl UpdateCheckResult {
    pub fn update_available(&self) -> bool {
        self.update_available
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: Option<String>,
    browser_download_url: Option<String>,
    size: Option<u64>,
}

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/H2O-MERO/ntfy-pusher-Windows/releases/latest";

pub async fn check_for_updates() -> UpdateCheckResult {
    let mut result = UpdateCheckResult {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            result.error = Some(err.to_string());
            return result;
        }
    };

    let response = match client
        .get(LATEST_RELEASE_URL)
        .header(USER_AGENT, "ntfy-client-gui-update-checker")
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            result.error = Some(err.to_string());
            return result;
        }
    };

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        result.error = Some("仓库还没有 Release".to_string());
        return result;
    }
    if !response.status().is_success() {
        result.error = Some(format!("HTTP {}", response.status()));
        return result;
    }

    let release: GitHubRelease = match response.json().await {
        Ok(release) => release,
        Err(err) => {
            result.error = Some(err.to_string());
            return result;
        }
    };

    let Some(latest) = parse_version(&release.tag_name) else {
        result.error = Some(format!("无法解析版本号: {}", release.tag_name));
        return result;
    };
    let Some(current) = parse_version(&result.current_version) else {
        result.error = Some(format!("无法解析当前版本号: {}", result.current_version));
        return result;
    };

    result.latest_version = Some(format_version(&latest));
    result.release_notes = release.body.clone();
    result.release_page_url = release.html_url.clone();

    result.update_available = latest > current;
    if result.update_available {
        // 与原版一致：优先 zip，其次 exe。
        let asset = release
            .assets
            .iter()
            .find(|a| {
                a.name
                    .as_deref()
                    .map(|n| n.to_lowercase().ends_with(".zip"))
                    .unwrap_or(false)
            })
            .or_else(|| {
                release.assets.iter().find(|a| {
                    a.name
                        .as_deref()
                        .map(|n| n.to_lowercase().ends_with(".exe"))
                        .unwrap_or(false)
                })
            });

        if let Some(asset) = asset {
            if let Some(url) = asset.browser_download_url.clone() {
                result.asset_download_url = Some(url);
                result.asset_size = asset.size.unwrap_or(0);
                result.asset_name = asset.name.clone();
            }
        }
    }

    result
}

fn parse_version(tag: &str) -> Option<Version> {
    let cleaned = tag.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    Version::parse(cleaned)
        .ok()
        .or_else(|| Version::parse(&format!("{}.0", cleaned)).ok())
}

fn format_version(version: &Version) -> String {
    version.to_string()
}
