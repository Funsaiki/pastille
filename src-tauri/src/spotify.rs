//! Volume interne de Spotify (demande de Funsaiki, compte Premium), par l'API web de Spotify.
//! Connexion une fois, depuis le menu de l'icône (« Connecter Spotify… ») : autorisation dans
//! le navigateur avec PKCE (pas de secret à garder), retour sur une adresse locale (REDIRECT,
//! déclarée dans l'app Spotify de Funsaiki sur developer.spotify.com). L'identifiant de cette app
//! et le jeton de renouvellement sont dans spotify.json, à côté de apps.json.
//! Sans connexion, le curseur de la carte de lecture règle le volume de Spotify dans le
//! mélangeur de Windows (mixer.rs). Les échecs (connexion, autorisation retirée) s'affichent
//! dans les notifications du lanceur (`problem`) : la version installée n'a pas de console.

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
use tauri::{menu::MenuItem, AppHandle, Manager, Wry};
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

const PORT: u16 = 47913;
/// À déclarer telle quelle dans l'app Spotify (Redirect URIs)
pub const REDIRECT: &str = "http://127.0.0.1:47913/callback";
/// Lecture et pilotage du lecteur, playlists (groupe auto « spotify »). Une portée ajoutée
/// demande de se reconnecter (Status::Outdated).
const SCOPES: &str = "user-read-playback-state user-modify-playback-state playlist-read-private playlist-read-collaborative";
/// Playlists dans le groupe, au plus (titres likés compris)
const MAX_PLAYLISTS: usize = 100;
const API: &str = "https://api.spotify.com/v1";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
/// Le temps de se connecter dans le navigateur
const WAIT: Duration = Duration::from_secs(180);
/// Limite d'une requête (join() attendrait sans fin si le réseau ne répond pas)
const TIMEOUT: Duration = Duration::from_secs(8);

/// spotify.json
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Identifiant (Client ID) de l'app Spotify de Funsaiki
    #[serde(default)]
    client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

#[derive(Default)]
pub struct Spotify {
    auth: Mutex<Auth>,
    /// Un renouvellement à la fois : Spotify peut remplacer le jeton de renouvellement, deux
    /// demandes avec l'ancien se gêneraient
    refreshing: Mutex<()>,
    /// Entrée du menu de l'icône, remise à jour à chaque connexion ou déconnexion
    menu: OnceLock<MenuItem<Wry>>,
}

#[derive(Default)]
struct Auth {
    saved: Saved,
    /// Jeton d'accès et fin de sa validité
    access: Option<(String, Instant)>,
    /// Portées accordées, d'après la dernière réponse du point d'accès des jetons
    granted: Option<String>,
    /// Dernier échec (connexion, autorisation retirée), montré dans les notifications
    problem: Option<String>,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("spotify.json"))
}

/// Fichier absent ou illisible : pas de connexion
fn read_saved(app: &AppHandle) -> Saved {
    file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// Au démarrage
pub fn load(app: &AppHandle) -> Spotify {
    Spotify { auth: Mutex::new(Auth { saved: read_saved(app), ..Default::default() }), ..Default::default() }
}

/// « Recharger la config » : relit spotify.json (identifiant ajouté à la main, par exemple)
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
    update_menu(app);
}

/// Fichier temporaire puis renommage : un arrêt en pleine écriture ne perd pas l'identifiant
fn save(app: &AppHandle, saved: &Saved) {
    let Some(path) = file(app) else { return };
    let temp = path.with_extension("json.tmp");
    let written = fs::write(&temp, serde_json::to_string_pretty(saved).unwrap_or_default())
        .and_then(|_| fs::rename(&temp, &path));
    if let Err(e) = written {
        eprintln!("[lanceur] Spotify : {} non enregistré : {e}", path.display());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    /// Pas d'identifiant d'app dans spotify.json
    NoClient,
    Disconnected,
    Connected,
    /// Connecté, mais sans toutes les portées de SCOPES (connexion antérieure à leur ajout)
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

/* ── Menu de l'icône ── */

/// Libellé et état de l'entrée Spotify du menu
pub fn menu_label(app: &AppHandle) -> (&'static str, bool) {
    match status(app) {
        Status::NoClient => ("Connecter Spotify (identifiant à configurer)", false),
        Status::Disconnected => ("Connecter Spotify…", true),
        Status::Connected => ("Déconnecter Spotify", true),
        Status::Outdated => ("Reconnecter Spotify (playlists)…", true),
    }
}

/// Appelée par tray.rs une fois l'entrée créée
pub fn set_menu_item(app: &AppHandle, item: MenuItem<Wry>) {
    let _ = app.state::<Spotify>().menu.set(item);
}

fn update_menu(app: &AppHandle) {
    let (label, enabled) = menu_label(app);
    if let Some(item) = app.state::<Spotify>().menu.get() {
        let _ = item.set_text(label);
        let _ = item.set_enabled(enabled);
    }
}

/// Clic sur la notification « Reconnecter » : comme l'entrée du menu, sans jamais déconnecter
#[tauri::command]
pub fn spotify_reconnect(app: AppHandle) -> Result<(), String> {
    match status(&app) {
        Status::Outdated | Status::Disconnected => {
            toggle(&app);
            Ok(())
        }
        Status::Connected => Err("Spotify est déjà connecté.".into()),
        Status::NoClient => Err("Identifiant de l'app Spotify absent de spotify.json.".into()),
    }
}

/// Clic sur l'entrée : connexion dans le navigateur (hors du fil principal), ou déconnexion
pub fn toggle(app: &AppHandle) {
    match status(app) {
        Status::Connected => disconnect(app, None),
        Status::Disconnected | Status::Outdated => {
            if let Some(item) = app.state::<Spotify>().menu.get() {
                let _ = item.set_text("Connexion à Spotify en cours…");
                let _ = item.set_enabled(false);
            }
            let app = app.clone();
            std::thread::spawn(move || {
                let result = connect(&app);
                if let Err(e) = &result {
                    eprintln!("[lanceur] Spotify : {e}");
                }
                app.state::<Spotify>().auth.lock().unwrap().problem =
                    result.as_ref().err().map(|e| format!("Connexion impossible : {e}"));
                update_menu(&app);
                // Nouvelles portées : les playlists peuvent maintenant être lues
                if result.is_ok() {
                    crate::config::rescan(&app);
                }
            });
        }
        Status::NoClient => update_menu(app),
    }
}

/// `problem` : pourquoi, si ce n'est pas Funsaiki qui a choisi de se déconnecter
fn disconnect(app: &AppHandle, problem: Option<String>) {
    {
        let spotify = app.state::<Spotify>();
        let mut auth = spotify.auth.lock().unwrap();
        auth.saved.refresh_token = None;
        auth.access = None;
        auth.problem = problem;
        save(app, &auth.saved);
    }
    update_menu(app);
}

/* ── Connexion (PKCE) ── */

fn connect(app: &AppHandle) -> Result<(), String> {
    let client_id = app.state::<Spotify>().auth.lock().unwrap().saved.client_id.trim().to_string();
    let verifier = b64(&random(48)?); // 64 caractères
    let state = b64(&random(16)?);
    // Écoute avant d'ouvrir le navigateur : la réponse ne doit pas arriver avant
    let listener = TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|e| format!("adresse de retour {REDIRECT} indisponible ({e})."))?;
    let url = format!(
        "https://accounts.spotify.com/authorize?{}",
        form(&[
            ("client_id", &client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge(&verifier)?),
            ("scope", SCOPES),
            ("state", &state),
        ])
    );
    crate::shell::open_as_is(&url)?;
    let (mut stream, params) = wait_for_callback(&listener, &state)?;
    let result = (|| {
        let code = params.get("code").ok_or_else(|| match params.get("error").map(String::as_str) {
            Some("access_denied") => "autorisation refusée.".to_string(),
            other => format!("autorisation refusée ({}).", other.unwrap_or("sans motif")),
        })?;
        let token = post_token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("client_id", &client_id),
            ("code_verifier", &verifier),
        ])
        .map_err(TokenError::message)?;
        store(app, &token, None).map(|_| ())
    })();
    respond(&mut stream, "200 OK", &page(&result));
    result
}

/// Attend la réponse de Spotify sur /callback avec le bon `state`. Les autres requêtes (icône
/// de l'onglet, ancien onglet d'un essai abandonné) reçoivent une réponse et l'attente continue.
fn wait_for_callback(listener: &TcpListener, state: &str) -> Result<(TcpStream, HashMap<String, String>), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => match request_target(&mut stream).split_once('?') {
                Some(("/callback", query)) => {
                    let params = parse_query(query);
                    if params.get("state").map(String::as_str) == Some(state) {
                        return Ok((stream, params));
                    }
                    let stale = Err("cet onglet vient d'un ancien essai : utilise le dernier ouvert.".to_string());
                    respond(&mut stream, "400 Bad Request", &page(&stale));
                }
                _ => respond(&mut stream, "404 Not Found", ""),
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("pas de réponse de Spotify au bout de 3 minutes.".into())
}

/// Cible de la requête (« /callback?code=… »), lue jusqu'à la fin de sa première ligne
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
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
}

/// Page affichée dans le navigateur à la fin de la connexion, aux couleurs du lanceur
fn page(result: &Result<(), String>) -> String {
    let (title, text) = match result {
        Ok(()) => ("Spotify est connecté", "Le lanceur règle maintenant le volume de Spotify. Tu peux fermer cet onglet."),
        Err(e) => ("Connexion à Spotify impossible", e.as_str()),
    };
    format!(
        r#"<!doctype html><html lang="fr"><meta charset="utf-8"><title>Pastille</title><body style="margin:0;min-height:100vh;display:grid;place-items:center;background:#e2f5f4;font-family:Nunito,'Segoe UI',sans-serif;color:#0f3b42"><div style="max-width:420px;padding:26px 32px;border:3px solid #0f3b42;border-radius:26px;background:#fff;box-shadow:0 4px 0 #0f3b42"><h1 style="margin:0 0 8px;font-size:22px">{title}</h1><p style="margin:0;font-weight:600">{}</p></div></body></html>"#,
        escape(text)
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Paramètres d'une adresse (« a=1&b=x%20y »), décodés
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

/// Corps de formulaire ou paramètres d'adresse
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

/// Défi PKCE : SHA-256 du vérificateur, en base64 pour adresse
fn challenge(verifier: &str) -> Result<String, String> {
    let mut hash = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, verifier.as_bytes(), &mut hash) }
        .ok()
        .map_err(|e| e.message())?;
    Ok(b64(&hash))
}

/* ── Jetons ── */

/// Échec auprès du point d'accès des jetons
enum TokenError {
    /// Refusé par Spotify (autorisation retirée, jeton périmé, app supprimée…) : il faut se
    /// reconnecter
    Refused(String),
    /// Réseau, Spotify indisponible… : on réessaiera
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

/// Garde le jeton d'accès (et le nouveau jeton de renouvellement s'il y en a un). `sent` : le
/// jeton de renouvellement utilisé ; si Spotify a été déconnecté (ou reconnecté) pendant la
/// requête, la réponse est ignorée.
fn store(app: &AppHandle, json: &Value, sent: Option<&str>) -> Result<String, String> {
    let token = json["access_token"].as_str().ok_or("réponse de Spotify sans jeton.")?.to_string();
    let seconds = json["expires_in"].as_u64().unwrap_or(3600);
    let granted = json["scope"].as_str().map(String::from);
    let menu_changes = {
        let spotify = app.state::<Spotify>();
        let mut auth = spotify.auth.lock().unwrap();
        if sent.is_some() && auth.saved.refresh_token.as_deref() != sent {
            return Err("Spotify n'est plus connecté.".into());
        }
        // Une minute de marge
        auth.access = Some((token.clone(), Instant::now() + Duration::from_secs(seconds.saturating_sub(60))));
        if let Some(refresh) = json["refresh_token"].as_str() {
            auth.saved.refresh_token = Some(refresh.to_string());
            save(app, &auth.saved);
        }
        let changes = granted.is_some() && granted != auth.granted;
        if granted.is_some() {
            auth.granted = granted;
        }
        changes
    };
    if menu_changes {
        update_menu(app); // « Reconnecter Spotify (playlists)… » s'il manque des portées
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
        return Ok(token); // renouvelé pendant l'attente
    }
    let (client_id, refresh) = {
        let auth = spotify.auth.lock().unwrap();
        (auth.saved.client_id.trim().to_string(), auth.saved.refresh_token.clone().ok_or("Spotify n'est pas connecté.")?)
    };
    match post_token(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &client_id)]) {
        Ok(json) => store(app, &json, Some(&refresh)),
        // Autorisation retirée, app supprimée… : le curseur repasse au volume de Windows
        Err(TokenError::Refused(e)) => {
            eprintln!("[lanceur] Spotify : {e}");
            let expired = "Connexion à Spotify expirée : reconnecte-le depuis le menu de l'icône du lanceur.";
            disconnect(app, Some(expired.into()));
            Err(expired.into())
        }
        Err(TokenError::Other(e)) => Err(e),
    }
}

/* ── Lecteur : volume, aléatoire, répétition ── */

#[derive(Serialize)]
pub struct Volume {
    /// « app » : volume de Spotify lui-même (API web) ; « windows » : celui du mélangeur
    source: &'static str,
    /// Programme de Spotify pour set_app_volume (source « windows »)
    key: String,
    volume: u8,
}

#[derive(Serialize)]
pub struct PlayerState {
    /// None : Spotify connecté sans appareil actif (ou injoignable), ou pas de session audio
    volume: Option<Volume>,
    /// Aléatoire (« off », « on » ou « smart » : l'aléatoire intelligent de Spotify) et
    /// répétition (« off », « context », « track ») : connecté seulement
    shuffle: Option<&'static str>,
    repeat: Option<String>,
    /// Dernier échec de connexion, pour les notifications
    problem: Option<String>,
    /// Connexion d'avant les playlists (Status::Outdated) : notification « Reconnecter »
    reconnect: bool,
}

/// Appareil où Spotify joue (ce PC, ou le téléphone par Spotify Connect) ; None sans appareil actif
struct Player {
    volume: Option<u8>,
    shuffle: &'static str,
    repeat: String,
}

#[tauri::command]
pub async fn spotify_state(app: AppHandle) -> PlayerState {
    crate::blocking(move || {
        let mut state = PlayerState { volume: None, shuffle: None, repeat: None, problem: None, reconnect: false };
        if connected(&app) {
            match player(&app) {
                Ok(Some(player)) => {
                    state.volume = player.volume.map(|volume| Volume { source: "app", key: String::new(), volume });
                    state.shuffle = Some(player.shuffle);
                    state.repeat = Some(player.repeat);
                }
                Ok(None) => {}
                Err(e) => eprintln!("[lanceur] Spotify : {e}"),
            }
        } else {
            state.volume = crate::com::with_com(|| crate::mixer::spotify_session().ok().flatten())
                .map(|(key, volume)| Volume { source: "windows", key, volume });
        }
        state.problem = app.state::<Spotify>().auth.lock().unwrap().problem.clone();
        // Connu après le premier jeton (ses portées) : la lecture ci-dessus en demande un
        state.reconnect = status(&app) == Status::Outdated;
        state
    })
    .await
    .unwrap_or(PlayerState { volume: None, shuffle: None, repeat: None, problem: None, reconnect: false })
}

/// Volume de Spotify lui-même (connecté seulement ; sinon le front passe par le mélangeur)
#[tauri::command]
pub async fn set_spotify_volume(app: AppHandle, level: u8) -> Result<(), String> {
    crate::blocking(move || put(&app, &format!("/me/player/volume?volume_percent={}", level.min(100)))).await?
}

#[tauri::command]
pub async fn set_spotify_shuffle(app: AppHandle, on: bool) -> Result<(), String> {
    crate::blocking(move || put(&app, &format!("/me/player/shuffle?state={on}"))).await?
}

/// `mode` : « off », « context » (la playlist ou l'album) ou « track » (le titre)
#[tauri::command]
pub async fn set_spotify_repeat(app: AppHandle, mode: String) -> Result<(), String> {
    if !matches!(mode.as_str(), "off" | "context" | "track") {
        return Err(format!("Répétition « {mode} » inconnue."));
    }
    crate::blocking(move || put(&app, &format!("/me/player/repeat?state={mode}"))).await?
}

/// Commande du lecteur sans corps (volume, aléatoire, répétition)
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
        204 => Ok(None), // aucun appareil actif
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

/// Aléatoire intelligent (Smart Shuffle) : champ `smart_shuffle` de l'état du lecteur, absent
/// de la documentation ; l'API ne permet pas de l'activer (seulement aléatoire oui ou non)
fn shuffle_mode(json: &Value) -> &'static str {
    match (json["shuffle_state"].as_bool(), json["smart_shuffle"].as_bool()) {
        (_, Some(true)) => "smart",
        (Some(true), _) => "on",
        _ => "off",
    }
}

/// Développement : les champs de l'aléatoire tels que Spotify les donne, à chaque changement
/// (vérification de `smart_shuffle` chez Funsaiki)
fn log_shuffle(json: &Value) {
    static LAST: Mutex<String> = Mutex::new(String::new());
    if !cfg!(debug_assertions) {
        return;
    }
    let now = format!("shuffle_state={} smart_shuffle={}", json["shuffle_state"], json["smart_shuffle"]);
    let mut last = LAST.lock().unwrap();
    if *last != now {
        eprintln!("[lanceur] Spotify : {now}");
        *last = now;
    }
}

/* ── Playlists : groupe auto « spotify » de apps.json (config.rs) ── */

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    pub name: String,
    /// « spotify:playlist:… », ou « spotify:user:…:collection » pour les titres likés
    pub uri: String,
}

/// Titres likés puis playlists de Funsaiki (MAX_PLAYLISTS au plus). Vide sans connexion ;
/// None si Spotify est injoignable (le groupe garde alors la liste précédente).
pub fn playlists(app: &AppHandle) -> Option<Vec<Playlist>> {
    if !connected(app) {
        return Some(Vec::new());
    }
    fetch_playlists(app).map_err(|e| eprintln!("[lanceur] Spotify, playlists : {e}")).ok()
}

fn fetch_playlists(app: &AppHandle) -> Result<Vec<Playlist>, String> {
    let token = access_token(app)?;
    // Le jeton dit quelles portées sont accordées : connexion d'avant les playlists, Spotify
    // refuserait ; le menu de l'icône propose de se reconnecter
    if status(app) == Status::Outdated {
        return Ok(Vec::new());
    }
    let get = |url: &str| -> Result<Value, String> {
        let (status, body) = http("GET", url, Some(&token), None)?;
        if status == 200 { Ok(parse(&body)) } else { Err(api_error(status, &parse(&body))) }
    };
    let mut list = Vec::new();
    if let Some(user) = get(&format!("{API}/me"))?["id"].as_str() {
        list.push(Playlist { name: "Titres likés".into(), uri: format!("spotify:user:{user}:collection") });
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

/// Cible d'une playlist du groupe : lancée par play plutôt qu'ouverte (shell.rs)
pub fn is_playable(target: &str) -> bool {
    target.starts_with("spotify:playlist:") || (target.starts_with("spotify:user:") && target.ends_with(":collection"))
}

/// Joue une playlist (ou les titres likés). Spotify ouvert quelque part : tout de suite.
/// Sinon elle s'ouvre dans l'app Spotify de ce PC et démarre dès qu'il est prêt.
pub fn play(app: &AppHandle, context: &str) -> Result<(), String> {
    if connected(app) && start(app, context, None)? {
        return Ok(());
    }
    // Pas d'appareil actif (ou pas connecté) : l'app Spotify s'ouvre sur la playlist
    let uri = if context.ends_with(":collection") { "spotify:collection:tracks" } else { context };
    crate::shell::open_as_is(uri)?;
    if connected(app) {
        let (app, context) = (app.clone(), context.to_string());
        std::thread::spawn(move || play_when_ready(&app, &context));
    }
    Ok(())
}

/// Lecture lancée ; Ok(false) : pas d'appareil actif
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

/// Spotify vient de s'ouvrir : attend qu'il se présente comme appareil (8 s au plus), puis y
/// lance la lecture
fn play_when_ready(app: &AppHandle, context: &str) {
    for _ in 0..16 {
        std::thread::sleep(Duration::from_millis(500));
        match this_pc(app) {
            Ok(Some(device)) => {
                if let Err(e) = start(app, context, Some(&device)) {
                    eprintln!("[lanceur] Spotify : {e}");
                }
                return;
            }
            Ok(None) => {}
            Err(e) => return eprintln!("[lanceur] Spotify : {e}"),
        }
    }
}

/// Appareil Spotify de ce PC : un ordinateur, de préférence au nom du PC
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

/// Message en français pour les erreurs de Spotify (API : {"error": {"message", "reason"}} ;
/// jetons : {"error": "invalid_grant", "error_description"})
fn api_error(status: u16, json: &Value) -> String {
    let reason = json["error"]["reason"].as_str().unwrap_or_default();
    match (status, reason) {
        (404, _) | (_, "NO_ACTIVE_DEVICE") => "Spotify n'a pas d'appareil actif : lance la lecture.".into(),
        (_, "PREMIUM_REQUIRED") => "Spotify Premium est nécessaire pour piloter la lecture.".into(),
        (429, _) => "Spotify reçoit trop de demandes, réessaie dans un instant.".into(),
        _ => {
            let message = json["error"]["message"]
                .as_str()
                .or_else(|| json["error_description"].as_str())
                .unwrap_or("réponse inattendue");
            let code = json["error"].as_str().map(|c| format!(" [{c}]")).unwrap_or_default();
            format!("Spotify ({status}) : {message}{code}")
        }
    }
}

/* ── HTTP ── */

/// Client HTTP de Windows, gardé d'une requête à l'autre (connexions réutilisées : le curseur
/// répond plus vite) ; l'appartement COM reste en vie (com::keep_mta)
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

/// Requête HTTPS par le client HTTP de Windows (WinRT), sans cache, TIMEOUT au plus par étape.
/// `body` : (contenu, type : formulaire ou JSON)
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
    crate::com::with_com(send).map_err(|e| format!("Spotify injoignable : {}", e.message().trim_end()))
}

/// Attend une opération WinRT, TIMEOUT au plus ; au-delà, elle est annulée
fn wait<T: RuntimeType + 'static, P: RuntimeType + 'static>(
    operation: IAsyncOperationWithProgress<T, P>,
) -> windows::core::Result<T> {
    let deadline = Instant::now() + TIMEOUT;
    while operation.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            let _ = operation.Cancel();
            return Err(windows::core::Error::new(ERROR_TIMEOUT.to_hresult(), "pas de réponse au bout de 8 s"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    operation.GetResults()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exemple de la RFC 7636 (annexe B)
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
        assert!(!has_scopes("user-read-playback-state user-modify-playback-state"), "connexion d'avant les playlists");
        assert!(is_playable("spotify:playlist:37i9dQZF1DX"));
        assert!(is_playable("spotify:user:johnny:collection"));
        assert!(!is_playable("spotify:"), "le lanceur de Spotify s'ouvre, il ne se joue pas");
        assert!(!is_playable("steam://rungameid/1"));
    }

    /// Le retour de Spotify est attendu malgré une icône d'onglet, un ancien onglet et une
    /// requête arrivée en deux morceaux (boucle locale, port choisi par le système)
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
        let (mut stream, params) = wait_for_callback(&listener, "actuel").unwrap();
        assert_eq!(params["code"], "bon");
        respond(&mut stream, "200 OK", "ok");
        drop(stream);
        assert!(client.join().unwrap().starts_with("HTTP/1.1 200"));
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
    }

    #[test]
    fn page_escapes_the_message() {
        assert!(page(&Err("<b>".into())).contains("&lt;b&gt;"));
    }

    /// Diagnostic : le client HTTP de Windows joint l'API de Spotify (sans jeton : 401), deux
    /// fois avec le même client
    /// cargo test real_spotify_api -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_spotify_api() {
        for _ in 0..2 {
            let start = Instant::now();
            let (status, body) = http("GET", &format!("{API}/me/player"), None, None).unwrap();
            eprintln!("{status} en {:?} : {}", start.elapsed(), body.replace(char::is_whitespace, ""));
            assert_eq!(status, 401);
        }
    }
}
