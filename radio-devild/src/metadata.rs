use anyhow::Context;
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct Metadata {
    pub track_id: String,
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub art_url: String,
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ScraperResponse {
    title: Option<String>,
    #[serde(rename = "iArtist")]
    i_artist: Option<String>,
    #[serde(rename = "iName")]
    i_name: Option<String>,
    #[serde(rename = "iImg")]
    i_img: Option<String>,
    #[serde(rename = "trackId")]
    track_id: Option<String>,
}

fn parse_metadata(response: ScraperResponse) -> Metadata {
    let art_url = response.i_img.unwrap_or_default();
    let track_id = response.track_id.unwrap_or_default();

    let (artist, title) = match (response.i_artist, response.i_name) {
        (Some(artist), Some(name)) if !artist.is_empty() && !name.is_empty() => (artist, name),
        _ => {
            let raw_title = response.title.unwrap_or_default();
            if let Some((artist_part, title_part)) = raw_title.split_once(" - ") {
                (artist_part.to_string(), title_part.to_string())
            } else {
                (String::new(), raw_title)
            }
        }
    };

    Metadata {
        track_id,
        artist,
        title,
        album: None,
        art_url,
        duration_secs: None,
    }
}

pub async fn fetch_metadata(country: &str, alias: &str) -> anyhow::Result<Metadata> {
    let url = format!("http://scraper.onlineradiobox.com/{}.{}", country, alias);
    let response = reqwest::get(&url)
        .await
        .context("failed to fetch now-playing metadata")?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("scraper returned {} for {}.{}", status, country, alias);
    }

    let scraper: ScraperResponse = response
        .json()
        .await
        .context("failed to parse scraper JSON response")?;

    Ok(parse_metadata(scraper))
}

pub fn spawn_metadata_poller(
    country: String,
    alias: String,
    poll_interval: Duration,
    tx: watch::Sender<Metadata>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match fetch_metadata(&country, &alias).await {
                Ok(metadata) => {
                    tracing::debug!(
                        "now playing on {}/{}: {} - {}",
                        country,
                        alias,
                        metadata.artist,
                        metadata.title
                    );
                    let _ = tx.send(metadata);
                }
                Err(e) => {
                    let status_str = e.to_string();
                    let is_permanent = status_str.contains("404")
                        || status_str.contains("410")
                        || status_str.contains("geo-blocked");

                    if is_permanent {
                        tracing::error!(
                            "metadata fetch permanently failed for {}/{}: {}",
                            country,
                            alias,
                            e
                        );
                        let _ = tx.send(Metadata::default());
                    } else {
                        tracing::warn!(
                            "metadata fetch failed (will retry) for {}/{}: {}",
                            country,
                            alias,
                            e
                        );
                    }
                }
            }
            tokio::time::sleep(poll_interval).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scraper_with_structured() -> ScraperResponse {
        serde_json::from_str(
            r#"{"alias":"uk.bbcradio1","stationId":1193,"updated":1783067791,"trackId":"1369276698548314001","title":"Aitch - Rain (feat. Tay Keith)","citatisId":40608,"iName":"Rain (feat. Tay Keith)","iArtist":"AJ Tracey","iImg":"https://example.com/art.jpg"}"#,
        )
        .unwrap()
    }

    fn scraper_title_only(title: &str) -> ScraperResponse {
        serde_json::from_str(&format!(
            r#"{{"alias":"test","stationId":1,"updated":1,"trackId":"123","title":"{}","iName":"","iArtist":"","iImg":"https://example.com/img.jpg"}}"#,
            title
        ))
        .unwrap()
    }

    #[test]
    fn parse_structured_artist_and_name() {
        let result = parse_metadata(scraper_with_structured());
        assert_eq!(result.artist, "AJ Tracey");
        assert_eq!(result.title, "Rain (feat. Tay Keith)");
        assert_eq!(result.track_id, "1369276698548314001");
        assert_eq!(result.art_url, "https://example.com/art.jpg");
        assert!(result.album.is_none());
        assert!(result.duration_secs.is_none());
    }

    #[test]
    fn parse_fallback_split_on_dash() {
        let result = parse_metadata(scraper_title_only("Aitch - Rain (feat. Tay Keith)"));
        assert_eq!(result.artist, "Aitch");
        assert_eq!(result.title, "Rain (feat. Tay Keith)");
    }

    #[test]
    fn parse_fallback_no_separator() {
        let result = parse_metadata(scraper_title_only("Some Song Title"));
        assert_eq!(result.artist, "");
        assert_eq!(result.title, "Some Song Title");
    }

    #[test]
    fn parse_fallback_empty_title() {
        let result = parse_metadata(scraper_title_only(""));
        assert_eq!(result.artist, "");
        assert_eq!(result.title, "");
    }

    #[test]
    fn parse_empty_artist_still_uses_structured_name() {
        let response: ScraperResponse = serde_json::from_str(
            r#"{"alias":"test","stationId":1,"updated":1,"trackId":"456","title":"Fallback Title","iName":"Structured Title","iArtist":"","iImg":""}"#,
        )
        .unwrap();
        let result = parse_metadata(response);
        assert_eq!(result.artist, "Fallback Title".split_once(" - ").map(|_| "").unwrap_or(""));
        assert_eq!(result.title, "Fallback Title");
    }

    #[test]
    fn parse_empty_name_still_uses_fallback() {
        let response: ScraperResponse = serde_json::from_str(
            r#"{"alias":"test","stationId":1,"updated":1,"trackId":"789","title":"Artist - Song","iName":"","iArtist":"","iImg":""}"#,
        )
        .unwrap();
        let result = parse_metadata(response);
        assert_eq!(result.artist, "Artist");
        assert_eq!(result.title, "Song");
    }
}
