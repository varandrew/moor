use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/varandrew/moor/releases/latest";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
}

fn compare_release(current: &str, release: Release) -> Result<UpdateCheck, String> {
    if release.draft || release.prerelease {
        return Err("No stable release is available".into());
    }
    let latest = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| "Invalid release version".to_string())?;
    if !latest.pre.is_empty() {
        return Err("No stable release is available".into());
    }
    let installed = Version::parse(current).map_err(|_| "Invalid current version".to_string())?;
    let mut url = reqwest::Url::parse("https://github.com/varandrew/moor/releases/tag/").unwrap();
    url.path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .push(&release.tag_name);
    Ok(UpdateCheck {
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        update_available: latest.cmp_precedence(&installed).is_gt(),
        release_url: url.to_string(),
    })
}

pub async fn check_update(current: &str) -> Result<UpdateCheck, String> {
    check_update_at(current, RELEASE_API).await
}

async fn check_update_at(current: &str, endpoint: &str) -> Result<UpdateCheck, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("Moor/", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .map_err(|error| format!("Update check failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Update check failed: HTTP {}", response.status()));
    }
    let release = response
        .json::<Release>()
        .await
        .map_err(|_| "Invalid release response".to_string())?;
    compare_release(current, release)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn release_comparison_never_suggests_a_downgrade() {
        for (current, latest, available) in [
            ("0.8.1", "v0.8.2", true),
            ("0.8.1", "v0.8.1", false),
            ("0.9.0", "v0.8.1", false),
            ("0.8.2-beta.1", "v0.8.2", true),
            ("0.9.0-beta.1", "v0.8.2", false),
            ("0.8.1+build.1", "v0.8.1+build.2", false),
        ] {
            let result = compare_release(current, release(latest)).unwrap();
            assert_eq!(result.update_available, available);
            assert!(result
                .release_url
                .starts_with("https://github.com/varandrew/moor/releases/tag/"));
        }
        assert!(compare_release("0.8.1", release("not-a-version")).is_err());
        assert!(compare_release("0.8.1", release("v0.9.0-beta.1")).is_err());
        let mut beta = release("v0.9.0-beta.1");
        beta.prerelease = true;
        assert!(compare_release("0.8.1", beta).is_err());
    }

    #[tokio::test]
    async fn offline_and_rate_limited_checks_are_errors() {
        use axum::{routing::get, Router};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route(
                        "/",
                        get(|| async { axum::http::StatusCode::TOO_MANY_REQUESTS }),
                    )
                    .route(
                        "/missing",
                        get(|| async { axum::http::StatusCode::NOT_FOUND }),
                    )
                    .route("/invalid", get(|| async { "invalid json" })),
            )
            .await
            .unwrap();
        });
        assert!(check_update_at("0.8.1", &format!("http://{address}/"))
            .await
            .is_err());
        assert!(
            check_update_at("0.8.1", &format!("http://{address}/missing"))
                .await
                .is_err()
        );
        assert!(
            check_update_at("0.8.1", &format!("http://{address}/invalid"))
                .await
                .is_err()
        );
        task.abort();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert!(check_update_at("0.8.1", &format!("http://{address}/"))
            .await
            .is_err());
    }
}
