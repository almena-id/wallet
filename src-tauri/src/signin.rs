//! Answering a sign-in request.
//!
//! **The wallet answers one platform and no other.** Everything it will fetch,
//! believe or post to has to sit under [`PLATFORM`], and the request itself has
//! to carry that platform's signature. A link is still only a link: it says
//! where to look, and nothing it says is taken on trust.
//!
//! Nothing is decided here either. Reading a request only prepares what the
//! approval screen shows; the answer goes out when somebody says so, and it is
//! signed with a key that belongs to that verifier alone.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use ed25519_dalek::Signer as _;
use p256::ecdsa::signature::Verifier as _;
use serde::{Deserialize, Serialize, Serializer};
use tauri::{Manager, State};

use crate::identity::Held;

/// The one platform this wallet answers to.
///
/// Pinned at build time rather than taken from the link. A wallet that follows
/// whichever address a QR code names is a wallet that can be pointed at a
/// stranger's server, and the person scanning has no way to see the difference.
///
/// The value is decided by `build.rs` and handed to this compilation, so there
/// is no fallback and nothing to be lost: `env!` fails to compile if the build
/// script did not set it. It used to come from `.cargo/config.toml`, which
/// cargo finds by walking up from the directory it was invoked in — and the iOS
/// build invokes it from above this one, so the phone silently got a
/// development address while the desktop got the real one. See `build.rs`.
const PLATFORM: &str = env!("ALMENA_PLATFORM_URL");

/// How long the answer this wallet signs stays good for.
const ANSWER_SECONDS: u64 = 120;

/// What can go wrong, as codes rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum SignInError {
    /// The link, or what it pointed at, was not something this wallet reads.
    Unreadable,
    /// It named somewhere other than this wallet's own platform.
    NotOurs,
    /// The platform's signature on the request did not hold up.
    Unverified,
    /// The request was already past its time.
    Expired,
    /// The platform could not be reached.
    Unreachable,
    /// There is no identity open to answer with.
    NoIdentity,
    /// Asked to answer something that was never read.
    Nothing,
    /// The platform would not take the answer.
    Refused,
}

impl SignInError {
    const fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "signin_unreadable",
            Self::NotOurs => "signin_not_ours",
            Self::Unverified => "signin_unverified",
            Self::Expired => "signin_expired",
            Self::Unreachable => "signin_unreachable",
            Self::NoIdentity => "signin_no_identity",
            Self::Nothing => "signin_nothing",
            Self::Refused => "signin_refused",
        }
    }
}

impl Serialize for SignInError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// Who is asking, as the approval screen shows them.
///
/// Read from the platform's claims and written out to the interface, and the
/// two do not spell this the same way: the platform's wire format is
/// snake_case, the interface's is camelCase. The alias is what bridges them —
/// without it the logo arrives and is quietly dropped.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asker {
    pub did: String,
    pub name: String,
    #[serde(alias = "logo_url")]
    pub logo_url: Option<String>,
}

/// A request that has been read and checked, waiting for somebody to decide.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub verifier: Asker,
    /// When the request stops being answerable, as seconds since the epoch.
    pub expires_at: u64,
    /// Never leaves this side: the interface has no use for it and no business
    /// being able to replay it.
    #[serde(skip)]
    nonce: String,
    #[serde(skip)]
    response_uri: String,
}

/// The request being shown, held between reading it and answering it.
#[derive(Default)]
pub struct Pending(Mutex<Option<Request>>);

/// The claims the platform signs into a request.
#[derive(Deserialize)]
struct Claims {
    iss: String,
    verifier: Asker,
    nonce: String,
    response_uri: String,
    exp: u64,
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: String,
}

#[derive(Deserialize)]
struct Discovery {
    jwks_uri: String,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: String,
    x: String,
    y: String,
}

/// Read a sign-in link, check who signed it, and hold what it asks for.
///
/// # Errors
///
/// Every way the request can fail to be one this wallet may answer: unreadable,
/// addressed elsewhere, unsigned by this platform, or out of time.
#[tauri::command]
pub async fn signin_read(
    link: String,
    pending: State<'_, Pending>,
) -> Result<Request, SignInError> {
    log::info!("reading a sign-in code; this wallet answers {PLATFORM}");

    let request_uri = request_uri_of(&link)?;
    log::info!("fetching the request from {request_uri}");

    let token = fetch_text(&request_uri).await?;
    let claims = verified(&token).await?;

    if claims.iss != PLATFORM {
        log::error!(
            "the request is issued by {}, and this wallet answers {PLATFORM}",
            claims.iss
        );
        return Err(SignInError::NotOurs);
    }
    if !under_platform(&claims.response_uri) {
        log::error!(
            "the request would be answered to {}, which is not under {PLATFORM}",
            claims.response_uri
        );
        return Err(SignInError::NotOurs);
    }
    if claims.exp <= now() {
        // Both clocks, because a device an hour out of step looks exactly like
        // a code somebody was too slow to scan.
        log::error!(
            "the request expired at {} and this device says it is {}",
            claims.exp,
            now()
        );
        return Err(SignInError::Expired);
    }

    // The verifier and nothing derived from it. `sign-in.md` gives up the map
    // from a site to the identifier used with it, and a line carrying both
    // would rebuild it inside a file made to be handed to somebody else.
    log::info!(
        "the request checks out: {} ({})",
        claims.verifier.name,
        claims.verifier.did
    );

    let request = Request {
        verifier: claims.verifier,
        expires_at: claims.exp,
        nonce: claims.nonce,
        response_uri: claims.response_uri,
    };

    *pending
        .0
        .lock()
        .expect("the request lock is never held across a panic") = Some(request.clone());

    Ok(request)
}

/// Answer that the person accepts, with a key that belongs to this verifier
/// alone.
///
/// # Errors
///
/// [`SignInError::Nothing`] when no request is being shown,
/// [`SignInError::NoIdentity`] when there is no identity open, and
/// [`SignInError::Refused`] when the platform will not take the answer.
#[tauri::command]
pub async fn signin_accept(
    pending: State<'_, Pending>,
    held: State<'_, Held>,
) -> Result<(), SignInError> {
    let request = take(&pending)?;

    if request.expires_at <= now() {
        log::error!(
            "accepted too late: the request expired at {} and this device says it is {}",
            request.expires_at,
            now()
        );
        return Err(SignInError::Expired);
    }

    let key = held.site_key(&request.verifier.did).ok_or_else(|| {
        log::error!("there is no identity open to answer with");
        SignInError::NoIdentity
    })?;
    // The identifier this derives is deliberately absent from every line below:
    // it is the one thing that, written beside the verifier, would undo the
    // separation the whole design is built on.
    let did = crate::identity::did_for(&key.verifying_key().to_bytes());
    let answer = sign_answer(&key, &did, &request);

    log::info!(
        "answering {} at {}",
        request.verifier.did,
        request.response_uri
    );

    let response = reqwest::Client::new()
        .post(&request.response_uri)
        .json(&serde_json::json!({ "id_token": answer }))
        .send()
        .await
        .map_err(|cause| {
            log::error!("could not reach {}: {cause}", request.response_uri);
            SignInError::Unreachable
        })?;

    if response.status().is_success() {
        log::info!("the platform accepted the answer");
        Ok(())
    } else {
        log::error!(
            "the platform answered {} and would not take it",
            response.status()
        );
        Err(SignInError::Refused)
    }
}

/// Answer that the person refuses, so the screen they left behind can say so
/// rather than count down to nothing.
///
/// # Errors
///
/// [`SignInError::Nothing`] when no request is being shown.
#[tauri::command]
pub async fn signin_decline(pending: State<'_, Pending>) -> Result<(), SignInError> {
    let request = take(&pending)?;

    log::info!("declining {}", request.verifier.did);

    // Refusing is the person's answer, not a request of their own: if it does
    // not arrive, the sign-in they walked away from times out on its own.
    if let Err(cause) = reqwest::Client::new()
        .post(format!("{}/decline", request.response_uri))
        .send()
        .await
    {
        log::warn!("the refusal did not reach the platform: {cause}");
    }

    Ok(())
}

/// Forgets a request nobody answered.
#[tauri::command]
pub fn signin_forget(pending: State<'_, Pending>) {
    *pending
        .0
        .lock()
        .expect("the request lock is never held across a panic") = None;
}

/// Registers the state a request in progress needs.
pub fn manage<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    app.manage(Pending::default());
}

fn take(pending: &State<'_, Pending>) -> Result<Request, SignInError> {
    pending
        .0
        .lock()
        .expect("the request lock is never held across a panic")
        .take()
        .ok_or(SignInError::Nothing)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn under_platform(url: &str) -> bool {
    // Sharing a prefix with the platform is not being the platform:
    // `https://api.almena.id.evil.example` starts with our origin and belongs
    // to somebody else, so what follows it has to begin a path, a query or a
    // fragment — or the address has to be the origin itself.
    url.strip_prefix(PLATFORM)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
}

/// The address a sign-in link points at, if it points at this platform.
fn request_uri_of(link: &str) -> Result<String, SignInError> {
    let parsed = url::Url::parse(link).map_err(|cause| {
        log::error!("the scanned code is not a link this wallet can read: {cause}");
        SignInError::Unreadable
    })?;

    let request_uri = parsed
        .query_pairs()
        .find(|(name, _)| name == "request_uri")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| {
            log::error!("the scanned code carries no request_uri");
            SignInError::Unreadable
        })?;

    if under_platform(&request_uri) {
        Ok(request_uri)
    } else {
        // The single most useful line this log can carry. A wallet is pinned to
        // one platform at build time, so a code minted by an environment served
        // under any other name is refused — and from the screen that is
        // indistinguishable from the platform being down.
        log::error!(
            "the scanned code names {request_uri}, and this wallet answers {PLATFORM} and nothing else"
        );
        Err(SignInError::NotOurs)
    }
}

async fn fetch_text(url: &str) -> Result<String, SignInError> {
    let response = reqwest::get(url).await.map_err(|cause| {
        log::error!("could not reach {url}: {cause}");
        SignInError::Unreachable
    })?;

    let response = response.error_for_status().map_err(|cause| {
        log::error!("{url} answered {cause}");
        SignInError::Unreadable
    })?;

    response.text().await.map_err(|cause| {
        log::error!("{url} answered a body that would not be read: {cause}");
        SignInError::Unreadable
    })
}

async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, SignInError> {
    let response = reqwest::get(url).await.map_err(|cause| {
        log::error!("could not reach {url}: {cause}");
        SignInError::Unreachable
    })?;

    response.json().await.map_err(|cause| {
        log::error!("{url} answered something that is not the JSON expected: {cause}");
        SignInError::Unreadable
    })
}

/// Check the platform's signature on a request, and read what it says.
///
/// The header is decoded before the signature is checked — it has to be, since
/// it names the key — and nothing read that way is believed until the signature
/// over it holds.
async fn verified(token: &str) -> Result<Claims, SignInError> {
    let parts: Vec<&str> = token.trim().split('.').collect();
    let [encoded_header, encoded_claims, encoded_signature] = parts[..] else {
        return Err(SignInError::Unreadable);
    };

    let header: Header =
        serde_json::from_slice(&decode(encoded_header)?).map_err(|_| SignInError::Unreadable)?;

    // Pinned: a token that names a weaker algorithm must not be believed for it.
    if header.alg != "ES256" {
        log::error!("the request is signed with {}, not ES256", header.alg);
        return Err(SignInError::Unverified);
    }

    let discovery: Discovery =
        fetch_json(&format!("{PLATFORM}/.well-known/openid-configuration")).await?;
    if !under_platform(&discovery.jwks_uri) {
        log::error!(
            "discovery points its keys at {}, which is not under {PLATFORM}",
            discovery.jwks_uri
        );
        return Err(SignInError::NotOurs);
    }

    let jwks: Jwks = fetch_json(&discovery.jwks_uri).await?;
    let jwk = jwks
        .keys
        .iter()
        .find(|key| key.kid == header.kid)
        .ok_or_else(|| {
            log::error!(
                "the request names key {} and the platform published none with that name",
                header.kid
            );
            SignInError::Unverified
        })?;

    let point = p256::EncodedPoint::from_affine_coordinates(
        decode(&jwk.x)?.as_slice().into(),
        decode(&jwk.y)?.as_slice().into(),
        false,
    );
    let key = p256::ecdsa::VerifyingKey::from_encoded_point(&point)
        .map_err(|_| SignInError::Unverified)?;
    let signature = p256::ecdsa::Signature::from_slice(&decode(encoded_signature)?)
        .map_err(|_| SignInError::Unverified)?;

    key.verify(
        format!("{encoded_header}.{encoded_claims}").as_bytes(),
        &signature,
    )
    .map_err(|cause| {
        log::error!("the platform's signature on the request did not hold up: {cause}");
        SignInError::Unverified
    })?;

    serde_json::from_slice(&decode(encoded_claims)?).map_err(|_| SignInError::Unreadable)
}

fn decode(part: &str) -> Result<Vec<u8>, SignInError> {
    B64.decode(part).map_err(|_| SignInError::Unreadable)
}

/// The self-issued token that says "I hold the key behind this identifier".
fn sign_answer(key: &ed25519_dalek::SigningKey, did: &str, request: &Request) -> String {
    let issued = now();
    let header = B64.encode(br#"{"alg":"EdDSA","typ":"JWT"}"#);
    let claims = B64.encode(
        serde_json::json!({
            // Self-issued: the wallet is its own provider, so it issues for
            // itself and signs with the very key the identifier names.
            "iss": did,
            "sub": did,
            "aud": request.response_uri,
            "nonce": request.nonce,
            "iat": issued,
            "exp": issued + ANSWER_SECONDS,
        })
        .to_string(),
    );

    let signed = format!("{header}.{claims}");
    let signature = B64.encode(key.sign(signed.as_bytes()).to_bytes());

    format!("{signed}.{signature}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_pointing_at_this_platform_is_read() {
        let link = format!("almena://signin?request_uri={PLATFORM}/v1/signin/requests/r1");

        assert_eq!(
            request_uri_of(&link).expect("a link for this platform"),
            format!("{PLATFORM}/v1/signin/requests/r1")
        );
    }

    #[test]
    fn a_link_pointing_anywhere_else_is_refused() {
        // The whole reason the platform is pinned: a QR code is written by
        // whoever printed it.
        let link = "almena://signin?request_uri=https://evil.example.com/v1/signin/requests/r1";

        assert!(matches!(request_uri_of(link), Err(SignInError::NotOurs)));
    }

    #[test]
    fn a_link_that_asks_for_nothing_is_refused() {
        for link in ["almena://signin", "almena://signin?other=1", "not a url"] {
            assert!(request_uri_of(link).is_err(), "{link}");
        }
    }

    #[test]
    fn the_claims_the_platform_signs_are_read_as_it_writes_them() {
        // The platform's wire format, not this side's: it says `logo_url`, and
        // a wallet that only reads `logoUrl` shows an approval screen with the
        // verifier's logo missing and no error to explain it.
        let claims: Claims = serde_json::from_str(
            r#"{
                "iss": "https://api.almena.id",
                "verifier": {
                    "did": "did:web:almena.id:portal",
                    "name": "Almena ID",
                    "logo_url": "https://almena.id/brand/app-icon.png"
                },
                "nonce": "n",
                "response_uri": "https://api.almena.id/v1/signin/responses/r1",
                "exp": 1
            }"#,
        )
        .expect("the claims the platform signs");

        assert_eq!(
            claims.verifier.logo_url.as_deref(),
            Some("https://almena.id/brand/app-icon.png")
        );
    }

    #[test]
    fn an_address_that_only_starts_like_ours_is_still_checked() {
        assert!(under_platform(PLATFORM));
        assert!(under_platform(&format!(
            "{PLATFORM}/v1/signin/responses/r1"
        )));
        // A host that merely begins with ours, which is what an attacker can
        // register without asking anybody.
        assert!(!under_platform(&format!("{PLATFORM}.evil.example/v1")));
        assert!(!under_platform(&format!("{PLATFORM}evil.example/v1")));
        assert!(!under_platform("https://evil.example/v1"));
    }
}

/// The one test that leaves the process.
///
/// It answers a real request from a running platform, with a real signature, so
/// that what this wallet builds and what the platform will accept are checked
/// against each other rather than against each other's documentation. Ignored
/// by default: it needs a platform, and a link to answer.
///
/// ```text
/// ALMENA_TEST_LINK='almena://signin?request_uri=…' cargo test -- --ignored
/// ```
#[cfg(test)]
mod interop {
    use super::*;

    #[tokio::test]
    #[ignore = "needs a running platform and a link to answer"]
    async fn a_real_request_is_read_verified_and_answered() {
        let link = std::env::var("ALMENA_TEST_LINK").expect("a link to answer");

        let request_uri = request_uri_of(&link).expect("a link for this platform");
        let token = fetch_text(&request_uri).await.expect("the request");
        let claims = verified(&token).await.expect("the platform's signature");

        assert_eq!(claims.iss, PLATFORM);
        assert!(under_platform(&claims.response_uri));

        let request = Request {
            verifier: claims.verifier,
            expires_at: claims.exp,
            nonce: claims.nonce,
            response_uri: claims.response_uri,
        };

        // A key of no significance: what is under test is the shape of the
        // answer, not where the key came from. Derivation has its own tests.
        let key = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        let did = crate::identity::did_for(&key.verifying_key().to_bytes());

        let response = reqwest::Client::new()
            .post(&request.response_uri)
            .json(&serde_json::json!({ "id_token": sign_answer(&key, &did, &request) }))
            .send()
            .await
            .expect("the platform");

        assert!(
            response.status().is_success(),
            "the platform refused the answer: {}",
            response.status()
        );
    }
}
