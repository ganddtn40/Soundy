//! Public catalogue lookups the importer can use without an account.
//!
//! Audius exposes a no-auth search endpoint and Jamendo exposes one keyed by a developer
//! `client_id`. Both are only used to label a row as external playback with a link to the
//! catalogue's own page; neither is treated as a playable Soundy provider. A result is accepted
//! only when its title, credited artist and length describe the same recording.

use std::time::Duration;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::import::{SourceEntry, same_recording, search_query};

const AUDIUS: &str = "https://api.audius.co/v1";
const JAMENDO: &str = "https://api.jamendo.com/v3.0";
const TIMEOUT: Duration = Duration::from_secs(12);
const LIMIT: usize = 8;

/// The public catalogues one import may consult. It holds one HTTP client for both, so an
/// import creates no more than one connection pool.
#[derive(Clone)]
pub struct Catalogue {
    http: reqwest::Client,
    audius: String,
    jamendo: String,
    jamendo_client_id: Option<String>,
}

impl Catalogue {
    pub fn public() -> Self {
        Self::at(
            AUDIUS,
            JAMENDO,
            std::env::var("SOUNDY_JAMENDO_CLIENT_ID").ok(),
        )
    }

    pub fn at(
        audius: impl Into<String>,
        jamendo: impl Into<String>,
        jamendo_client_id: Option<String>,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(env!("CARGO_PKG_NAME"))
            .build()
            .unwrap_or_default();
        Self {
            http,
            audius: audius.into(),
            jamendo: jamendo.into(),
            jamendo_client_id: jamendo_client_id.filter(|id| !id.trim().is_empty()),
        }
    }

    /// Audius's own page for `entry`, when its public search returns the same recording.
    pub async fn audius(&self, entry: &SourceEntry) -> Result<Option<String>> {
        let query = search_query(entry);
        let response = self
            .http
            .get(format!("{}/tracks/search", self.audius))
            .query(&[("query", query.as_str()), ("limit", "8")])
            .send()
            .await
            .context("cannot ask Audius")?;
        if !response.status().is_success() {
            anyhow::bail!("Audius answered {}", response.status());
        }
        let body: AudiusBody = response
            .json()
            .await
            .context("cannot read Audius's answer")?;
        Ok(body
            .data
            .into_iter()
            .take(LIMIT)
            .find(|track| {
                track.is_streamable
                    && same_recording(
                        entry,
                        &track.title,
                        track.user.name.as_deref().unwrap_or_default(),
                        Duration::from_secs(track.duration),
                        track.isrc.as_deref(),
                    )
            })
            .and_then(|track| {
                let permalink = track.permalink?;
                (!permalink.is_empty()).then(|| format!("https://audius.co{}", permalink.trim()))
            }))
    }

    /// Jamendo's own page for `entry`, when a `client_id` is configured and its public search
    /// returns the same recording. Jamendo is optional: a build without the environment value
    /// simply skips it and leaves the row to the other catalogues.
    pub async fn jamendo(&self, entry: &SourceEntry) -> Result<Option<String>> {
        let Some(client_id) = self.jamendo_client_id.as_deref() else {
            return Ok(None);
        };
        let query = search_query(entry);
        let response = self
            .http
            .get(format!("{}/tracks/", self.jamendo))
            .query(&[
                ("client_id", client_id),
                ("format", "json"),
                ("limit", "8"),
                ("search", query.as_str()),
            ])
            .send()
            .await
            .context("cannot ask Jamendo")?;
        if !response.status().is_success() {
            anyhow::bail!("Jamendo answered {}", response.status());
        }
        let body: JamendoBody = response
            .json()
            .await
            .context("cannot read Jamendo's answer")?;
        Ok(body
            .results
            .into_iter()
            .take(LIMIT)
            .find(|track| {
                same_recording(
                    entry,
                    &track.name,
                    &track.artist_name,
                    Duration::from_secs(track.duration),
                    None,
                )
            })
            .and_then(|track| {
                let url = track.shareurl.or_else(|| {
                    (track.id != 0).then(|| format!("https://www.jamendo.com/track/{}", track.id))
                })?;
                (!url.is_empty()).then_some(url)
            }))
    }

    /// The first permitted public catalogue page matching `entry`, Audius first. A network
    /// failure is returned so the caller can stop asking once the connection is gone, rather
    /// than waiting on the same timeout for every remaining row.
    pub async fn find(&self, entry: &SourceEntry) -> Result<Option<String>> {
        let mut failure = None;
        match self.audius(entry).await {
            Ok(Some(url)) => return Ok(Some(url)),
            Ok(None) => {}
            Err(error) => {
                log::debug!("import: Audius lookup failed: {error:#}");
                failure = Some(error);
            }
        }
        match self.jamendo(entry).await {
            Ok(Some(url)) => return Ok(Some(url)),
            Ok(None) => {}
            Err(error) => {
                log::debug!("import: Jamendo lookup failed: {error:#}");
                failure = Some(error);
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}

#[derive(Deserialize)]
struct AudiusBody {
    #[serde(default)]
    data: Vec<AudiusTrack>,
}

#[derive(Deserialize)]
struct AudiusTrack {
    title: String,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    is_streamable: bool,
    isrc: Option<String>,
    permalink: Option<String>,
    #[serde(default)]
    user: AudiusUser,
}

#[derive(Default, Deserialize)]
struct AudiusUser {
    name: Option<String>,
}

#[derive(Deserialize)]
struct JamendoBody {
    #[serde(default)]
    results: Vec<JamendoTrack>,
}

#[derive(Deserialize)]
struct JamendoTrack {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    artist_name: String,
    #[serde(default)]
    duration: u64,
    shareurl: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;

    fn entry() -> SourceEntry {
        SourceEntry {
            title: "Hardwired".into(),
            artists: "Rameses B".into(),
            duration: Duration::from_secs(302),
            ..Default::default()
        }
    }

    /// A one-answer HTTP server, so the search is tested over a real request rather than a
    /// mocked client.
    fn serve(body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{address}")
    }

    #[test]
    fn builds_a_search_with_the_artist_when_there_is_one() {
        assert_eq!(search_query(&entry()), "Hardwired Rameses B");
        let mut unnamed = entry();
        unnamed.artists.clear();
        assert_eq!(search_query(&unnamed), "Hardwired");
    }

    #[test]
    fn does_not_accept_a_remix_or_a_different_artist() {
        let wanted = entry();
        assert!(same_recording(
            &wanted,
            "Rameses B - Hardwired",
            "Rameses B",
            Duration::from_secs(302),
            None
        ));
        assert!(!same_recording(
            &wanted,
            "Rameses B - Hardwired (Remix)",
            "Rameses B",
            Duration::from_secs(302),
            None
        ));
        assert!(!same_recording(
            &wanted,
            "Hardwired",
            "Another Artist",
            Duration::from_secs(302),
            None
        ));
    }

    #[tokio::test]
    async fn takes_the_first_same_recording_from_audius_and_skips_versions() {
        let body = r#"{"data":[
            {"title":"Hardwired (Remix)","duration":302,"is_streamable":true,"permalink":"/ramesesb/remix","user":{"name":"Rameses B"}},
            {"title":"Hardwired","duration":300,"is_streamable":true,"permalink":"/ramesesb/hardwired","user":{"name":"Rameses B"}}
        ]}"#;
        let catalogue = Catalogue::at(serve(body), "http://127.0.0.1:1", None);
        assert_eq!(
            catalogue.audius(&entry()).await.unwrap().as_deref(),
            Some("https://audius.co/ramesesb/hardwired")
        );
    }

    #[tokio::test]
    async fn leaves_a_row_unlinked_when_audius_has_no_same_recording() {
        let body = r#"{"data":[
            {"title":"Hardwired (Live)","duration":302,"is_streamable":true,"permalink":"/ramesesb/live","user":{"name":"Rameses B"}},
            {"title":"Hardwired","duration":302,"is_streamable":false,"permalink":"/ramesesb/blocked","user":{"name":"Rameses B"}}
        ]}"#;
        let catalogue = Catalogue::at(serve(body), "http://127.0.0.1:1", None);
        assert_eq!(catalogue.audius(&entry()).await.unwrap(), None);
        assert_eq!(catalogue.jamendo(&entry()).await.unwrap(), None);
    }
}
