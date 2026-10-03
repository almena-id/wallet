//! A copy of what the wallet holds besides its identity, for the day it is
//! restored from its words.
//!
//! The phrase brings back every DID — they are all derived from it — but not
//! what was written down beside them: the mediator, the contacts, the
//! conversations, the credentials, the name and the picture. This puts all of
//! that in one file, sealed exactly as the files on the device are (under the
//! key the seed derives, `m/2'`, with an associated data of its own), so the
//! file is nothing to anybody without the words, and the words open it on any
//! device. Where the file goes is the person's choice, through the system's
//! own save sheet; nothing is sent anywhere.
//!
//! **Restoring adds, it never takes away.** What the device already holds wins
//! over what the file says, and what the file has that the device does not is
//! added: a contact, a message, a credential, or a name or picture where there
//! is none. Restoring the same file twice changes nothing the second time.

use std::collections::BTreeMap;
use std::io::Write as _;

use serde::{Deserialize, Serialize, Serializer};
use tauri::{Runtime, State};
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_fs::{FilePath, FsExt as _, OpenOptions};

use super::conversation::{self, Entry};
use super::state::{self, State as Messaging};
use super::{photo, seed, Gate, MessagingError};
use crate::credentials::{self, Credential};
use crate::identity::Held;

const AAD: &[u8] = b"almena-wallet/backup/1";
/// The extension the save sheet suggests and the open sheet filters by.
const EXTENSION: &str = "almenabackup";

/// Everything in the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contents {
    /// When it was made, in seconds since the epoch.
    made: u64,
    messaging: Messaging,
    /// Each conversation's history, by its id.
    conversations: BTreeMap<String, Vec<Entry>>,
    credentials: Vec<Credential>,
    photo: Option<String>,
}

/// What a restore added.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    pub contacts: usize,
    pub messages: usize,
    pub credentials: usize,
}

/// Why a backup was not made or not restored. A code, as the other errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupError {
    /// The wallet is locked: there is no seed to seal or open with.
    Locked,
    /// The file could not be written or read where the person chose.
    File,
    /// The file is not a backup this identity can open: another identity's,
    /// or not a backup at all.
    NotThisIdentity,
    /// What the device holds could not be read or written.
    Storage,
}

impl BackupError {
    const fn code(self) -> &'static str {
        match self {
            Self::Locked => "backup_locked",
            Self::File => "backup_file",
            Self::NotThisIdentity => "backup_not_this_identity",
            Self::Storage => "backup_storage",
        }
    }
}

impl Serialize for BackupError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl From<MessagingError> for BackupError {
    fn from(error: MessagingError) -> Self {
        match error {
            MessagingError::Locked => Self::Locked,
            _ => Self::Storage,
        }
    }
}

/// Seals what the device holds into a file, asking the person where it goes.
/// `false` when they closed the sheet.
#[tauri::command]
pub async fn backup_export<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> Result<bool, BackupError> {
    let seed = seed(&held)?;
    let bytes = {
        let _guard = gate.0.lock().await;
        state::seal(&gather(&app, &seed)?, &seed, AAD)?
    };
    let name = format!("almena-{}.{EXTENSION}", today());
    let sheet = app.clone();
    let Some(path) = tauri::async_runtime::spawn_blocking(move || {
        sheet
            .dialog()
            .file()
            .set_file_name(name)
            .add_filter("Almena backup", &[EXTENSION])
            .blocking_save_file()
    })
    .await
    .map_err(|_| BackupError::File)?
    else {
        return Ok(false);
    };
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = app
        .fs()
        .open(path, options)
        .map_err(|_| BackupError::File)?;
    file.write_all(&bytes).map_err(|_| BackupError::File)?;
    file.sync_all().map_err(|_| BackupError::File)?;
    Ok(true)
}

/// Adds what a backup file holds to what the device holds, asking the person
/// for the file. `None` when they closed the sheet.
#[tauri::command]
pub async fn backup_import<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> Result<Option<Restored>, BackupError> {
    let seed = seed(&held)?;
    let sheet = app.clone();
    let Some(path) = tauri::async_runtime::spawn_blocking(move || {
        sheet
            .dialog()
            .file()
            .add_filter("Almena backup", &[EXTENSION])
            .blocking_pick_file()
    })
    .await
    .map_err(|_| BackupError::File)?
    else {
        return Ok(None);
    };
    let bytes = read(&app, path)?;
    let from: Contents =
        state::open(&bytes, &seed, AAD).map_err(|_| BackupError::NotThisIdentity)?;

    // Under the gate, so a live delivery does not write between the read and
    // the write and have its message lost.
    let _guard = gate.0.lock().await;
    let (into, restored) = merged(gather(&app, &seed)?, from);
    state::write(&app, &seed, &into.messaging)?;
    for (id, entries) in &into.conversations {
        conversation::replace(&app, &seed, id, entries)?;
    }
    credentials::replace(&app, &seed, &into.credentials)?;
    if into.photo.is_some() {
        photo::write(&app, &seed, into.photo.as_deref())?;
    }
    Ok(Some(restored))
}

/// What the device holds now.
fn gather<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
) -> Result<Contents, MessagingError> {
    let messaging = state::read(app, seed)?;
    let mut conversations = BTreeMap::new();
    for relationship in &messaging.relationships {
        let id = conversation::id(&relationship.ours);
        let entries = conversation::read(app, seed, &id)?;
        if !entries.is_empty() {
            conversations.insert(id, entries);
        }
    }
    Ok(Contents {
        made: almena_didcomm::message::now(),
        conversations,
        credentials: credentials::all(app, seed)?,
        photo: photo::read(app, seed)?,
        messaging,
    })
}

/// `into` with what `from` adds, and how much that was. What `into` already
/// has is kept as it is.
fn merged(mut into: Contents, from: Contents) -> (Contents, Restored) {
    let mut restored = Restored::default();
    if into.messaging.mediation.is_none() {
        into.messaging.mediation = from.messaging.mediation;
    }
    if into.messaging.profile.is_none() {
        into.messaging.profile = from.messaging.profile;
    }
    for relationship in from.messaging.relationships {
        if !into
            .messaging
            .relationships
            .iter()
            .any(|r| r.ours == relationship.ours)
        {
            into.messaging.relationships.push(relationship);
            restored.contacts += 1;
        }
    }
    for (id, entries) in from.conversations {
        let kept = into.conversations.entry(id).or_default();
        for entry in entries {
            if !kept.iter().any(|e| e.id == entry.id) {
                kept.push(entry);
                restored.messages += 1;
            }
        }
        kept.sort_by_key(|e| e.at);
    }
    for credential in from.credentials {
        if !into.credentials.iter().any(|c| c.id == credential.id) {
            into.credentials.push(credential);
            restored.credentials += 1;
        }
    }
    if into.photo.is_none() {
        into.photo = from.photo;
    }
    (into, restored)
}

fn read<R: Runtime>(app: &tauri::AppHandle<R>, path: FilePath) -> Result<Vec<u8>, BackupError> {
    use std::io::Read as _;
    // Read access said out loud: the plugin's options start with none, and on
    // Android an empty mode opens nothing.
    let mut options = OpenOptions::new();
    options.read(true);
    let mut file = app
        .fs()
        .open(path, options)
        .map_err(|_| BackupError::File)?;
    let mut bytes = Vec::new();
    // A backup is small; anything past this is not one.
    (&mut file)
        .take(64 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|_| BackupError::File)?;
    Ok(bytes)
}

/// Today's date, `YYYY-MM-DD` in UTC, for the file's suggested name.
fn today() -> String {
    let days = almena_didcomm::message::now() / 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::state::Relationship;

    fn entry(id: &str, at: u64) -> Entry {
        Entry {
            id: id.into(),
            mine: false,
            content: id.into(),
            at,
            failed: false,
            notice: None,
        }
    }

    fn contents(ours: &[&str], messages: &[(&str, u64)], profile: Option<&str>) -> Contents {
        let mut conversations = BTreeMap::new();
        if let Some(first) = ours.first() {
            conversations.insert(
                conversation::id(first),
                messages.iter().map(|(id, at)| entry(id, *at)).collect(),
            );
        }
        Contents {
            made: 1,
            messaging: Messaging {
                mediation: None,
                profile: profile.map(str::to_owned),
                relationships: ours
                    .iter()
                    .map(|o| Relationship::new((*o).into(), "did:theirs".into(), false))
                    .collect(),
            },
            conversations,
            credentials: Vec::new(),
            photo: None,
        }
    }

    #[test]
    fn restoring_adds_what_is_missing_and_keeps_what_is_there() {
        let device = contents(&["did:a"], &[("m2", 2)], Some("Here"));
        let file = contents(&["did:a", "did:b"], &[("m1", 1), ("m2", 2)], Some("There"));
        let (merged, restored) = merged(device, file);
        assert_eq!(
            restored,
            Restored {
                contacts: 1,
                messages: 1,
                credentials: 0
            }
        );
        assert_eq!(merged.messaging.profile.as_deref(), Some("Here"));
        let history = &merged.conversations[&conversation::id("did:a")];
        assert_eq!(
            history.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["m1", "m2"]
        );
    }

    #[test]
    fn restoring_the_same_file_twice_adds_nothing_the_second_time() {
        let file = contents(&["did:a"], &[("m1", 1)], Some("There"));
        let (once, _) = merged(Contents::default(), file.clone());
        let (_, again) = merged(once, file);
        assert_eq!(again, Restored::default());
    }

    #[test]
    fn a_backup_opens_only_with_the_seed_it_was_sealed_with() {
        let file = contents(&["did:a"], &[("m1", 1)], None);
        let sealed = state::seal(&file, &[1u8; 64], AAD).expect("sealed");
        assert_eq!(
            state::open::<Contents>(&sealed, &[1u8; 64], AAD).expect("opened"),
            file
        );
        assert!(state::open::<Contents>(&sealed, &[2u8; 64], AAD).is_err());
        // Nor as any other sealed file of the same identity.
        assert!(
            state::open::<Contents>(&sealed, &[1u8; 64], b"almena-wallet/messaging/1").is_err()
        );
    }

    #[test]
    fn the_suggested_name_carries_a_real_date() {
        let date = today();
        assert_eq!(date.len(), 10);
        assert!(date.starts_with("20"));
    }
}
