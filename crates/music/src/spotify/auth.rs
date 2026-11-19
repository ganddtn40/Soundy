use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::spotify::offline;
use crate::vault::Vault;
use crate::{PromptSink, SignInFailure, SignInProblem, SignInPrompt, credentials};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::{FileId, Session, SessionConfig};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge, RedirectUrl, RefreshToken,
    Scope, TokenResponse, TokenUrl, basic::BasicClient, basic::BasicErrorResponseType,
};

pub const DEFAULT_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8989/login";

const PRODUCT_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
const PRODUCT_POLL: std::time::Duration = std::time::Duration::from_millis(100);
/// The bytes of a Spotify file id, which librespot names its kept files by.
const FILE_ID: usize = 20;
/// The metadata-only sign-in kept beside librespot's, under the same accounts folder. It carries
/// a refresh token, so a playlist can be read again without another browser round trip.
const METADATA: &str = "metadata.json";
/// How long before expiry a metadata access token is refreshed, so a request never starts on a
/// token with a second left.
const REFRESH_MARGIN: u64 = 120;

pub const SCOPES: &[&str] = &[
    "playlist-read-collaborative",
    "playlist-read-private",
    "streaming",
    "user-follow-read",
    "user-library-read",
    "user-read-email",
    "user-read-playback-state",
    "user-read-private",
    "user-read-recently-played",
    "user-top-read",
];

/// What the metadata-only sign-in asks for: reading the account's own or collaborative
/// playlists. It deliberately leaves out playback, library, email and profile scopes, so the
/// browser consent matches what this path can actually do.
const METADATA_SCOPES: &[&str] = &["playlist-read-collaborative", "playlist-read-private"];

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub client_id: String,
    pub redirect_uri: String,
    pub cache_dir: PathBuf,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_CLIENT_ID.to_owned(),
            redirect_uri: DEFAULT_REDIRECT_URI.to_owned(),
            cache_dir: credentials::dir("spotify"),
        }
    }
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(redirect_uri) =
            std::env::var("SOUNDY_REDIRECT_URI").or_else(|_| std::env::var("SONORA_REDIRECT_URI"))
        {
            config.redirect_uri = redirect_uri;
        }
        config
    }

    /// The stored credential librespot writes after a successful connect.
    pub fn file(&self) -> PathBuf {
        self.cache_dir.join(credentials::FILE)
    }

    /// The metadata-only sign-in, kept apart from librespot's playback credential.
    pub fn metadata_file(&self) -> PathBuf {
        self.cache_dir.join(METADATA)
    }
}

/// What one completed Spotify authorization handed back. The refresh token is the part worth
/// keeping; the access token lives for an hour.
struct Grant {
    access: String,
    refresh: Option<String>,
    expires_in: u64,
}

/// The metadata-only sign-in on disk. It never holds a client secret, and a build with no
/// stored token simply asks Spotify once through the browser.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Metadata {
    access_token: String,
    refresh_token: String,
    /// Seconds since the epoch at which `access_token` stops being accepted.
    expires_at: u64,
}

impl Metadata {
    fn usable(&self) -> Option<String> {
        let now = now();
        (!self.access_token.is_empty() && self.expires_at > now + REFRESH_MARGIN)
            .then(|| self.access_token.clone())
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn read_metadata(path: &Path) -> Option<Metadata> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_metadata(path: &Path, grant: &Grant, previous: Option<&Metadata>) -> Result<()> {
    let metadata = Metadata {
        access_token: grant.access.clone(),
        // Spotify may hand back a new refresh token. When it does not, the old one stays valid.
        refresh_token: grant
            .refresh
            .clone()
            .or_else(|| previous.map(|metadata| metadata.refresh_token.clone()))
            .unwrap_or_default(),
        expires_at: now().saturating_add(grant.expires_in),
    };
    let bytes = serde_json::to_vec(&metadata).context("cannot encode the Spotify sign-in")?;
    credentials::write(path, &bytes)?;
    credentials::secure(path);
    Ok(())
}

/// Answers a bearer token for the Web API without needing a Premium playback session. A stored
/// token is refreshed when it is near expiry; when there is none, or Spotify revoked it, the
/// listener signs in once through the browser. The token is kept in the app's private accounts
/// folder and is never written to a log.
///
/// This is blocking: the caller runs it on the blocking pool, never on the window thread.
pub fn metadata_access(config: &AuthConfig, cancel: &AtomicBool) -> Result<String> {
    let path = config.metadata_file();
    let mut stored = read_metadata(&path);

    if let Some(token) = stored.as_ref().and_then(Metadata::usable) {
        return Ok(token);
    }

    if let Some(refresh) = stored
        .as_ref()
        .map(|metadata| metadata.refresh_token.as_str())
        .filter(|refresh| !refresh.is_empty())
    {
        match refresh_access(config, refresh) {
            Ok(grant) => {
                save_metadata(&path, &grant, stored.as_ref())?;
                return Ok(grant.access);
            }
            Err(RefreshError::Revoked) => {
                // The listener revoked the app or the token expired. A fresh sign-in is the
                // only honest way forward, so the stale pair is removed rather than kept.
                credentials::remove(&path);
                stored = None;
            }
            Err(RefreshError::Failed(error)) => return Err(error),
        }
    }

    let prompt: PromptSink = Arc::new(|_| {});
    let grant = authorize(
        &config.client_id,
        &config.redirect_uri,
        METADATA_SCOPES,
        prompt,
        Some(cancel),
    )?;
    save_metadata(&path, &grant, stored.as_ref())?;
    Ok(grant.access)
}

enum RefreshError {
    Revoked,
    Failed(anyhow::Error),
}

fn grant(response: &impl TokenResponse) -> Grant {
    Grant {
        access: response.access_token().secret().to_owned(),
        refresh: response
            .refresh_token()
            .map(|token| token.secret().to_owned()),
        expires_in: response
            .expires_in()
            .map(|left| left.as_secs())
            .unwrap_or(3600),
    }
}

pub fn release(config: &AuthConfig) {
    let Some(address) = socket_address(&config.redirect_uri) else {
        log::warn!(
            "auth: cannot read a socket address from {}",
            config.redirect_uri
        );
        return;
    };
    let Ok(mut stream) = std::net::TcpStream::connect(address) else {
        return;
    };
    let _ = std::io::Write::write_all(&mut stream, b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
}

fn socket_address(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("http://")
        .or_else(|| uri.strip_prefix("https://"))?;
    let authority = rest.split('/').next().filter(|host| !host.is_empty())?;
    match authority.rsplit_once(':') {
        Some((_, port)) if port.chars().all(|digit| digit.is_ascii_digit()) => {
            Some(authority.to_owned())
        }
        _ => Some(format!("{authority}:80")),
    }
}

pub async fn restore(config: &AuthConfig) -> Result<Option<Session>> {
    let session = session(config)?;
    let Some(credentials) = session.cache().and_then(|cache| cache.credentials()) else {
        return Ok(None);
    };

    session.connect(credentials, true).await.map_err(denied)?;
    credentials::secure(&config.file());
    premium(&session).await?;
    Ok(Some(session))
}

pub async fn login(config: &AuthConfig, prompt: PromptSink) -> Result<Session> {
    let client_id = config.client_id.clone();
    let redirect_uri = config.redirect_uri.clone();

    let grant = tokio::task::spawn_blocking(move || {
        authorize(&client_id, &redirect_uri, SCOPES, prompt, None)
    })
    .await??;

    let session = session(config)?;
    session
        .connect(Credentials::with_access_token(grant.access), true)
        .await
        .map_err(denied)?;
    credentials::secure(&config.file());
    premium(&session).await?;
    Ok(session)
}

fn refresh_access(config: &AuthConfig, refresh: &str) -> std::result::Result<Grant, RefreshError> {
    let client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_auth_uri(
            AuthUrl::new("https://accounts.spotify.com/authorize".to_owned())
                .map_err(|error| RefreshError::Failed(error.into()))?,
        )
        .set_token_uri(
            TokenUrl::new("https://accounts.spotify.com/api/token".to_owned())
                .map_err(|error| RefreshError::Failed(error.into()))?,
        )
        .set_redirect_uri(
            RedirectUrl::new(config.redirect_uri.clone())
                .map_err(|error| RefreshError::Failed(error.into()))?,
        );
    let refresh = RefreshToken::new(refresh.to_owned());
    match client.exchange_refresh_token(&refresh).request(&exchange) {
        Ok(response) => Ok(grant(&response)),
        // Only a refused grant means the stored refresh token is dead. A rate limit, a server
        // error or a malformed answer is reported as it is, and the token is kept for the next
        // try rather than sending the listener through a browser that cannot help.
        Err(oauth2::RequestTokenError::ServerResponse(error))
            if error.error() == &BasicErrorResponseType::InvalidGrant =>
        {
            Err(RefreshError::Revoked)
        }
        Err(error) => Err(RefreshError::Failed(anyhow!(
            "cannot refresh the Spotify sign-in: {error}"
        ))),
    }
}

fn authorize(
    client_id: &str,
    redirect_uri: &str,
    requested: &[&str],
    prompt: PromptSink,
    cancel: Option<&AtomicBool>,
) -> Result<Grant> {
    let client = BasicClient::new(ClientId::new(client_id.to_owned()))
        .set_auth_uri(AuthUrl::new(
            "https://accounts.spotify.com/authorize".to_owned(),
        )?)
        .set_token_uri(TokenUrl::new(
            "https://accounts.spotify.com/api/token".to_owned(),
        )?)
        .set_redirect_uri(RedirectUrl::new(redirect_uri.to_owned())?);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let scopes = requested
        .iter()
        .map(|scope| Scope::new((*scope).to_owned()));
    let (url, csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(scopes)
        .set_pkce_challenge(challenge)
        .url();

    // Bind before the browser opens, so a callback that arrives immediately is not refused.
    let address = socket_address(redirect_uri)
        .with_context(|| format!("redirect URI has no socket address: {redirect_uri}"))?;
    let listener = TcpListener::bind(&address)
        .with_context(|| format!("cannot listen for Spotify login at {address}"))?;

    prompt(SignInPrompt::Url(url.to_string()));
    open::that_in_background(url.as_str());

    let query = wait_for_callback(listener, cancel)?;
    // The browser must hand back the state this sign-in generated. Checking it means a page
    // that is not the one Soundy opened cannot feed the listener an authorization code of its
    // choosing. PKCE already makes a stolen code useless; this closes the other half.
    if query.get("state").map(String::as_str) != Some(csrf.secret().as_str()) {
        return Err(SignInFailure(SignInProblem::Refused).into());
    }
    if let Some(error) = query.get("error") {
        let problem = match error.as_str() {
            "access_denied" => SignInProblem::Cancelled,
            _ => SignInProblem::Refused,
        };
        return Err(SignInFailure(problem).into());
    }
    let code = query
        .get("code")
        .context("Spotify login callback had no authorization code")?;
    let response = client
        .exchange_code(AuthorizationCode::new(code.to_owned()))
        .set_pkce_verifier(verifier)
        .request(&exchange)
        .map_err(|error| anyhow!("failed to exchange Spotify authorization code: {error}"))?;
    Ok(grant(&response))
}

/// Waits for the browser callback, ignoring requests that are not it (a favicon, a second tab)
/// and stopping when the dialog's Cancel sets `cancel`. The listener is non-blocking so a
/// cancelled sign-in never waits for a browser that was already closed.
fn wait_for_callback(
    listener: TcpListener,
    cancel: Option<&AtomicBool>,
) -> Result<HashMap<String, String>> {
    listener.set_nonblocking(true)?;
    loop {
        if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
            return Err(SignInFailure(SignInProblem::Cancelled).into());
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
                let mut request = String::new();
                if BufReader::new(&stream).read_line(&mut request).is_err() {
                    continue;
                }
                let Some(target) = request.split_whitespace().nth(1) else {
                    continue;
                };
                let Ok(callback) = oauth2::url::Url::parse(&format!("http://localhost{target}"))
                else {
                    continue;
                };
                let query: HashMap<String, String> = callback
                    .query_pairs()
                    .map(|(name, value)| (name.into_owned(), value.into_owned()))
                    .collect();
                if !query.contains_key("code") && !query.contains_key("error") {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    );
                    continue;
                }
                let response = "You can return to Soundy.";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}",
                    response.len()
                )?;
                return Ok(query);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// Carries one oauth2 request over reqwest. oauth2 only knows the reqwest it was built against,
/// so the workspace one is handed in as a plain function instead.
fn exchange(request: oauth2::HttpRequest) -> Result<oauth2::HttpResponse, reqwest::Error> {
    let sent = reqwest::blocking::Client::new().execute(request.try_into()?)?;
    let status = sent.status();
    let headers = sent.headers().clone();
    let mut response = http::Response::new(sent.bytes()?.to_vec());
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

async fn premium(session: &Session) -> Result<()> {
    let deadline = tokio::time::Instant::now() + PRODUCT_WAIT;
    loop {
        if let Some(account) = session.user_data().attributes.get("type") {
            match account.as_str() {
                "premium" => return Ok(()),
                _ => {
                    session.shutdown();
                    return Err(anyhow::Error::new(SignInFailure(SignInProblem::Premium)));
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(PRODUCT_POLL).await;
    }
}

fn denied(error: librespot_core::Error) -> anyhow::Error {
    let problem = classify(&error.to_string());
    anyhow::Error::new(error).context(SignInFailure(problem))
}

fn classify(message: &str) -> SignInProblem {
    let message = message.to_lowercase();
    if message.contains("travel restriction") {
        return SignInProblem::Region;
    }
    if message.contains("bad credentials") || message.contains("invalid credentials") {
        return SignInProblem::Credentials;
    }
    if message.contains("connection")
        || message.contains("timed out")
        || message.contains("dns")
        || message.contains("network")
    {
        return SignInProblem::Network;
    }
    SignInProblem::Refused
}

pub fn forget(config: &AuthConfig) {
    credentials::remove(&config.file());
    credentials::remove(&config.metadata_file());
}

/// Drops only the metadata-only sign-in, leaving a playback credential alone. The importer asks
/// for this when the Web API refuses a stored token, so the next attempt signs in again instead
/// of replaying a token Spotify has already rejected.
pub fn forget_metadata(config: &AuthConfig) {
    credentials::remove(&config.metadata_file());
}

pub(crate) fn session(config: &AuthConfig) -> Result<Session> {
    // librespot keeps each file as the CDN sent it, still under the key Spotify hands out
    // again on every play, so a kept file is no easier to play than the stream.
    let audio = Vault::foreign("spotify");
    let cache = Cache::new(
        Some(config.cache_dir.as_path()),
        None,
        audio.as_ref().map(|(folder, _)| folder.as_path()),
        audio.as_ref().map(|(_, room)| *room),
    )
    .with_context(|| format!("cannot open cache at {}", config.cache_dir.display()))?;

    let session_config = SessionConfig {
        client_id: config.client_id.clone(),
        device_id: device_id(&config.cache_dir),
        ..Default::default()
    };

    let session = Session::new(session_config, Some(cache));
    if let Some(((folder, _), cache)) = audio.zip(session.cache()) {
        let cache = Arc::downgrade(cache);
        Vault::adopt("spotify", move || {
            cache
                .upgrade()
                .map(|cache| clear(&cache, &folder))
                .is_some()
        });
    }
    Ok(session)
}

/// Removes every audio file librespot keeps in `folder` through `cache`, so its count of what
/// it holds stays true while its session runs. librespot files each one under the first two
/// hex digits of its id, and anything else in the folder is removed as it is.
fn clear(cache: &Cache, folder: &Path) {
    for group in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let prefix = group.file_name().to_string_lossy().into_owned();
        for item in std::fs::read_dir(group.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = format!("{prefix}{}", item.file_name().to_string_lossy());
            let removed = match offline::hex(&name).filter(|raw| raw.len() == FILE_ID) {
                Some(raw) => cache
                    .remove_file(FileId::from_raw(&raw))
                    .map_err(anyhow::Error::from),
                None => std::fs::remove_file(item.path()).map_err(anyhow::Error::from),
            };
            if let Err(error) = removed {
                log::warn!("spotify: cannot remove a kept file: {error:#}");
            }
        }
        std::fs::remove_dir(group.path()).ok();
    }
}

/// The id this install goes by on Spotify's device list. A new one each launch would leave a
/// stale Sonora behind every time, so the first one is kept next to the credentials.
fn device_id(dir: &Path) -> String {
    let file = dir.join("device-id");
    let kept = std::fs::read_to_string(&file)
        .map(|id| id.trim().to_owned())
        .ok()
        .filter(|id| !id.is_empty());
    if let Some(id) = kept {
        return id;
    }

    let id = SessionConfig::default().device_id;
    if let Err(error) = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&file, &id)) {
        log::warn!("auth: cannot keep the device id: {error}");
    }
    id
}

#[cfg(test)]
mod tests {
    use super::{Grant, METADATA, Metadata, now, read_metadata, save_metadata, socket_address};

    #[test]
    fn reads_host_and_port() {
        assert_eq!(
            socket_address("http://127.0.0.1:8989/login").as_deref(),
            Some("127.0.0.1:8989")
        );
    }

    #[test]
    fn defaults_a_missing_port() {
        assert_eq!(
            socket_address("http://localhost/login").as_deref(),
            Some("localhost:80")
        );
    }

    #[test]
    fn rejects_a_uri_without_a_scheme() {
        assert!(socket_address("127.0.0.1:8989/login").is_none());
    }

    #[test]
    fn a_metadata_token_is_used_until_it_is_near_expiry() {
        let fresh = Metadata {
            access_token: "token".to_owned(),
            refresh_token: "refresh".to_owned(),
            expires_at: now() + 3600,
        };
        assert_eq!(fresh.usable().as_deref(), Some("token"));

        let near = Metadata {
            expires_at: now() + super::REFRESH_MARGIN - 1,
            ..fresh.clone()
        };
        assert_eq!(near.usable(), None);

        let missing = Metadata {
            access_token: String::new(),
            ..fresh
        };
        assert_eq!(missing.usable(), None);
    }

    #[test]
    fn the_metadata_sign_in_asks_only_for_playlist_read_scopes() {
        assert!(
            super::METADATA_SCOPES
                .iter()
                .all(|scope| scope.starts_with("playlist-read-"))
        );
        assert!(!super::METADATA_SCOPES.contains(&"streaming"));
    }

    #[test]
    fn forgetting_the_metadata_sign_in_keeps_the_playback_credential() {
        let dir = tempfile::tempdir().unwrap();
        let config = super::AuthConfig {
            client_id: "id".to_owned(),
            redirect_uri: "http://127.0.0.1:8989/login".to_owned(),
            cache_dir: dir.path().to_path_buf(),
        };
        std::fs::write(config.metadata_file(), b"{}").unwrap();
        std::fs::write(config.file(), b"playback").unwrap();
        super::forget_metadata(&config);
        assert!(!config.metadata_file().exists());
        assert!(config.file().exists());
    }

    #[test]
    fn a_refresh_without_a_new_token_keeps_the_stored_one() {
        // Credential writes are deliberately confined to the system's private data folder. Put
        // the test file under it, as the app does, so the guard is exercised too; `tempfile`
        // removes the folder afterwards.
        let data = dirs::data_local_dir().expect("a private data folder");
        let dir = tempfile::Builder::new()
            .prefix("soundy-auth-test-")
            .tempdir_in(data)
            .unwrap();
        let path = dir.path().join(METADATA);
        let previous = Metadata {
            access_token: "old".to_owned(),
            refresh_token: "kept".to_owned(),
            expires_at: 0,
        };
        let grant = Grant {
            access: "new".to_owned(),
            refresh: None,
            expires_in: 60,
        };

        save_metadata(&path, &grant, Some(&previous)).unwrap();

        let stored = read_metadata(&path).unwrap();
        assert_eq!(stored.access_token, "new");
        assert_eq!(stored.refresh_token, "kept");
        assert!(stored.expires_at > now());
    }
}
