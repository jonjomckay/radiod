use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WidgetResponse {
    #[serde(rename = "streamURL")]
    stream_url: String,
    #[serde(rename = "streamType")]
    stream_type: i32,
    #[serde(rename = "isGeoBlocked")]
    is_geo_blocked: bool,
    #[serde(rename = "isRestricted")]
    is_restricted: bool,
}

pub async fn resolve_stream_url(country: &str, alias: &str) -> anyhow::Result<String> {
    let url = format!(
        "https://onlineradiobox.com/json/{}/{}/widget/",
        country, alias
    );

    let response = reqwest::get(&url)
        .await
        .context("failed to fetch stream URL from Online Radio Box")?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!(
            "Online Radio Box API returned {} for station {}/{}",
            status,
            country,
            alias
        );
    }

    let widget: WidgetResponse = response
        .json()
        .await
        .context("failed to parse widget JSON response")?;

    if widget.is_geo_blocked {
        anyhow::bail!("station {}/{} is geo-blocked", country, alias);
    }

    if widget.stream_url.is_empty() {
        anyhow::bail!("empty stream URL for station {}/{}", country, alias);
    }

    Ok(widget.stream_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_widget_response() {
        let json = r#"{"streamURL":"https://example.com/stream","streamType":6,"isGeoBlocked":false,"isRestricted":false}"#;
        let widget: WidgetResponse = serde_json::from_str(json).unwrap();
        assert_eq!(widget.stream_url, "https://example.com/stream");
        assert_eq!(widget.stream_type, 6);
        assert!(!widget.is_geo_blocked);
        assert!(!widget.is_restricted);
    }

    #[test]
    fn deserialize_geo_blocked_response() {
        let json = r#"{"streamURL":"","streamType":0,"isGeoBlocked":true,"isRestricted":false}"#;
        let widget: WidgetResponse = serde_json::from_str(json).unwrap();
        assert!(widget.is_geo_blocked);
        assert!(widget.stream_url.is_empty());
    }
}
