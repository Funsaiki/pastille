//! Spotify's own volume (Funsaiki's request, Premium account), through Spotify's web API.
//! Connected once, from the Settings page (Spotify section, "Connect"): authorization in
//! the browser with PKCE (no secret to keep), redirect to a local address (REDIRECT,
//! declared in each user's Spotify app on developer.spotify.com). That app's id
//! and the refresh token are in spotify.json, next to apps.json. Everyone has their own
//! app: without an id, connecting starts with a local page
//! that explains how to create it and where to paste its id (spotify-setup.html).
//! Without a connection, the slider of the now playing card sets Spotify's volume in the
//! Windows mixer (mixer.rs). Failures (connection, authorization revoked) show up
//! in the launcher's notifications (`problem`): the installed build has no console.

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use windows::core::{h, RuntimeType, HSTRING};
use windows::Foundation::Uri;
use windows::Storage::Streams::UnicodeEncoding;
use windows::Web::Http::Filters::{HttpBaseProtocolFilter, HttpCacheReadBehavior};
use windows::Web::Http::Headers::HttpCredentialsHeaderValue;
use windows::Web::Http::{HttpClient, HttpMethod, HttpRequestMessage, HttpStringContent};
use windows::Win32::Foundation::ERROR_TIMEOUT;
use windows::Win32::Security::Cryptography::{
    BCryptGenRandom, BCryptHash, BCRYPT_SHA256_ALG_HANDLE, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_future::{AsyncStatus, IAsyncOperationWithProgress};

use crate::i18n::tr;

const PORT: u16 = 47913;
/// To be declared as is in the Spotify app (Redirect URIs)
pub const REDIRECT: &str = "http://127.0.0.1:47913/callback";
/// Reading and controlling the player, playlists (auto "spotify" group). An added scope
/// requires reconnecting (Status::Outdated).
const SCOPES: &str = "user-read-playback-state user-modify-playback-state playlist-read-private playlist-read-collaborative";
/// Playlists in the group, at most (liked songs included)
const MAX_PLAYLISTS: usize = 100;
const API: &str = "https://api.spotify.com/v1";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
/// Time to sign in in the browser
const WAIT: Duration = Duration::from_secs(180);
/// First setup: time to create one's Spotify app by following the setup page
const SETUP_WAIT: Duration = Duration::from_secs(15 * 60);
/// Tab from an earlier attempt (Spotify's redirect or setup page)
fn stale() -> String {
    tr("cet onglet vient d'un ancien essai : utilise le dernier ouvert.", "this tab comes from an earlier attempt: use the latest one.").into()
}
/// Limit for a request (join() would wait forever if the network doesn't answer)
const TIMEOUT: Duration = Duration::from_secs(8);

/// spotify.json
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Id (Client ID) of one's Spotify app, pasted on the setup page
    #[serde(default)]
    client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

#[derive(Default)]
pub struct Spotify {
    auth: Mutex<Auth>,
    /// One refresh at a time: Spotify can replace the refresh token, two
    /// requests with the old one would get in each other's way
    refreshing: Mutex<()>,
    /// During a connection: the page opened in the browser, reopened by the Settings page
    /// (tab closed or lost)
    pending: Mutex<Option<String>>,
}

#[derive(Default)]
struct Auth {
    saved: Saved,
    /// Access token and end of its validity
    access: Option<(String, Instant)>,
    /// Granted scopes, from the last response of the token endpoint
    granted: Option<String>,
    /// Last failure (connection, authorization revoked), shown in the notifications
    problem: Option<String>,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    // Simulated first launch (config::example_mode): Spotify not set up yet, as on
    // another PC; Funsaiki's connection stays intact
    let name = if crate::config::example_mode() { "spotify-example.json" } else { "spotify.json" };
    app.path().app_config_dir().ok().map(|dir| dir.join(name))
}

/// File missing or unreadable: no connection
fn read_saved(app: &AppHandle) -> Saved {
    file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// At startup
pub fn load(app: &AppHandle) -> Spotify {
    Spotify { auth: Mutex::new(Auth { saved: read_saved(app), ..Default::default() }), ..Default::default() }
}

/// "Reload the config": reads spotify.json again (id added by hand, for example)
pub fn reload(app: &AppHandle) {
    let saved = read_saved(app);
    {
        let spotify = app.state::<Spotify>();
        let mut auth = spotify.auth.lock().unwrap();
        if auth.saved != saved {
            auth.access = None;
        }
        auth.saved = saved;
    }
}

/// Temporary file then rename: a shutdown in the middle of writing doesn't lose the id
fn save(app: &AppHandle, saved: &Saved) {
    let Some(path) = file(app) else { return };
    let temp = path.with_extension("json.tmp");
    let written = fs::write(&temp, serde_json::to_string_pretty(saved).unwrap_or_default())
        .and_then(|_| fs::rename(&temp, &path));
    if let Err(e) = written {
        eprintln!("[pastille] Spotify: {} not saved: {e}", path.display());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    /// No app id in spotify.json
    NoClient,
    Disconnected,
    Connected,
    /// Connected, but without all the scopes of SCOPES (connection older than their addition)
    Outdated,
}

fn status(app: &AppHandle) -> Status {
    let spotify = app.state::<Spotify>();
    let auth = spotify.auth.lock().unwrap();
    match (&auth.saved.client_id, &auth.saved.refresh_token) {
        (id, _) if id.trim().is_empty() => Status::NoClient,
        (_, None) => Status::Disconnected,
        (_, Some(_)) if auth.granted.as_deref().is_some_and(|granted| !has_scopes(granted)) => Status::Outdated,
        (_, Some(_)) => Status::Connected,
    }
}

fn connected(app: &AppHandle) -> bool {
    matches!(status(app), Status::Connected | Status::Outdated)
}

fn has_scopes(granted: &str) -> bool {
    SCOPES.split(' ').all(|scope| granted.split(' ').any(|g| g == scope))
}

/* ── Settings page ── */

/// Connection state for the page: "pending" during a connection in the browser
pub fn link(app: &AppHandle) -> &'static str {
    if app.state::<Spotify>().pending.lock().unwrap().is_some() {
        return "pending";
    }
    match status(app) {
        Status::NoClient | Status::Disconnected => "disconnected",
        Status::Connected => "connected",
        Status::Outdated => "outdated",
    }
}

/// Click on the "Reconnect" notification: like the page's button, without ever disconnecting
#[tauri::command]
pub fn spotify_reconnect(app: AppHandle) -> Result<(), String> {
    if status(&app) == Status::Connected {
        return Err(tr("Spotify est déjà connecté.", "Spotify is already connected.").into());
    }
    toggle(&app);
    Ok(())
}

/// Button of the Settings page: connection in the browser, or disconnection. Connection in
/// progress: its page, reopened.
pub fn toggle(app: &AppHandle) {
    let pending = app.state::<Spotify>().pending.lock().unwrap().clone();
    if let Some(page) = pending {
        std::thread::spawn(move || {
            if let Err(e) = crate::shell::open_as_is(&page) {
                eprintln!("[pastille] Spotify: {e}");
            }
        });
        return;
    }
    match status(app) {
        Status::Connected => disconnect(app, None),
        Status::NoClient | Status::Disconnected | Status::Outdated => start_connect(app),
    }
}

/// Listens on the redirect address, then the rest off the main thread: browser, wait,
/// tokens. Without an app id, the browser opens on the setup page.
fn start_connect(app: &AppHandle) {
    let client_id = app.state::<Spotify>().auth.lock().unwrap().saved.client_id.trim().to_string();
    // Listen before opening the browser: the answer must not arrive before
    let prepared = Login::new().and_then(|login| {
        let listener = TcpListener::bind(("127.0.0.1", PORT))
            .map_err(|e| tr(format!("adresse de retour {REDIRECT} indisponible ({e})."), format!("redirect address {REDIRECT} unavailable ({e}).")))?;
        Ok((login, listener))
    });
    let (login, listener) = match prepared {
        Ok(prepared) => prepared,
        Err(e) => return finish(app, Err(e)),
    };
    let page = if client_id.is_empty() { login.setup_url() } else { login.authorize_url(&client_id) };
    *app.state::<Spotify>().pending.lock().unwrap() = Some(page.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        let result = crate::shell::open_as_is(&page).and_then(|_| connect(&app, &listener, &login, client_id));
        *app.state::<Spotify>().pending.lock().unwrap() = None;
        finish(&app, result);
    });
}

/// End of a connection: a failure is shown in the notifications
fn finish(app: &AppHandle, result: Result<(), String>) {
    if let Err(e) = &result {
        eprintln!("[pastille] Spotify: {e}");
    }
    app.state::<Spotify>().auth.lock().unwrap().problem = result.as_ref().err().map(|e| tr(format!("Connexion impossible : {e}"), format!("Couldn't connect: {e}")));
    // New scopes: the playlists can now be read
    if result.is_ok() {
        crate::config::rescan(app);
    }
}

/// `problem`: why, if it isn't Funsaiki who chose to disconnect
fn disconnect(app: &AppHandle, problem: Option<String>) {
    {
        let spotify = app.state::<Spotify>();
        let mut auth = spotify.auth.lock().unwrap();
        auth.saved.refresh_token = None;
        auth.access = None;
        auth.problem = problem;
        save(app, &auth.saved);
    }
}

/* ── Connection (PKCE) ── */

/// A connection: PKCE verifier and `state`, which also serves as a token for the setup
/// page (another web page can't use it)
struct Login {
    verifier: String,
    challenge: String,
    state: String,
}

impl Login {
    fn new() -> Result<Login, String> {
        let verifier = b64(&random(48)?); // 64 characters
        Ok(Login { challenge: challenge(&verifier)?, verifier, state: b64(&random(16)?) })
    }

    /// Spotify's authorization page
    fn authorize_url(&self, client_id: &str) -> String {
        format!(
            "https://accounts.spotify.com/authorize?{}",
            form(&[
                ("client_id", client_id),
                ("response_type", "code"),
                ("redirect_uri", REDIRECT),
                ("code_challenge_method", "S256"),
                ("code_challenge", &self.challenge),
                ("scope", SCOPES),
                ("state", &self.state),
            ])
        )
    }

    /// Setup page, served by the connection's listener
    fn setup_url(&self) -> String {
        format!("http://127.0.0.1:{PORT}/spotify?n={}", self.state)
    }
}

/// Empty `client_id`: first setup, the browser is on the setup page and
/// the id arrives through it. It is only kept once the connection succeeds.
fn connect(app: &AppHandle, listener: &TcpListener, login: &Login, mut client_id: String) -> Result<(), String> {
    let setup = client_id.is_empty();
    let (mut stream, params) =
        wait_for_callback(listener, &login.state, if setup { SETUP_WAIT } else { WAIT }, |stream, path, params| {
            if !setup {
                respond(stream, "404 Not Found", "");
            } else if let Some(id) = serve_setup(stream, login, path, params) {
                client_id = id;
            }
        })?;
    let result = (|| {
        let code = params.get("code").ok_or_else(|| match params.get("error").map(String::as_str) {
            Some("access_denied") => tr("autorisation refusée.", "authorization denied.").to_string(),
            other => tr(
                format!("autorisation refusée ({}).", other.unwrap_or("sans motif")),
                format!("authorization denied ({}).", other.unwrap_or("no reason")),
            ),
        })?;
        let token = post_token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("client_id", &client_id),
            ("code_verifier", &login.verifier),
        ])
        .map_err(TokenError::message)?;
        if setup {
            remember_client(app, &client_id);
        }
        store(app, &token, None).map(|_| ())
    })();
    respond(&mut stream, "200 OK", &page(&result));
    result
}

/// Id pasted on the setup page, which worked
fn remember_client(app: &AppHandle, client_id: &str) {
    let spotify = app.state::<Spotify>();
    let mut auth = spotify.auth.lock().unwrap();
    auth.saved.client_id = client_id.to_string();
    auth.access = None;
    save(app, &auth.saved);
}

/// Waits for Spotify's answer on /callback with the right `state`, `wait` at most. An old
/// tab gets an explanation; the other requests (setup page, tab
/// icon) go through `other`, which answers them. The wait goes on.
fn wait_for_callback(
    listener: &TcpListener,
    state: &str,
    wait: Duration,
    mut other: impl FnMut(&mut TcpStream, &str, &HashMap<String, String>),
) -> Result<(TcpStream, HashMap<String, String>), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let target = request_target(&mut stream);
                let (path, query) = target.split_once('?').unwrap_or((&target, ""));
                let params = parse_query(query);
                if path != "/callback" {
                    other(&mut stream, path, &params);
                } else if params.get("state").map(String::as_str) == Some(state) {
                    return Ok((stream, params));
                } else {
                    respond(&mut stream, "400 Bad Request", &page(&Err(stale())));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let minutes = wait.as_secs() / 60;
    Err(tr(format!("pas de réponse de Spotify au bout de {minutes} minutes."), format!("no answer from Spotify after {minutes} minutes.")))
}

/* ── Setup page (first setup) ── */

/// /spotify: the tutorial and the id field; /client-id: the pasted id. Valid,
/// it is returned and the browser goes to Spotify's authorization; otherwise the page comes back
/// with the error. One can go back and paste another one as long as the wait lasts.
fn serve_setup(stream: &mut TcpStream, login: &Login, path: &str, params: &HashMap<String, String>) -> Option<String> {
    if !matches!(path, "/spotify" | "/client-id") {
        respond(stream, "404 Not Found", "");
        return None;
    }
    if params.get("n") != Some(&login.state) {
        respond(stream, "400 Bad Request", &page(&Err(stale())));
        return None;
    }
    if path == "/spotify" {
        respond(stream, "200 OK", &setup_page(&login.state, "", None));
        return None;
    }
    let typed = params.get("id").map(String::as_str).unwrap_or_default();
    let Some(id) = client_id_of(typed) else {
        respond(stream, "200 OK", &setup_page(&login.state, typed, Some(&client_id_error(typed))));
        return None;
    };
    redirect(stream, &login.authorize_url(&id));
    Some(id)
}

/// Spotify Client ID: 32 hexadecimal characters (surrounding spaces and uppercase accepted).
/// The Client secret has the same form: only Spotify tells them apart (page "A problem?").
fn client_id_of(text: &str) -> Option<String> {
    let id = text.trim().to_ascii_lowercase();
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
}

fn client_id_error(typed: &str) -> String {
    match typed.trim().chars().count() {
        0 => tr("Colle d'abord le Client ID de l'étape 4.", "First paste the Client ID from step 4.").into(),
        n => tr(
            format!("Ce n'est pas un Client ID : il compte 32 caractères, des chiffres et des lettres de a à f (là : {n})."),
            format!("That's not a Client ID: it has 32 characters, digits and letters from a to f (here: {n})."),
        ),
    }
}

/// Page in Pastille's language; the styles, shared by both, in spotify-setup.css
fn setup_page(state: &str, typed: &str, error: Option<&str>) -> String {
    let error = error.map(|e| format!(r#"<p class="error" role="alert">{}</p>"#, escape(e))).unwrap_or_default();
    tr(include_str!("spotify-setup.html"), include_str!("spotify-setup.en.html"))
        .replace("{{style}}", include_str!("spotify-setup.css"))
        .replace("{{favicon}}", FAVICON)
        .replace("{{fonts}}", fonts())
        .replace("{{redirect}}", REDIRECT)
        .replace("{{nonce}}", state)
        .replace("{{error}}", &error)
        .replace("{{id}}", &escape(typed))
}

/// Launcher icon (Apps tile, icons/icon.svg) for the tab
const FAVICON: &str = "<link rel=\"icon\" href=\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 1024 1024'%3E%3Crect x='80' y='103' width='864' height='864' rx='290' fill='%230f3b42'/%3E%3Crect x='80' y='58' width='864' height='864' rx='290' fill='%239fe0dc' stroke='%230f3b42' stroke-width='37'/%3E%3Cpath d='M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z' transform='translate(248 226) scale(22)' fill='none' stroke='%230f3b42' stroke-width='2.2' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E\">";

/// Nunito (700 to 900), the launcher's font, in the pages: embedded (the listener stops right
/// after the result page, nothing more could be downloaded)
fn fonts() -> &'static str {
    static FONTS: OnceLock<String> = OnceLock::new();
    FONTS.get_or_init(|| {
        let files: [(u16, &[u8]); 3] = [
            (700, include_bytes!("../../node_modules/@fontsource/nunito/files/nunito-latin-700-normal.woff2")),
            (800, include_bytes!("../../node_modules/@fontsource/nunito/files/nunito-latin-800-normal.woff2")),
            (900, include_bytes!("../../node_modules/@fontsource/nunito/files/nunito-latin-900-normal.woff2")),
        ];
        files
            .iter()
            .map(|(weight, bytes)| {
                format!(
                    "@font-face{{font-family:Nunito;font-weight:{weight};src:url(data:font/woff2;base64,{}) format('woff2')}}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            })
            .collect()
    })
}

/// Target of the request ("/callback?code=…"), read up to the end of its first line
fn request_target(stream: &mut TcpStream) -> String {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = Vec::new();
    let mut buf = [0u8; 2048];
    while !request.windows(2).any(|w| w == b"\r\n") && request.len() < 16 * 1024 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    String::from_utf8_lossy(&request).split_whitespace().nth(1).unwrap_or_default().to_string()
}

fn respond(stream: &mut TcpStream, status: &str, html: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
}

fn redirect(stream: &mut TcpStream, url: &str) {
    let _ = write!(stream, "HTTP/1.1 303 See Other\r\nLocation: {url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}

/// Page shown in the browser at the end of the connection, in the launcher's colors (cards
/// of the setup page)
fn page(result: &Result<(), String>) -> String {
    let (title, text, retry) = match result {
        Ok(()) => (
            tr("Spotify est connecté", "Spotify is connected"),
            tr("Pastille pilote maintenant Spotify. Tu peux fermer cet onglet.", "Pastille now controls Spotify. You can close this tab."),
            "",
        ),
        Err(e) => (
            tr("Connexion à Spotify impossible", "Couldn't connect Spotify"),
            e.as_str(),
            tr(
                r#"<p style="margin:8px 0 0;color:rgba(15,59,66,.72)">Pour réessayer&nbsp;: Paramètres de Pastille (la clé à côté de ton nom), section Spotify, «&nbsp;Connecter&nbsp;».</p>"#,
                r#"<p style="margin:8px 0 0;color:rgba(15,59,66,.72)">To try again: Pastille's Settings (the wrench next to your name), Spotify section, “Connect”.</p>"#,
            ),
        ),
    };
    let lang = tr("fr", "en");
    format!(
        r#"<!doctype html><html lang="{lang}"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Pastille</title>{FAVICON}<style>{}</style><body style="margin:0;min-height:100vh;padding:16px;box-sizing:border-box;display:grid;place-items:center;background:#e2f5f4;font-family:Nunito,'Segoe UI',sans-serif;font-weight:700;font-size:15px;line-height:1.5;color:#0f3b42"><div style="max-width:440px;padding:22px 28px;border:2.5px solid #0f3b42;border-radius:24px;background:#fff;box-shadow:0 3px 0 #0f3b42"><h1 style="margin:0 0 6px;font-size:22px;font-weight:900;line-height:1.2">{title}</h1><p style="margin:0">{}</p>{retry}</div></body></html>"#,
        fonts(),
        escape(text)
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Parameters of a URL ("a=1&b=x%20y"), decoded
fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |i: usize| bytes.get(i).and_then(|&b| (b as char).to_digit(16));
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], hex(i + 1), hex(i + 2)) {
            (b'+', ..) => out.push(b' '),
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                i += 2;
            }
            (b, ..) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Form body or URL parameters
fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", crate::shell::encode_query(v))).collect::<Vec<_>>().join("&")
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random(n: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; n];
    unsafe { BCryptGenRandom(None, &mut buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.ok().map_err(|e| e.message())?;
    Ok(buf)
}

/// PKCE challenge: SHA-256 of the verifier, in URL-safe base64
fn challenge(verifier: &str) -> Result<String, String> {
    let mut hash = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, verifier.as_bytes(), &mut hash) }
        .ok()
        .map_err(|e| e.message())?;
    Ok(b64(&hash))
}

/* ── Tokens ── */

/// Failure with the token endpoint
enum TokenError {
    /// Refused by Spotify (authorization revoked, expired token, app deleted…): one has to
    /// reconnect
    Refused(String),
    /// Network, Spotify unavailable…: will retry
    Other(String),
}

impl TokenError {
    fn message(self) -> String {
        match self {
            TokenError::Refused(message) | TokenError::Other(message) => message,
        }
    }
}

fn post_token(params: &[(&str, &str)]) -> Result<Value, TokenError> {
    let (status, body) = http("POST", TOKEN_URL, None, Some((&form(params), "application/x-www-form-urlencoded")))
        .map_err(TokenError::Other)?;
    let json = parse(&body);
    match status {
        200 => Ok(json),
        400 | 401 => Err(TokenError::Refused(api_error(status, &json))),
        _ => Err(TokenError::Other(api_error(status, &json))),
    }
}

/// Keeps the access token (and the new refresh token if there is one). `sent`: the
/// refresh token used; if Spotify was disconnected (or reconnected) during the
/// request, the answer is ignored.
fn store(app: &AppHandle, json: &Value, sent: Option<&str>) -> Result<String, String> {
    let token = json["access_token"].as_str().ok_or(tr("réponse de Spotify sans jeton.", "Spotify's answer has no token."))?.to_string();
    let seconds = json["expires_in"].as_u64().unwrap_or(3600);
    let granted = json["scope"].as_str().map(String::from);
    {
        let spotify = app.state::<Spotify>();
        let mut auth = spotify.auth.lock().unwrap();
        if sent.is_some() && auth.saved.refresh_token.as_deref() != sent {
            return Err(tr("Spotify n'est plus connecté.", "Spotify is no longer connected.").into());
        }
        // One minute of margin
        auth.access = Some((token.clone(), Instant::now() + Duration::from_secs(seconds.saturating_sub(60))));
        if let Some(refresh) = json["refresh_token"].as_str() {
            auth.saved.refresh_token = Some(refresh.to_string());
            save(app, &auth.saved);
        }
        // Granted scopes: without the playlist ones, the page offers to reconnect
        if granted.is_some() {
            auth.granted = granted;
        }
    }
    Ok(token)
}

fn access_token(app: &AppHandle) -> Result<String, String> {
    let spotify = app.state::<Spotify>();
    let cached = || {
        let auth = spotify.auth.lock().unwrap();
        auth.access.as_ref().filter(|(_, until)| Instant::now() < *until).map(|(token, _)| token.clone())
    };
    if let Some(token) = cached() {
        return Ok(token);
    }
    let _one_at_a_time = spotify.refreshing.lock().unwrap();
    if let Some(token) = cached() {
        return Ok(token); // refreshed during the wait
    }
    let (client_id, refresh) = {
        let auth = spotify.auth.lock().unwrap();
        (auth.saved.client_id.trim().to_string(), auth.saved.refresh_token.clone().ok_or(tr("Spotify n'est pas connecté.", "Spotify isn't connected."))?)
    };
    match post_token(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &client_id)]) {
        Ok(json) => store(app, &json, Some(&refresh)),
        // Authorization revoked, app deleted…: the slider goes back to the Windows volume
        Err(TokenError::Refused(e)) => {
            eprintln!("[pastille] Spotify: {e}");
            let expired = tr(
                "Connexion à Spotify expirée : reconnecte-le depuis les Paramètres de Pastille.",
                "Spotify connection expired: reconnect it from Pastille's Settings.",
            );
            disconnect(app, Some(expired.into()));
            Err(expired.into())
        }
        Err(TokenError::Other(e)) => Err(e),
    }
}

/* ── Player: volume, shuffle, repeat ── */

#[derive(Serialize)]
pub struct Volume {
    /// "app": Spotify's own volume (web API); "windows": the mixer's one
    source: &'static str,
    /// Spotify's program for set_app_volume ("windows" source)
    key: String,
    volume: u8,
}

#[derive(Serialize)]
pub struct PlayerState {
    /// None: Spotify connected without an active device (or unreachable), or no audio session
    volume: Option<Volume>,
    /// Shuffle ("off", "on" or "smart": Spotify's Smart Shuffle) and
    /// repeat ("off", "context", "track"): connected only
    shuffle: Option<&'static str>,
    repeat: Option<String>,
    /// Last connection failure, for the notifications
    problem: Option<String>,
    /// Connection older than the playlists (Status::Outdated): "Reconnect" notification
    reconnect: bool,
    /// Not connected (no id, or disconnected): "Connect" notification, Spotify open
    connect: bool,
}

/// Device where Spotify plays (this PC, or the phone through Spotify Connect); None without an active device
struct Player {
    volume: Option<u8>,
    shuffle: &'static str,
    repeat: String,
}

#[tauri::command]
pub async fn spotify_state(app: AppHandle) -> PlayerState {
    crate::blocking(move || {
        let mut state =
            PlayerState { volume: None, shuffle: None, repeat: None, problem: None, reconnect: false, connect: false };
        if connected(&app) {
            match player(&app) {
                Ok(Some(player)) => {
                    state.volume = player.volume.map(|volume| Volume { source: "app", key: String::new(), volume });
                    state.shuffle = Some(player.shuffle);
                    state.repeat = Some(player.repeat);
                }
                Ok(None) => {}
                Err(e) => eprintln!("[pastille] Spotify: {e}"),
            }
        } else {
            state.volume = crate::com::with_com(|| crate::mixer::spotify_session().ok().flatten())
                .map(|(key, volume)| Volume { source: "windows", key, volume });
        }
        state.problem = app.state::<Spotify>().auth.lock().unwrap().problem.clone();
        // Known after the first token (its scopes): the read above asks for one
        let status = status(&app);
        state.reconnect = status == Status::Outdated;
        state.connect = matches!(status, Status::NoClient | Status::Disconnected);
        state
    })
    .await
    .unwrap_or(PlayerState { volume: None, shuffle: None, repeat: None, problem: None, reconnect: false, connect: false })
}

/// Spotify's own volume (connected only; otherwise the front goes through the mixer)
#[tauri::command]
pub async fn set_spotify_volume(app: AppHandle, level: u8) -> Result<(), String> {
    crate::blocking(move || put(&app, &format!("/me/player/volume?volume_percent={}", level.min(100)))).await?
}

#[tauri::command]
pub async fn set_spotify_shuffle(app: AppHandle, on: bool) -> Result<(), String> {
    crate::blocking(move || put(&app, &format!("/me/player/shuffle?state={on}"))).await?
}

/// `mode`: "off", "context" (the playlist or album) or "track" (the song)
#[tauri::command]
pub async fn set_spotify_repeat(app: AppHandle, mode: String) -> Result<(), String> {
    if !matches!(mode.as_str(), "off" | "context" | "track") {
        return Err(tr(format!("Répétition « {mode} » inconnue."), format!("Unknown repeat \"{mode}\".")));
    }
    crate::blocking(move || put(&app, &format!("/me/player/repeat?state={mode}"))).await?
}

/// Player command without a body (volume, shuffle, repeat)
fn put(app: &AppHandle, path: &str) -> Result<(), String> {
    let token = access_token(app)?;
    let (status, body) = http("PUT", &format!("{API}{path}"), Some(&token), Some(("", "application/json")))?;
    if matches!(status, 200 | 202 | 204) { Ok(()) } else { Err(api_error(status, &parse(&body))) }
}

fn player(app: &AppHandle) -> Result<Option<Player>, String> {
    let token = access_token(app)?;
    let (status, body) = http("GET", &format!("{API}/me/player"), Some(&token), None)?;
    let json = parse(&body);
    match status {
        204 => Ok(None), // no active device
        200 => {
            log_shuffle(&json);
            Ok(Some(Player {
                volume: json["device"]["volume_percent"].as_u64().map(|v| v.min(100) as u8),
                shuffle: shuffle_mode(&json),
                repeat: json["repeat_state"].as_str().unwrap_or("off").to_string(),
            }))
        }
        _ => Err(api_error(status, &json)),
    }
}

/// Smart Shuffle: `smart_shuffle` field of the player state, missing from
/// the documentation; the API can't turn it on (only shuffle on or off)
fn shuffle_mode(json: &Value) -> &'static str {
    match (json["shuffle_state"].as_bool(), json["smart_shuffle"].as_bool()) {
        (_, Some(true)) => "smart",
        (Some(true), _) => "on",
        _ => "off",
    }
}

/// Development: the shuffle fields as Spotify gives them, at each change
/// (checking `smart_shuffle` on Funsaiki's PC)
fn log_shuffle(json: &Value) {
    static LAST: Mutex<String> = Mutex::new(String::new());
    if !cfg!(debug_assertions) {
        return;
    }
    let now = format!("shuffle_state={} smart_shuffle={}", json["shuffle_state"], json["smart_shuffle"]);
    let mut last = LAST.lock().unwrap();
    if *last != now {
        eprintln!("[pastille] Spotify: {now}");
        *last = now;
    }
}

/* ── Playlists: auto "spotify" group of apps.json (config.rs) ── */

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    pub name: String,
    /// "spotify:playlist:…", or "spotify:user:…:collection" for the liked songs
    pub uri: String,
}

/// Liked Songs then Funsaiki's playlists (MAX_PLAYLISTS at most). Empty without a connection;
/// None if Spotify is unreachable (the group then keeps the previous list).
pub fn playlists(app: &AppHandle) -> Option<Vec<Playlist>> {
    if !connected(app) {
        return Some(Vec::new());
    }
    fetch_playlists(app).map_err(|e| eprintln!("[pastille] Spotify, playlists: {e}")).ok()
}

fn fetch_playlists(app: &AppHandle) -> Result<Vec<Playlist>, String> {
    let token = access_token(app)?;
    // The token tells which scopes are granted: connection older than the playlists, Spotify
    // would refuse; the Settings page offers to reconnect
    if status(app) == Status::Outdated {
        return Ok(Vec::new());
    }
    let get = |url: &str| -> Result<Value, String> {
        let (status, body) = http("GET", url, Some(&token), None)?;
        if status == 200 { Ok(parse(&body)) } else { Err(api_error(status, &parse(&body))) }
    };
    let mut list = Vec::new();
    if let Some(user) = get(&format!("{API}/me"))?["id"].as_str() {
        list.push(Playlist { name: tr("Titres likés", "Liked Songs").into(), uri: format!("spotify:user:{user}:collection") });
    }
    let mut next = Some(format!("{API}/me/playlists?limit=50"));
    while let Some(url) = next.take() {
        let page = get(&url)?;
        for item in page["items"].as_array().into_iter().flatten() {
            if let (Some(name), Some(uri)) = (item["name"].as_str(), item["uri"].as_str()) {
                list.push(Playlist { name: name.trim().to_string(), uri: uri.to_string() });
            }
        }
        if list.len() < MAX_PLAYLISTS {
            next = page["next"].as_str().map(String::from);
        }
    }
    list.truncate(MAX_PLAYLISTS);
    Ok(list)
}

/// Target of a playlist of the group: launched by play rather than opened (shell.rs)
pub fn is_playable(target: &str) -> bool {
    target.starts_with("spotify:playlist:") || (target.starts_with("spotify:user:") && target.ends_with(":collection"))
}

/// Plays a playlist (or the liked songs). Spotify open somewhere: right away.
/// Otherwise it opens in this PC's Spotify app and starts as soon as it is ready.
pub fn play(app: &AppHandle, context: &str) -> Result<(), String> {
    if connected(app) && start(app, context, None)? {
        return Ok(());
    }
    // No active device (or not connected): the Spotify app opens on the playlist
    let uri = if context.ends_with(":collection") { "spotify:collection:tracks" } else { context };
    crate::shell::open_as_is(uri)?;
    if connected(app) {
        let (app, context) = (app.clone(), context.to_string());
        std::thread::spawn(move || play_when_ready(&app, &context));
    }
    Ok(())
}

/// Playback started; Ok(false): no active device
fn start(app: &AppHandle, context: &str, device: Option<&str>) -> Result<bool, String> {
    let token = access_token(app)?;
    let url = match device {
        Some(id) => format!("{API}/me/player/play?device_id={}", crate::shell::encode_query(id)),
        None => format!("{API}/me/player/play"),
    };
    let body = serde_json::json!({ "context_uri": context }).to_string();
    let (status, answer) = http("PUT", &url, Some(&token), Some((&body, "application/json")))?;
    match status {
        200 | 202 | 204 => Ok(true),
        404 => Ok(false),
        _ => Err(api_error(status, &parse(&answer))),
    }
}

/// Spotify just opened: waits for it to show up as a device (8 s at most), then starts
/// playback on it
fn play_when_ready(app: &AppHandle, context: &str) {
    for _ in 0..16 {
        std::thread::sleep(Duration::from_millis(500));
        match this_pc(app) {
            Ok(Some(device)) => {
                if let Err(e) = start(app, context, Some(&device)) {
                    eprintln!("[pastille] Spotify: {e}");
                }
                return;
            }
            Ok(None) => {}
            Err(e) => return eprintln!("[pastille] Spotify: {e}"),
        }
    }
}

/// This PC's Spotify device: a computer, preferably with the PC's name
fn this_pc(app: &AppHandle) -> Result<Option<String>, String> {
    let token = access_token(app)?;
    let (status, body) = http("GET", &format!("{API}/me/player/devices"), Some(&token), None)?;
    let json = parse(&body);
    if status != 200 {
        return Err(api_error(status, &json));
    }
    let name = std::env::var("COMPUTERNAME").unwrap_or_default();
    let computers: Vec<&Value> =
        json["devices"].as_array().into_iter().flatten().filter(|d| d["type"] == "Computer").collect();
    let device = computers
        .iter()
        .find(|d| d["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(&name)))
        .or(computers.first());
    Ok(device.and_then(|d| d["id"].as_str()).map(String::from))
}

fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

/// Message in Pastille's language for Spotify's errors (API: {"error": {"message", "reason"}};
/// tokens: {"error": "invalid_grant", "error_description"})
fn api_error(status: u16, json: &Value) -> String {
    let reason = json["error"]["reason"].as_str().unwrap_or_default();
    let message =
        json["error"]["message"].as_str().or_else(|| json["error_description"].as_str()).unwrap_or(tr("réponse inattendue", "unexpected answer"));
    match (status, reason) {
        (404, _) | (_, "NO_ACTIVE_DEVICE") => {
            tr("Spotify n'a pas d'appareil actif : lance la lecture.", "Spotify has no active device: start playback.").into()
        }
        // Since 2026, the owner of the Spotify app (everyone has their own) must also have
        // Premium: message without a known `reason`
        (_, "PREMIUM_REQUIRED") => premium(),
        (403, _) if message.to_ascii_lowercase().contains("premium") => premium(),
        (429, _) => tr("Spotify reçoit trop de demandes, réessaie dans un instant.", "Spotify gets too many requests, try again in a moment.").into(),
        _ => {
            let code = json["error"].as_str().map(|c| format!(" [{c}]")).unwrap_or_default();
            tr(format!("Spotify ({status}) : {message}{code}"), format!("Spotify ({status}): {message}{code}"))
        }
    }
}

fn premium() -> String {
    tr("Spotify Premium est nécessaire pour piloter la lecture.", "Spotify Premium is needed to control playback.").into()
}

/* ── HTTP ── */

/// Windows HTTP client, kept from one request to the next (connections reused: the slider
/// responds faster); the COM apartment stays alive (com::keep_mta)
static CLIENT: Mutex<Option<HttpClient>> = Mutex::new(None);

fn client() -> windows::core::Result<HttpClient> {
    let mut client = CLIENT.lock().unwrap();
    if let Some(client) = client.as_ref() {
        return Ok(client.clone());
    }
    let filter = HttpBaseProtocolFilter::new()?;
    filter.CacheControl()?.SetReadBehavior(HttpCacheReadBehavior::NoCache)?;
    let created = HttpClient::Create(&filter)?;
    *client = Some(created.clone());
    Ok(created)
}

/// HTTPS request through the Windows HTTP client (WinRT), without cache, TIMEOUT at most per step.
/// `body`: (content, type: form or JSON)
fn http(method: &str, url: &str, bearer: Option<&str>, body: Option<(&str, &str)>) -> Result<(u16, String), String> {
    let send = || -> windows::core::Result<(u16, String)> {
        let request =
            HttpRequestMessage::Create(&HttpMethod::Create(&HSTRING::from(method))?, &Uri::CreateUri(&HSTRING::from(url))?)?;
        if let Some(token) = bearer {
            request
                .Headers()?
                .SetAuthorization(&HttpCredentialsHeaderValue::CreateFromSchemeWithToken(h!("Bearer"), &HSTRING::from(token))?)?;
        }
        if let Some((content, media_type)) = body {
            request.SetContent(&HttpStringContent::CreateFromStringWithEncodingAndMediaType(
                &HSTRING::from(content),
                UnicodeEncoding::Utf8,
                &HSTRING::from(media_type),
            )?)?;
        }
        let response = wait(client()?.SendRequestAsync(&request)?)?;
        let status = response.StatusCode()?.0 as u16;
        let text = wait(response.Content()?.ReadAsStringAsync()?)?.to_string();
        Ok((status, text))
    };
    crate::com::with_com(send).map_err(|e| tr(format!("Spotify injoignable : {}", e.message().trim_end()), format!("Spotify unreachable: {}", e.message().trim_end())))
}

/// Waits for a WinRT operation, TIMEOUT at most; beyond that, it is canceled
fn wait<T: RuntimeType + 'static, P: RuntimeType + 'static>(
    operation: IAsyncOperationWithProgress<T, P>,
) -> windows::core::Result<T> {
    let deadline = Instant::now() + TIMEOUT;
    while operation.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            let _ = operation.Cancel();
            return Err(windows::core::Error::new(ERROR_TIMEOUT.to_hresult(), tr("pas de réponse au bout de 8 s", "no answer after 8 s")));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    operation.GetResults()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Example from RFC 7636 (appendix B)
    #[test]
    fn pkce_challenge() {
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").unwrap(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let verifier = b64(&random(48).unwrap());
        assert_eq!(verifier.len(), 64);
        assert!(verifier.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }

    #[test]
    fn query_and_form() {
        let params = parse_query("code=AQB_x-1&state=a%2Bb&error=access+denied");
        assert_eq!(params["code"], "AQB_x-1");
        assert_eq!(params["state"], "a+b");
        assert_eq!(params["error"], "access denied");
        assert_eq!(decode("%E9t%C3%A9%"), "\u{fffd}té%");
        assert_eq!(
            form(&[("redirect_uri", REDIRECT), ("scope", "a-b c-d")]),
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A47913%2Fcallback&scope=a-b+c-d"
        );
    }

    #[test]
    fn shuffle_modes() {
        assert_eq!(shuffle_mode(&parse(r#"{"shuffle_state":false}"#)), "off");
        assert_eq!(shuffle_mode(&parse(r#"{"shuffle_state":true,"smart_shuffle":false}"#)), "on");
        assert_eq!(shuffle_mode(&parse(r#"{"shuffle_state":true,"smart_shuffle":true}"#)), "smart");
        assert_eq!(shuffle_mode(&parse(r#"{"shuffle_state":true}"#)), "on", "champ absent");
    }

    #[test]
    fn scopes_and_playable_targets() {
        assert!(has_scopes(&format!("other {SCOPES}")));
        assert!(!has_scopes("user-read-playback-state user-modify-playback-state"), "connection from before the playlists");
        assert!(is_playable("spotify:playlist:37i9dQZF1DX"));
        assert!(is_playable("spotify:user:johnny:collection"));
        assert!(!is_playable("spotify:"), "the Spotify launcher opens, it isn't played");
        assert!(!is_playable("steam://rungameid/1"));
    }

    /// An error in the test client doesn't wait the 3 minutes of WAIT
    const TEST_WAIT: Duration = Duration::from_secs(10);

    /// Spotify's redirect is waited for despite a tab icon, an old tab and a
    /// request that arrives in two pieces (loopback, port chosen by the system)
    #[test]
    fn callback_waits_for_the_right_answer() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::thread::spawn(move || {
            let send = |parts: &[&str]| {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                for part in parts {
                    stream.write_all(part.as_bytes()).unwrap();
                    std::thread::sleep(Duration::from_millis(30));
                }
                let mut answer = String::new();
                let _ = stream.read_to_string(&mut answer);
                answer
            };
            assert!(send(&["GET /favicon.ico HTTP/1.1\r\n\r\n"]).starts_with("HTTP/1.1 404"));
            assert!(send(&["GET /callback?code=vieux&state=ancien HTTP/1.1\r\n\r\n"]).starts_with("HTTP/1.1 400"));
            send(&["GET /callback?code=bon&sta", "te=actuel HTTP/1.1\r\nHost: x\r\n\r\n"])
        });
        let (mut stream, params) =
            wait_for_callback(&listener, "actuel", TEST_WAIT, |stream, _, _| respond(stream, "404 Not Found", "")).unwrap();
        assert_eq!(params["code"], "bon");
        respond(&mut stream, "200 OK", "ok");
        drop(stream);
        assert!(client.join().unwrap().starts_with("HTTP/1.1 200"));
    }

    /// First setup: the setup page (with the right token only), an
    /// id refused then a good one, which goes to Spotify, then Spotify's redirect
    #[test]
    fn setup_page_takes_the_client_id() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let login = Login::new().unwrap();
        let nonce = login.state.clone();
        let client = std::thread::spawn(move || {
            let get = |target: &str| {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                write!(stream, "GET {target} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                let mut answer = String::new();
                let _ = stream.read_to_string(&mut answer);
                answer
            };
            assert!(get("/spotify?n=other").starts_with("HTTP/1.1 400"), "token of another attempt");
            assert!(get(&format!("/ailleurs?n={nonce}")).starts_with("HTTP/1.1 404"));
            let setup = get(&format!("/spotify?n={nonce}"));
            assert!(setup.starts_with("HTTP/1.1 200") && setup.contains(REDIRECT) && setup.contains(&nonce));
            assert!(!setup.contains("{{") && !setup.contains(r#"class="error""#));
            let refused = get(&format!("/client-id?n={nonce}&id=%22abc%3E"));
            assert!(refused.contains(r#"class="error""#) && refused.contains("(là : 5)"));
            assert!(refused.contains(r#"value="&quot;abc&gt;""#), "value escaped");
            assert!(get(&format!("/client-id?n={nonce}&id=")).contains("Colle d'abord"));
            let sent = get(&format!("/client-id?n={nonce}&id=+0123456789ABCDEF0123456789abcdef+"));
            assert!(sent.starts_with("HTTP/1.1 303"));
            assert!(sent.contains("Location: https://accounts.spotify.com/authorize?client_id=0123456789abcdef0123456789abcdef&"));
            assert!(sent.contains(&format!("&state={nonce}")));
            get(&format!("/callback?code=bon&state={nonce}"))
        });
        let mut chosen = None;
        let (mut stream, params) = wait_for_callback(&listener, &login.state, TEST_WAIT, |stream, path, params| {
            if let Some(id) = serve_setup(stream, &login, path, params) {
                chosen = Some(id);
            }
        })
        .unwrap();
        assert_eq!(params["code"], "bon");
        assert_eq!(chosen.as_deref(), Some("0123456789abcdef0123456789abcdef"));
        respond(&mut stream, "200 OK", "ok");
        drop(stream);
        assert!(client.join().unwrap().starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn client_ids() {
        assert_eq!(client_id_of(" 0123456789ABCDEF0123456789abcdef\n").as_deref(), Some("0123456789abcdef0123456789abcdef"));
        assert_eq!(client_id_of("0123456789abcdef0123456789abcde"), None, "31 characters");
        assert_eq!(client_id_of("0123456789abcdef0123456789abcdeg"), None, "not hexadecimal");
        assert_eq!(client_id_of("0123456789abcdef 0123456789abcdef"), None);
    }

    /// Both languages: every placeholder filled in, the page's language set
    #[test]
    fn setup_page_in_both_languages() {
        let english = crate::i18n::in_english(|| setup_page("n", "", None));
        for (page, lang) in [(setup_page("n", "", None), "fr"), (english, "en")] {
            assert!(!page.contains("{{"), "placeholder left in the {lang} page");
            assert!(page.contains(&format!(r#"<html lang="{lang}">"#)));
        }
    }

    /// Preview of the setup and result pages, in French then in English (`-en`), to open in a
    /// browser: cargo test write_setup_page -- --ignored (files in the temp folder)
    #[test]
    #[ignore]
    fn write_setup_page() {
        let dir = std::env::temp_dir();
        let write = |suffix: &str| {
            let file = |name: &str| dir.join(format!("pastille-spotify{name}{suffix}.html"));
            fs::write(file(""), setup_page("token", "", None)).unwrap();
            fs::write(file("-error"), setup_page("token", "abc", Some(&client_id_error("abc")))).unwrap();
            fs::write(file("-ok"), page(&Ok(()))).unwrap();
            fs::write(file("-failed"), page(&Err(tr("autorisation refusée.", "authorization denied.").into()))).unwrap();
        };
        write("");
        crate::i18n::in_english(|| write("-en"));
        eprintln!("{}", dir.display());
    }

    #[test]
    fn saved_file_format() {
        let saved: Saved = serde_json::from_str(r#"{ "clientId": "abc" }"#).unwrap();
        assert_eq!(saved, Saved { client_id: "abc".into(), refresh_token: None });
        assert_eq!(serde_json::to_string(&saved).unwrap(), r#"{"clientId":"abc"}"#);
    }

    #[test]
    fn errors_in_french() {
        let no_device = parse(r#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#);
        assert!(api_error(404, &no_device).contains("pas d'appareil actif"));
        assert!(api_error(400, &parse(r#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#)).contains("invalid_grant"));
        assert!(api_error(500, &Value::Null).contains("500"));
        let owner = parse(r#"{"error":{"status":403,"message":"Active premium subscription required for the owner of the app."}}"#);
        assert!(api_error(403, &owner).contains("Premium"));
    }

    #[test]
    fn page_escapes_the_message() {
        assert!(page(&Err("<b>".into())).contains("&lt;b&gt;"));
    }

    /// Diagnostic: the Windows HTTP client reaches Spotify's API (without a token: 401), twice
    /// with the same client
    /// cargo test real_spotify_api -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_spotify_api() {
        for _ in 0..2 {
            let start = Instant::now();
            let (status, body) = http("GET", &format!("{API}/me/player"), None, None).unwrap();
            eprintln!("{status} in {:?}: {}", start.elapsed(), body.replace(char::is_whitespace, ""));
            assert_eq!(status, 401);
        }
    }
}
