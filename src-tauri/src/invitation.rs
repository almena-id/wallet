//! Answering an invitation to an organization.
//!
//! An invitation arrives in a mailbox, as a code somebody can point a camera
//! at, because there is no other way to reach a person the platform has never
//! met: whoever invited them cannot know the identifier this wallet will
//! derive, since that identifier does not exist until this is answered.
//!
//! What the wallet does about it is [`crate::platform`]'s shape, the same one
//! signing in has: a link says where to look, the request behind it is signed,
//! and nothing is decided until somebody says so.
//!
//! **The answer proves two identifiers at once, and this is the only place
//! either is claimed.** One is derived from the organization's own `did:web`
//! and is what that organization will deal with — its issuers, its verifiers,
//! anything they sign about this person — so the same person in two
//! organizations is two identifiers with nothing in common. The other is
//! derived from the console's verifier, and is what an account on the platform
//! is; without it somebody would join an organization they could never sign in
//! to see. Nothing but this device knows the two belong together, and it says
//! so once, here, to the platform that has to write them on the same row.
//!
//! There is nothing to decline. A sign-in has a screen counting down on the
//! other side, and telling it "no" is a kindness; an invitation has a row in a
//! database that expires on its own, and an answer saying no would be a fact
//! about somebody that nobody asked to record.

use std::sync::Mutex;

use serde::{Deserialize, Serialize, Serializer};
use tauri::{Manager, State};

use crate::identity::Held;
use crate::platform::{self, Asker, Failure, PLATFORM};

/// What can go wrong, as codes rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum InvitationError {
    /// The link, or what it pointed at, was not something this wallet reads.
    Unreadable,
    /// It named somewhere other than this wallet's own platform.
    NotOurs,
    /// The platform's signature on the invitation did not hold up.
    Unverified,
    /// The invitation was already past its time, or had been answered.
    Expired,
    /// The platform could not be reached.
    Unreachable,
    /// There is no identity open to answer with.
    NoIdentity,
    /// Asked to answer something that was never read.
    Nothing,
    /// This identity is already in that organization, or already waiting to be.
    AlreadyJoined,
    /// The platform would not take the answer.
    Refused,
}

impl InvitationError {
    const fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "invitation_unreadable",
            Self::NotOurs => "invitation_not_ours",
            Self::Unverified => "invitation_unverified",
            Self::Expired => "invitation_expired",
            Self::Unreachable => "invitation_unreachable",
            Self::NoIdentity => "invitation_no_identity",
            Self::Nothing => "invitation_nothing",
            Self::AlreadyJoined => "invitation_already_joined",
            Self::Refused => "invitation_refused",
        }
    }
}

impl From<Failure> for InvitationError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Unreadable => Self::Unreadable,
            Failure::NotOurs => Self::NotOurs,
            Failure::Unverified => Self::Unverified,
            Failure::Unreachable => Self::Unreachable,
            Failure::Refused => Self::Refused,
            Failure::Conflict => Self::AlreadyJoined,
        }
    }
}

impl Serialize for InvitationError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// What is being offered, as the approval screen shows it.
#[derive(Clone, Serialize, Deserialize)]
pub struct Offered {
    /// `owner` or `member`. A word from the platform's own closed list, said
    /// in the reader's language by the interface.
    pub role: String,
    /// What whoever invited them wrote this place down as, if anything. Shown
    /// so the person can see they are the one who was meant.
    #[serde(default)]
    pub name: Option<String>,
}

/// An invitation that has been read and checked, waiting for somebody to decide.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    /// The organization doing the inviting.
    pub entity: Asker,
    /// The place being offered in it.
    pub offered: Offered,
    /// When the invitation stops being answerable, as seconds since the epoch.
    pub expires_at: u64,
    /// The console, which the interface never shows: it is not who is asking,
    /// it is only where the second key comes from. Kept here so accepting does
    /// not have to be handed it back.
    #[serde(skip)]
    console: Asker,
    /// Never leaves this side: the interface has no use for either and no
    /// business being able to replay them.
    #[serde(skip)]
    nonce: String,
    #[serde(skip)]
    response_uri: String,
}

/// The invitation being shown, held between reading it and answering it.
#[derive(Default)]
pub struct Pending(Mutex<Option<Invitation>>);

/// The claims the platform signs into an invitation.
#[derive(Deserialize)]
struct Claims {
    iss: String,
    entity: Asker,
    console: Asker,
    invitation: Offered,
    nonce: String,
    response_uri: String,
    exp: u64,
}

/// Read an invitation link, check who signed it, and hold what it offers.
///
/// # Errors
///
/// Every way the invitation can fail to be one this wallet may answer:
/// unreadable, addressed elsewhere, unsigned by this platform, or out of time.
/// A code that has already been answered is `Expired` too — the platform
/// answers all of those the same way, deliberately, and a wallet that guessed
/// between them would be inventing the difference.
#[tauri::command]
pub async fn invitation_read(
    link: String,
    pending: State<'_, Pending>,
) -> Result<Invitation, InvitationError> {
    log::info!("reading an invitation; this wallet answers {PLATFORM}");

    let request_uri = platform::request_uri_of(&link)?;
    log::info!("fetching the invitation from {request_uri}");

    let token = platform::fetch_text(&request_uri).await?;
    let claims: Claims = platform::verified(&token).await?;

    if claims.iss != PLATFORM {
        log::error!(
            "the invitation is issued by {}, and this wallet answers {PLATFORM}",
            claims.iss
        );
        return Err(InvitationError::NotOurs);
    }
    if !platform::under_platform(&claims.response_uri) {
        log::error!(
            "the invitation would be answered to {}, which is not under {PLATFORM}",
            claims.response_uri
        );
        return Err(InvitationError::NotOurs);
    }
    if claims.exp <= platform::now() {
        // Both clocks, because a device a day out of step looks exactly like an
        // invitation somebody left sitting in their mail for a week.
        log::error!(
            "the invitation expired at {} and this device says it is {}",
            claims.exp,
            platform::now()
        );
        return Err(InvitationError::Expired);
    }

    // The organization and what it offered, and nothing derived from either.
    // The identifiers this wallet is about to derive are exactly what a line
    // carrying both would undo: this log is a file made to be handed to
    // somebody else.
    log::info!(
        "the invitation checks out: {} ({}) offering {}",
        claims.entity.name,
        claims.entity.did,
        claims.invitation.role
    );

    let invitation = Invitation {
        entity: claims.entity,
        offered: claims.invitation,
        expires_at: claims.exp,
        console: claims.console,
        nonce: claims.nonce,
        response_uri: claims.response_uri,
    };

    *pending
        .0
        .lock()
        .expect("the invitation lock is never held across a panic") = Some(invitation.clone());

    Ok(invitation)
}

/// Answer that the person accepts, under the two identifiers this device
/// derives for them.
///
/// # Errors
///
/// [`InvitationError::Nothing`] when no invitation is being shown,
/// [`InvitationError::NoIdentity`] when there is no identity open,
/// [`InvitationError::AlreadyJoined`] when this identity is already in that
/// organization, and [`InvitationError::Refused`] when the platform will not
/// take the answer.
#[tauri::command]
pub async fn invitation_accept(
    pending: State<'_, Pending>,
    held: State<'_, Held>,
) -> Result<(), InvitationError> {
    let invitation = take(&pending)?;

    if invitation.expires_at <= platform::now() {
        log::error!(
            "accepted too late: the invitation expired at {} and this device says it is {}",
            invitation.expires_at,
            platform::now()
        );
        return Err(InvitationError::Expired);
    }

    let no_identity = || {
        log::error!("there is no identity open to answer with");
        InvitationError::NoIdentity
    };
    // One key for the organization, one for the console, both from the same
    // seed and neither derivable from the other. Nothing is stored to make
    // this work: the path is the digest of the identifier, so the same phrase
    // on a new phone derives exactly the same pair again.
    let entity_key = held.site_key(&invitation.entity.did).ok_or_else(no_identity)?;
    let console_key = held
        .site_key(&invitation.console.did)
        .ok_or_else(no_identity)?;

    // Both identifiers are deliberately absent from every line below. Written
    // beside the organization they name, they would undo the separation the
    // whole design is built on — and written beside each other they would say
    // that one person holds both.
    let entity_did = crate::identity::did_for(&entity_key.verifying_key().to_bytes());
    let console_did = crate::identity::did_for(&console_key.verifying_key().to_bytes());

    let answer = platform::sign_answer(
        &entity_key,
        &entity_did,
        &invitation.response_uri,
        &invitation.nonce,
    );
    let console_answer = platform::sign_answer(
        &console_key,
        &console_did,
        &invitation.response_uri,
        &invitation.nonce,
    );

    log::info!(
        "accepting {}'s invitation at {}",
        invitation.entity.did,
        invitation.response_uri
    );

    platform::post_json(
        &invitation.response_uri,
        &serde_json::json!({ "id_token": answer, "console_token": console_answer }),
    )
    .await?;

    Ok(())
}

/// Forgets an invitation nobody answered.
#[tauri::command]
pub fn invitation_forget(pending: State<'_, Pending>) {
    *pending
        .0
        .lock()
        .expect("the invitation lock is never held across a panic") = None;
}

/// Registers the state an invitation in progress needs.
pub fn manage<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    app.manage(Pending::default());
}

fn take(pending: &State<'_, Pending>) -> Result<Invitation, InvitationError> {
    pending
        .0
        .lock()
        .expect("the invitation lock is never held across a panic")
        .take()
        .ok_or(InvitationError::Nothing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_claims_the_platform_signs_are_read_as_it_writes_them() {
        // The platform's wire format, not this side's. And an organization with
        // no logo at all, which is every organization: a `did:web` names one
        // and carries no image, so the field has to be allowed to be missing
        // rather than merely null.
        let claims: Claims = serde_json::from_str(
            r#"{
                "iss": "https://api.almena.id",
                "client_id": "did:web:almena.id:e1",
                "entity": {"did": "did:web:almena.id:e1", "name": "Universidad"},
                "console": {
                    "did": "did:web:almena.id:vportal",
                    "name": "Almena ID",
                    "logo_url": "https://almena.id/brand/app-icon.png"
                },
                "invitation": {"role": "member", "name": "Ana"},
                "nonce": "n",
                "response_uri": "https://api.almena.id/v1/invitations/responses/c1",
                "exp": 1
            }"#,
        )
        .expect("the claims the platform signs");

        assert_eq!(claims.entity.name, "Universidad");
        assert!(claims.entity.logo_url.is_none());
        assert_eq!(
            claims.console.logo_url.as_deref(),
            Some("https://almena.id/brand/app-icon.png")
        );
        assert_eq!(claims.invitation.name.as_deref(), Some("Ana"));
    }

    #[test]
    fn an_invitation_with_nobody_named_is_still_an_invitation() {
        // Naming the person is the inviter's option, not the platform's
        // requirement: nobody has to be called anything to be invited.
        let claims: Claims = serde_json::from_str(
            r#"{
                "iss": "https://api.almena.id",
                "entity": {"did": "did:web:almena.id:e1", "name": "Universidad"},
                "console": {"did": "did:web:almena.id:vportal", "name": "Almena ID"},
                "invitation": {"role": "owner", "name": null},
                "nonce": "n",
                "response_uri": "https://api.almena.id/v1/invitations/responses/c1",
                "exp": 1
            }"#,
        )
        .expect("the claims the platform signs");

        assert_eq!(claims.invitation.role, "owner");
        assert!(claims.invitation.name.is_none());
    }
}
