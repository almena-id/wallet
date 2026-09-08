//! The one platform this wallet talks to, and everything that is true of every
//! conversation with it.
//!
//! **The wallet answers one platform and no other.** Everything it will fetch,
//! believe or post to has to sit under [`PLATFORM`], and whatever it is shown
//! has to carry that platform's signature. A link is still only a link: it says
//! where to look, and nothing it says is taken on trust.
//!
//! Two flows come through here — signing in to a verifier, and answering an
//! invitation to an organization — and they differ only in what the platform
//! puts in the claims and what the wallet signs back. The checking is the same
//! checking, so it is written once: a second copy is a second place to get the
//! signature verification right, and one of the two copies is always the one
//! nobody looked at.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ed25519_dalek::Signer as _;
use p256::ecdsa::signature::Verifier as _;
use serde::{Deserialize, Serialize};

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
pub const PLATFORM: &str = env!("ALMENA_PLATFORM_URL");

/// How long an answer this wallet signs stays good for.
pub const ANSWER_SECONDS: u64 = 120;

/// What can go wrong between this wallet and the platform.
///
/// Deliberately not the error either flow reports. A sign-in and an invitation
/// fail in the same ways here and in different ways above — one can be declined
/// and the other cannot, one needs a screen waiting and the other needs a
/// mailbox — and the person reading the message is told which flow they were
/// in. Each module maps this into its own.
///
/// Running out of time is not among these, and that is not an omission: a
/// deadline is in the claims, and how long is too long is the flow's own
/// question. Thirty seconds is generous for a code on a screen and absurd for
/// an invitation somebody will read tomorrow.
#[derive(Debug, Clone, Copy)]
pub enum Failure {
    /// The link, or what it pointed at, was not something this wallet reads.
    Unreadable,
    /// It named somewhere other than this wallet's own platform.
    NotOurs,
    /// The platform's signature did not hold up.
    Unverified,
    /// The platform could not be reached.
    Unreachable,
    /// The platform would not take the answer.
    Refused,
    /// The platform refused because the work was already done. Distinct from
    /// [`Failure::Refused`], because there is nothing wrong and nothing to
    /// retry — and because a flow that cannot produce one still has to say so.
    Conflict,
}

/// Who is asking, as an approval screen shows them.
///
/// Read from the platform's claims and written out to the interface, and the
/// two do not spell this the same way: the platform's wire format is
/// snake_case, the interface's is camelCase. The alias is what bridges them —
/// without it the logo arrives and is quietly dropped.
///
/// The logo is optional twice over: absent because the subject has none, and
/// absent because the subject is a kind that never has one. An organization is
/// the second — it is named by a `did:web` and carries no image — so the field
/// defaults rather than failing to parse.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asker {
    pub did: String,
    pub name: String,
    #[serde(default, alias = "logo_url")]
    pub logo_url: Option<String>,
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

/// This device's idea of the time, in seconds since the epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Whether an address belongs to the platform this wallet answers.
pub fn under_platform(url: &str) -> bool {
    // Sharing a prefix with the platform is not being the platform:
    // `https://api.almena.id.evil.example` starts with our origin and belongs
    // to somebody else, so what follows it has to begin a path, a query or a
    // fragment — or the address has to be the origin itself.
    url.strip_prefix(PLATFORM)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#']))
}

/// The address an `almena://` link points at, if it points at this platform.
pub fn request_uri_of(link: &str) -> Result<String, Failure> {
    let parsed = url::Url::parse(link).map_err(|cause| {
        log::error!("the scanned code is not a link this wallet can read: {cause}");
        Failure::Unreadable
    })?;

    let request_uri = parsed
        .query_pairs()
        .find(|(name, _)| name == "request_uri")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| {
            log::error!("the scanned code carries no request_uri");
            Failure::Unreadable
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
        Err(Failure::NotOurs)
    }
}

pub async fn fetch_text(url: &str) -> Result<String, Failure> {
    let response = reqwest::get(url).await.map_err(|cause| {
        log::error!("could not reach {url}: {cause}");
        Failure::Unreachable
    })?;

    let response = response.error_for_status().map_err(|cause| {
        log::error!("{url} answered {cause}");
        Failure::Unreadable
    })?;

    response.text().await.map_err(|cause| {
        log::error!("{url} answered a body that would not be read: {cause}");
        Failure::Unreadable
    })
}

pub async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, Failure> {
    let response = reqwest::get(url).await.map_err(|cause| {
        log::error!("could not reach {url}: {cause}");
        Failure::Unreachable
    })?;

    response.json().await.map_err(|cause| {
        log::error!("{url} answered something that is not the JSON expected: {cause}");
        Failure::Unreadable
    })
}

/// Post an answer, and say whether the platform took it.
pub async fn post_json(url: &str, body: &serde_json::Value) -> Result<(), Failure> {
    let response = reqwest::Client::new()
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|cause| {
            log::error!("could not reach {url}: {cause}");
            Failure::Unreachable
        })?;

    if response.status().is_success() {
        log::info!("the platform accepted the answer");
        Ok(())
    } else if response.status() == reqwest::StatusCode::CONFLICT {
        log::info!("the platform answered that this was already done");
        Err(Failure::Conflict)
    } else {
        log::error!(
            "the platform answered {} and would not take it",
            response.status()
        );
        Err(Failure::Refused)
    }
}

/// Check the platform's signature on a request, and read what it says.
///
/// The header is decoded before the signature is checked — it has to be, since
/// it names the key — and nothing read that way is believed until the signature
/// over it holds. Generic in the claims because the two flows are told
/// different things and neither of them decides how the checking is done.
pub async fn verified<C: serde::de::DeserializeOwned>(token: &str) -> Result<C, Failure> {
    let parts: Vec<&str> = token.trim().split('.').collect();
    let [encoded_header, encoded_claims, encoded_signature] = parts[..] else {
        return Err(Failure::Unreadable);
    };

    let header: Header =
        serde_json::from_slice(&decode(encoded_header)?).map_err(|_| Failure::Unreadable)?;

    // Pinned: a token that names a weaker algorithm must not be believed for it.
    if header.alg != "ES256" {
        log::error!("the request is signed with {}, not ES256", header.alg);
        return Err(Failure::Unverified);
    }

    let discovery: Discovery =
        fetch_json(&format!("{PLATFORM}/.well-known/openid-configuration")).await?;
    if !under_platform(&discovery.jwks_uri) {
        log::error!(
            "discovery points its keys at {}, which is not under {PLATFORM}",
            discovery.jwks_uri
        );
        return Err(Failure::NotOurs);
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
            Failure::Unverified
        })?;

    let point = p256::EncodedPoint::from_affine_coordinates(
        decode(&jwk.x)?.as_slice().into(),
        decode(&jwk.y)?.as_slice().into(),
        false,
    );
    let key =
        p256::ecdsa::VerifyingKey::from_encoded_point(&point).map_err(|_| Failure::Unverified)?;
    let signature = p256::ecdsa::Signature::from_slice(&decode(encoded_signature)?)
        .map_err(|_| Failure::Unverified)?;

    key.verify(
        format!("{encoded_header}.{encoded_claims}").as_bytes(),
        &signature,
    )
    .map_err(|cause| {
        log::error!("the platform's signature on the request did not hold up: {cause}");
        Failure::Unverified
    })?;

    serde_json::from_slice(&decode(encoded_claims)?).map_err(|_| Failure::Unreadable)
}

pub fn decode(part: &str) -> Result<Vec<u8>, Failure> {
    B64.decode(part).map_err(|_| Failure::Unreadable)
}

/// The self-issued token that says "I hold the key behind this identifier".
pub fn sign_answer(
    key: &ed25519_dalek::SigningKey,
    did: &str,
    audience: &str,
    nonce: &str,
) -> String {
    let issued = now();
    let header = B64.encode(br#"{"alg":"EdDSA","typ":"JWT"}"#);
    let claims = B64.encode(
        serde_json::json!({
            // Self-issued: the wallet is its own provider, so it issues for
            // itself and signs with the very key the identifier names.
            "iss": did,
            "sub": did,
            "aud": audience,
            "nonce": nonce,
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

        assert!(matches!(request_uri_of(link), Err(Failure::NotOurs)));
    }

    #[test]
    fn a_link_that_asks_for_nothing_is_refused() {
        for link in ["almena://signin", "almena://signin?other=1", "not a url"] {
            assert!(request_uri_of(link).is_err(), "{link}");
        }
    }

    #[test]
    fn an_address_that_only_starts_like_ours_is_still_checked() {
        assert!(under_platform(PLATFORM));
        assert!(under_platform(&format!("{PLATFORM}/v1/signin/responses/r1")));
        // A host that merely begins with ours, which is what an attacker can
        // register without asking anybody.
        assert!(!under_platform(&format!("{PLATFORM}.evil.example/v1")));
        assert!(!under_platform(&format!("{PLATFORM}evil.example/v1")));
        assert!(!under_platform("https://evil.example/v1"));
    }
}
