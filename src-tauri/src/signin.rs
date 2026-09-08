//! Answering a sign-in request.
//!
//! What this wallet will fetch, believe and post to is [`crate::platform`]'s
//! business, and so is checking the platform's signature. What is here is the
//! sign-in itself: the claims the platform signs into a request, the shape the
//! approval screen shows, and the two answers a person can give.
//!
//! Nothing is decided here either. Reading a request only prepares what the
//! approval screen shows; the answer goes out when somebody says so, and it is
//! signed with a key that belongs to that verifier alone.

use std::sync::Mutex;

use serde::{Deserialize, Serialize, Serializer};
use tauri::{Manager, State};

use crate::identity::Held;
use crate::platform::{self, Asker, Failure, PLATFORM};

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

impl From<Failure> for SignInError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Unreadable => Self::Unreadable,
            Failure::NotOurs => Self::NotOurs,
            Failure::Unverified => Self::Unverified,
            Failure::Unreachable => Self::Unreachable,
            // A sign-in has nothing that can already have been done, so
            // this cannot arrive. It is still refused rather than ignored.
            Failure::Refused | Failure::Conflict => Self::Refused,
        }
    }
}

impl Serialize for SignInError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
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

    let request_uri = platform::request_uri_of(&link)?;
    log::info!("fetching the request from {request_uri}");

    let token = platform::fetch_text(&request_uri).await?;
    let claims: Claims = platform::verified(&token).await?;

    if claims.iss != PLATFORM {
        log::error!(
            "the request is issued by {}, and this wallet answers {PLATFORM}",
            claims.iss
        );
        return Err(SignInError::NotOurs);
    }
    if !platform::under_platform(&claims.response_uri) {
        log::error!(
            "the request would be answered to {}, which is not under {PLATFORM}",
            claims.response_uri
        );
        return Err(SignInError::NotOurs);
    }
    if claims.exp <= platform::now() {
        // Both clocks, because a device an hour out of step looks exactly like
        // a code somebody was too slow to scan.
        log::error!(
            "the request expired at {} and this device says it is {}",
            claims.exp,
            platform::now()
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

    if request.expires_at <= platform::now() {
        log::error!(
            "accepted too late: the request expired at {} and this device says it is {}",
            request.expires_at,
            platform::now()
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
    let answer = platform::sign_answer(&key, &did, &request.response_uri, &request.nonce);

    log::info!(
        "answering {} at {}",
        request.verifier.did,
        request.response_uri
    );

    platform::post_json(
        &request.response_uri,
        &serde_json::json!({ "id_token": answer }),
    )
    .await?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let request_uri = platform::request_uri_of(&link).expect("a link for this platform");
        let token = platform::fetch_text(&request_uri).await.expect("the request");
        let claims: Claims = platform::verified(&token)
            .await
            .expect("the platform's signature");

        assert_eq!(claims.iss, PLATFORM);
        assert!(platform::under_platform(&claims.response_uri));

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
        let answer = platform::sign_answer(&key, &did, &request.response_uri, &request.nonce);

        platform::post_json(
            &request.response_uri,
            &serde_json::json!({ "id_token": answer }),
        )
        .await
        .expect("the platform to take the answer");
    }
}
