//! The log, and getting it off the device it was written on.
//!
//! **A phone cannot be attached to a debugger by the person holding it.** When
//! something goes wrong on somebody's own device, the only account of it is what
//! the wallet wrote down, so the log has to be somewhere they can reach and in a
//! shape they can send.
//!
//! Which is why it is not written where a log usually goes. `app_log_dir()` on
//! iOS is `Library/Logs` inside the application's container, and nothing outside
//! the application can open it — not the Files app, not a share sheet, nothing
//! short of a Mac and a cable. The log goes to the container's `Documents`
//! instead, which `UIFileSharingEnabled` and `LSSupportsOpeningDocumentsInPlace`
//! turn into a folder in Files.
//!
//! From there the log leaves the device three ways, and they are not the same
//! thing: a folder somebody browses to and copies out of, a system share sheet
//! somebody sends it from, and a copy written wherever they said to put it.
//! Which of the three a platform has differs — see [`Reach`] — and **Android has
//! only the last two**, because its private storage is not a folder anything
//! outside this application can open.
//!
//! **Nothing is kept for longer than a week.** The log is bounded twice and the
//! two bounds answer different questions: rotation caps what it costs in disk,
//! and [`sweep`] caps how far back it reaches. A wallet opened once a month
//! would otherwise still be carrying its first run around years later, well
//! under any size limit — a size limit says nothing about age.
//!
//! **Nothing secret is ever logged.** Not the phrase, not the seed, not a PIN,
//! not a key, not a nonce. The rule is structural rather than a matter of care
//! at each call site: this log is a file the person can hand to somebody else,
//! and anything in it is something they have handed over.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Serialize, Serializer};
use tauri::{Manager, Runtime};

/// What the log file is called. The plugin adds the extension.
pub const STEM: &str = "almena-wallet";

/// How long a log file is kept. Swept at the start of a run — see [`sweep`].
const RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// What can go wrong, as codes rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum DevelopError {
    /// There is no log directory on this platform, or it could not be resolved.
    NoLogs,
    /// The log is there and would not be read.
    Unreadable,
    /// The export could not be written.
    Storage,
    /// There is no share sheet on this platform.
    NoShare,
    /// The person closed the sheet without sending anything. Not a failure —
    /// it is here so the interface can tell it apart from one.
    ///
    /// Only a share sheet produces it and only a phone has one, so a desktop
    /// build never constructs it. It stays in the list on every platform
    /// because the codes are a contract with the interface, and one that
    /// changed shape per platform would be a worse thing than an unused
    /// variant.
    #[cfg_attr(not(mobile), allow(dead_code))]
    Cancelled,
    /// There is nothing in the log to hand over yet.
    Empty,
}

impl DevelopError {
    const fn code(self) -> &'static str {
        match self {
            Self::NoLogs => "develop_no_logs",
            Self::Unreadable => "develop_unreadable",
            Self::Storage => "develop_storage",
            Self::NoShare => "develop_no_share",
            Self::Cancelled => "develop_cancelled",
            Self::Empty => "develop_empty",
        }
    }
}

impl Serialize for DevelopError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// One of the files the log is spread across.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub bytes: u64,
}

/// What the person holding this device can actually do with the log.
///
/// Three questions and not one, because the answers differ: a phone that can
/// hand the log to another application still cannot show it in a folder, and a
/// Linux desktop that can show it in a folder has no share sheet.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reach {
    /// A file manager opens the directory itself — Files on iOS, Finder,
    /// Explorer, whatever a Linux desktop has. **Not Android**, where the log
    /// is in private storage and nothing outside the application reaches it.
    pub browsable: bool,
    /// There is a system share sheet here to hand the file to.
    pub shareable: bool,
    /// There is a save dialog, so a copy can be put where the person chooses.
    pub savable: bool,
}

/// Where the log is and what is in it, as the Development section shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Logs {
    /// The directory, spelled out: on a phone it is the only way to say where to
    /// look, and on a computer it is what the reveal button opens.
    pub directory: String,
    pub files: Vec<LogFile>,
    pub total_bytes: u64,
    /// What can be done with these files on this device, which decides which
    /// buttons the Development section has any business offering.
    pub reach: Reach,
}

/// Where the log is written.
///
/// The application's own `Documents` on iOS, because that is the one directory
/// inside the container the person holding the phone can open. Everywhere else
/// the ordinary log directory, which on a computer is somewhere a file manager
/// reaches and on Android is private storage that a cable can still read.
pub fn directory<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    let path = if cfg!(target_os = "ios") {
        app.path().document_dir().ok()?.join("logs")
    } else {
        app.path().app_log_dir().ok()?
    };

    fs::create_dir_all(&path).ok()?;
    Some(path)
}

/// Throws away every log file older than [`RETENTION`].
///
/// **The rule is exact and not an approximation, because of what the
/// modification time of a log file is.** It is the moment the last line in it
/// was written, so a file untouched for a week is a file whose *newest* line is
/// a week old and whose every other line is older still. Deleting it throws
/// away nothing that is not already past the limit, and no guess is made about
/// what a file holds.
///
/// The converse is not true and is not claimed. A file written to an hour ago
/// may well open with a line from a month back, and it stays; rotation is what
/// eventually carries those away. A week is a floor under how far the log
/// reaches, not a ceiling.
///
/// Called once, as a run starts, and deliberately not on a timer. **A phone
/// gives an application no time it did not ask for** — iOS may not wake this
/// one for a fortnight, Android will kill it — so a sweep that waited to be
/// woken is a sweep that does not happen. This one costs a `read_dir`, and
/// running it before the log is opened is also the only moment at which the
/// file the plugin is about to write to can be removed without taking the
/// current run's own lines with it.
///
/// Every failure is ignored. A file that would not be deleted is swept on the
/// next run, and refusing to start a wallet over a log file is a worse answer
/// than keeping it a day longer.
pub fn sweep(path: &Path) {
    let now = SystemTime::now();

    for entry in fs::read_dir(path).into_iter().flatten().flatten() {
        if !entry.file_name().to_string_lossy().starts_with(STEM) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        // `duration_since` refuses a file stamped in the future, which a clock
        // put back is enough to produce. That is not older than the retention,
        // so it stays.
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age > RETENTION {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// What this build can offer, decided by the same compile-time switches that
/// decide which plugins are registered rather than by asking the device.
const fn reach() -> Reach {
    Reach {
        // Unchanged, and still false on Android: nothing in this module makes
        // private storage browsable, and saying it did would send somebody
        // looking in an explorer for a folder that is not there.
        browsable: !cfg!(target_os = "android"),
        // Registered only on mobile — see `lib.rs`.
        shareable: cfg!(mobile),
        // The save dialog exists on all five.
        savable: true,
    }
}

/// What the Development section shows about the log.
#[tauri::command(async)]
pub fn develop_logs<R: Runtime>(app: tauri::AppHandle<R>) -> Result<Logs, DevelopError> {
    let path = directory(&app).ok_or(DevelopError::NoLogs)?;
    let mut files = Vec::new();
    let mut total_bytes = 0;

    for entry in fs::read_dir(&path)
        .map_err(|_| DevelopError::Unreadable)?
        .flatten()
    {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(STEM) {
            continue;
        }

        total_bytes += metadata.len();
        files.push(LogFile {
            name,
            bytes: metadata.len(),
        });
    }

    files.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(Logs {
        directory: path.to_string_lossy().into_owned(),
        files,
        total_bytes,
        reach: reach(),
    })
}

/// The moment the export was made, in the one written form that means the same
/// thing to everybody who reads it.
///
/// The same formatting the log builder stamps each line with, so a header and
/// the lines under it can be compared without anyone converting between two
/// notations.
fn now_rfc3339() -> String {
    tauri_plugin_log::TimezoneStrategy::UseUtc
        .get_now()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Everything in the log, oldest first, behind the header the caller handed in.
///
/// **The header is prose and prose is not this module's.** It arrives already
/// written, in the language the person is reading, because this file is one
/// they hand to somebody else and the first thing it should say is what they
/// are handing over. What follows it is machine-readable and English: a
/// version, a platform, a time and a list of files.
///
/// Split from [`gather`] at the directory, so what the export actually says can
/// be read by a test with no application running behind it.
fn gather_from(path: &Path, header: Option<&str>) -> Result<String, DevelopError> {
    // Oldest first, so the export reads forwards. The plugin names a rotated
    // file after the one it replaced, so the plain name is always the newest.
    let mut names: Vec<String> = fs::read_dir(path)
        .map_err(|_| DevelopError::Unreadable)?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(STEM))
        .collect();
    names.sort();
    if let Some(index) = names.iter().position(|name| name == &format!("{STEM}.log")) {
        let newest = names.remove(index);
        names.push(newest);
    }

    let mut body = String::new();
    for name in &names {
        if let Ok(text) = fs::read_to_string(path.join(name)) {
            body.push_str(&format!("──────── {name}\n"));
            body.push_str(&text);
            if !text.ends_with('\n') {
                body.push('\n');
            }
        }
    }

    // Emptiness is decided on the log and not on what is written around it: a
    // header would otherwise make an empty log look like a full one.
    if body.is_empty() {
        return Err(DevelopError::Empty);
    }

    let mut gathered = String::new();
    if let Some(header) = header {
        gathered.push_str(header);
        if !header.ends_with('\n') {
            gathered.push('\n');
        }
        gathered.push('\n');
    }
    gathered.push_str("──────── export\n");
    gathered.push_str(&format!("version: {}\n", env!("CARGO_PKG_VERSION")));
    gathered.push_str(&format!("platform: {}\n", std::env::consts::OS));
    gathered.push_str(&format!("exported: {}\n", now_rfc3339()));
    gathered.push_str(&format!("files: {}\n\n", names.join(", ")));
    gathered.push_str(&body);

    Ok(gathered)
}

/// The same gather, told where to look by the running application.
fn gather<R: Runtime>(
    app: &tauri::AppHandle<R>,
    header: Option<&str>,
) -> Result<String, DevelopError> {
    let path = directory(app).ok_or(DevelopError::NoLogs)?;
    gather_from(&path, header)
}

/// Where the copy the share sheet is holding is put.
///
/// The cache and not `Documents`: it is not a file anybody browses to, and a
/// dated one of them per share would turn the wallet's folder in Files into a
/// pile. On Android it also has to be under `cacheDir`, because that is the one
/// place `file_paths.xml` lets the FileProvider hand out.
#[cfg(mobile)]
fn share_directory<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    let path = app.path().app_cache_dir().ok()?.join("share");
    fs::create_dir_all(&path).ok()?;
    Some(path)
}

/// Hands the log to whatever this device shares files with.
///
/// The sheet is the system's — `UIActivityViewController` on iOS, an
/// `ACTION_SEND` chooser on Android — so where it goes is the person's choice
/// and nothing here has to know about mail, or AirDrop, or a chat application.
///
/// **The copy is named for the moment it was made, and the previous ones are
/// swept first.** The iOS half of the share plugin copies the file into the
/// system temporary directory under its own name and swallows the error when
/// something is already there — so a second share of a file with a stable name
/// sends the first one. Somebody who reproduces a fault and shares again would
/// hand over the log from before it, which is the one outcome this whole
/// feature exists to prevent.
#[cfg(mobile)]
#[tauri::command(async)]
pub fn develop_logs_share<R: Runtime>(
    app: tauri::AppHandle<R>,
    header: Option<String>,
    title: Option<String>,
) -> Result<(), DevelopError> {
    use tauri_plugin_sharekit::{ShareExt, ShareFileOptions};

    let gathered = gather(&app, header.as_deref())?;
    let directory = share_directory(&app).ok_or(DevelopError::NoLogs)?;

    for entry in fs::read_dir(&directory)
        .map_err(|_| DevelopError::Storage)?
        .flatten()
    {
        let _ = fs::remove_file(entry.path());
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    let target = directory.join(format!("{STEM}-log-{stamp}.txt"));
    fs::write(&target, gathered).map_err(|_| DevelopError::Storage)?;

    // A `file://` URL and not a path: the Swift side parses it with
    // `URL(string:)` and the Kotlin side with `Uri.parse`, and neither is given
    // a bare path to guess at. `url` is already a dependency.
    let file = url::Url::from_file_path(&target)
        .map_err(|()| DevelopError::Storage)?
        .to_string();

    let window = app
        .get_webview_window("main")
        .ok_or(DevelopError::NoShare)?;

    app.share()
        .share_file(
            window,
            file,
            ShareFileOptions {
                mime_type: Some("text/plain".to_string()),
                title,
                position: None,
            },
        )
        .map_err(|error| {
            // The plugin answers a dismissed sheet with a rejection like any
            // other. Telling the two apart is a string match, which is as good
            // as the plugin's own interface allows; the day that is not good
            // enough is the day this gets vendored.
            if error.to_string().to_lowercase().contains("cancel") {
                DevelopError::Cancelled
            } else {
                DevelopError::Storage
            }
        })
}

/// A computer shares a file by having it — there is a path, a folder to open it
/// in and a save dialog, and a sheet on top of those would do less than any.
#[cfg(not(mobile))]
#[tauri::command(async)]
pub fn develop_logs_share<R: Runtime>(
    _app: tauri::AppHandle<R>,
    _header: Option<String>,
    _title: Option<String>,
) -> Result<(), DevelopError> {
    Err(DevelopError::NoShare)
}

/// Writes the log where the person said to put it.
///
/// The destination is not chosen here — the interface raised the save dialog
/// and this is told the answer. On a computer and on iOS that answer is a path;
/// on Android it is a `content://` URI naming a document the system created,
/// usually in Downloads, which is exactly where a file explorer looks. `std::fs`
/// cannot open the second, which is the whole reason the filesystem plugin is a
/// dependency: `FsExt::fs().open` takes both.
#[tauri::command(async)]
pub fn develop_logs_save_to<R: Runtime>(
    app: tauri::AppHandle<R>,
    destination: tauri_plugin_fs::FilePath,
    header: Option<String>,
) -> Result<(), DevelopError> {
    use std::io::Write;
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let gathered = gather(&app, header.as_deref())?;

    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);

    let mut file = app
        .fs()
        .open(destination, options)
        .map_err(|_| DevelopError::Storage)?;
    file.write_all(gathered.as_bytes())
        .map_err(|_| DevelopError::Storage)
}

/// The one crate the log accepts anything from, spelled the way `log` spells a
/// target: the library name from `Cargo.toml`, which every module in it is a
/// child of.
///
/// Named here rather than written into the builder in `lib.rs` because it is a
/// rule about this file's contents and because a test has to be able to check
/// that it still names something real — a rename in `Cargo.toml` would turn the
/// allowlist into a filter that matches nothing and silence the log outright.
pub const TARGET: &str = "wallet_lib";

/// What the line saying a run has started actually says.
///
/// Separated from the call that writes it so a test can read the one sentence
/// this crate puts in the log without a logger, a runtime or an application
/// behind it.
fn opening_line(version: &str, os: &str, home: &str) -> String {
    format!("──────── Almena ID wallet {version} started on {os}; identity kept in the {home}")
}

/// Writes the line that says a run has started, so an export read later can be
/// cut at the right place.
///
/// Deliberately dull: a version, a platform and where the identity is kept. It
/// is the first thing anybody debugging asks and the last thing anybody would
/// mind handing over.
pub fn opened(version: &str, os: &str, home: &str) {
    log::info!("{}", opening_line(version, os, home));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-ins for every kind of thing the wallet holds that must never be
    /// written down, each distinctive enough that finding it in a log is proof
    /// and not coincidence.
    ///
    /// The last one is not a secret at all, and it is the one that matters
    /// most: `sign-in.md` refuses to store the map from a verifier to the
    /// identifier used with it, and a single log line carrying both would
    /// rebuild that map inside a file designed to be handed to somebody else.
    const SECRETS: &[&str] = &[
        // The phrase, whole and as the words somebody would grep for.
        "vessel harbour lantern quarry",
        "vessel",
        "harbour",
        "lantern",
        "quarry",
        // The seed the phrase derives, in the two ways this wallet writes bytes.
        "9f3c1ad4be07",
        "3xJ8kQmZpTvB",
        // The four digits in front of it.
        "482915",
        // The key the device holds instead of those digits.
        "ed25519:7Kd2Rm",
        // A verifier and the identifier answered to it, which are only a secret
        // together.
        "verifier.example",
        "did:key:z6MkfTestIdentifier",
    ];

    /// Every place in this crate that can put a line into the log.
    ///
    /// A list and not a count, so the failure names the file somebody added a
    /// line to. Adding a log call is a deliberate act: it means deciding that
    /// what the line says is something the person holding the device would be
    /// content to hand to a stranger, and this is what makes somebody decide it
    /// rather than assume it.
    /// `src/signin.rs` was added deliberately, and this is the reasoning the
    /// comment above asks for. Answering a code is the one thing that reaches
    /// the network from somebody else's phone, and it was the one thing the log
    /// said nothing about: an export taken after a failed sign-in held only the
    /// line saying the wallet had started. What it writes now is where it tried
    /// to go, which platform this build answers to, who was asking, and why it
    /// stopped — all of which is on the screen the person was looking at, or in
    /// the code they photographed. **The identifier derived for the verifier is
    /// not among them**, and must never be: it is the one value that, written
    /// beside the verifier, rebuilds the map `sign-in.md` gives up.
    ///
    /// `src/platform.rs` is where most of what `src/signin.rs` used to write
    /// now lives, and it was read under the same rule: addresses this wallet
    /// tried to reach, the platform it is pinned to, and which check refused a
    /// token. Nothing it can see is a person — it is handed a URL and a
    /// signature and never an identity.
    ///
    /// `src/invitation.rs` writes the organization that invited somebody, its
    /// `did:web`, the role it offered, and where the answer was posted. All of
    /// it is on the screen the person was looking at before they accepted, and
    /// the entity is a public subject with a document anybody can fetch.
    /// **Neither identifier the wallet derives is among them.** That flow
    /// derives two at once, so the rule is stricter there than anywhere else:
    /// written beside the organization, either one undoes the separation; and
    /// written beside each other, the two say that one person holds both,
    /// which is the single fact this design exists to keep off paper.
    const CALL_SITES: &[&str] = &[
        "src/develop.rs",
        "src/invitation.rs",
        "src/lib.rs",
        "src/platform.rs",
        "src/signin.rs",
    ];

    /// A directory of this test's own, since the crate carries no temporary
    /// file helper and one test does not earn a dependency.
    fn scratch(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("almena-develop-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory would not be made");
        path
    }

    /// Every file under `src`, so the sweep below cannot be escaped by putting
    /// a log call in a module nobody remembered to list.
    fn sources(path: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("src would not be read").flatten() {
            let entry = entry.path();
            if entry.is_dir() {
                sources(&entry, found);
            } else if entry.extension().is_some_and(|kind| kind == "rs") {
                found.push(entry);
            }
        }
    }

    /// Nothing secret is ever logged, and this is what says so out loud.
    ///
    /// Two halves, because the claim has two halves. The first gathers an
    /// export over the one line this crate actually writes — produced by the
    /// code that writes it, not typed out here — and searches the result for
    /// every fixture above, header and banner included: a secret must not reach
    /// the file by any of the three routes that assemble it.
    ///
    /// The second is the half that keeps the claim true tomorrow. The rule
    /// holds today only because there are exactly two places in this crate that
    /// can write a line at all, and nothing about that is self-enforcing. So
    /// the sweep fails when a third appears, and whoever added it has to come
    /// here and say that what they are writing down is something the person
    /// holding the device would be content to hand over.
    #[test]
    fn the_export_holds_no_secret() {
        let directory = scratch("secrets");
        fs::write(
            directory.join(format!("{STEM}.log")),
            format!("{}\n", opening_line("0.1.0", "ios", "Keychain")),
        )
        .expect("the fixture log would not be written");

        let header = "Almena ID wallet log, exported on 4 September 2026 at 13:40.";
        let gathered =
            gather_from(&directory, Some(header)).expect("the log would not be gathered");

        for secret in SECRETS {
            assert!(
                !gathered.contains(secret),
                "the export holds {secret}, which is something the wallet must never write down"
            );
        }
        // The fixtures are only worth anything if the export is real, so say so.
        assert!(gathered.contains(header));
        assert!(gathered.contains("Almena ID wallet 0.1.0 started on ios"));

        let _ = fs::remove_dir_all(&directory);

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        sources(&root.join("src"), &mut files);
        files.sort();

        // `println!` and `dbg!` are swept alongside the log macros: the plugin
        // keeps a standard output target, so a line printed is a line logged,
        // and a debug build putting a seed on the console is the same accident
        // under another name.
        let calls = [
            "log::info!",
            "log::warn!",
            "log::error!",
            "log::debug!",
            "log::trace!",
            "println!",
            "eprintln!",
            "dbg!",
        ];

        let mut writers: Vec<String> = files
            .iter()
            .filter(|file| {
                let text = fs::read_to_string(file).unwrap_or_default();
                // Only what ships. A test module writes nothing into anybody's
                // log — and this very sweep spells all eight of these out a few
                // lines above, in a test module, in one of the files it reads.
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                calls.iter().any(|call| code.contains(call))
            })
            .map(|file| {
                file.strip_prefix(&root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        writers.sort();

        let known: Vec<String> = CALL_SITES.iter().map(|site| (*site).to_owned()).collect();
        assert_eq!(
            writers, known,
            "the places this crate can write a line have changed; every one of them has to be a \
             line somebody would be content to hand to a stranger, and the list above says which \
             ones were read"
        );
    }

    /// Stamps a fixture as though nothing had been written to it since.
    fn aged(path: &Path, age: Duration) {
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("the fixture would not be opened")
            .set_modified(SystemTime::now() - age)
            .expect("the fixture would not be stamped")
    }

    /// A week is as far back as the log reaches, and a start is what enforces it.
    ///
    /// The three fixtures are the three answers the sweep has to give: the file
    /// still being written to survives, the rotated one nothing has touched in
    /// over a week does not, and a file that is not this wallet's is not this
    /// wallet's to delete — the log directory is a folder somebody browses on
    /// iOS, and a sweep that took whatever it found there would eventually take
    /// something of theirs.
    #[test]
    fn a_week_is_all_the_log_reaches_back() {
        let directory = scratch("sweep");
        let current = directory.join(format!("{STEM}.log"));
        let rotated = directory.join(format!("{STEM}_2026-01-01.log"));
        let stranger = directory.join("notes.txt");

        for file in [&current, &rotated, &stranger] {
            fs::write(file, "a line\n").expect("the fixture would not be written");
        }
        aged(&current, Duration::from_secs(60 * 60));
        aged(&rotated, RETENTION + Duration::from_secs(60 * 60));
        aged(&stranger, RETENTION * 4);

        sweep(&directory);

        assert!(
            current.exists(),
            "the sweep took the file this run is writing to"
        );
        assert!(
            !rotated.exists(),
            "the log still reaches further back than a week"
        );
        assert!(
            stranger.exists(),
            "the sweep took a file that is not the wallet's"
        );

        let _ = fs::remove_dir_all(&directory);
    }

    /// The log is an allowlist. A dependency that starts calling `log::info!`
    /// must not reach a file that leaves the device, and this fails when the
    /// builder in `lib.rs` stops saying so.
    ///
    /// Read out of the source rather than off the built dispatcher, because
    /// `tauri_plugin_log::Builder` answers no questions about what it was told
    /// — and the thing worth protecting is the decision in `lib.rs`, not a
    /// value this test could just as easily have set itself.
    #[test]
    fn only_this_crate_writes_to_the_log() {
        // The allowlist is only an allowlist while it names something. A rename
        // of `[lib] name` in `Cargo.toml` would leave it matching nothing, and
        // a log with everything filtered out looks exactly like a log nobody
        // wrote to.
        assert!(
            module_path!().starts_with(TARGET),
            "the log allows {TARGET} and this crate is {}",
            module_path!()
        );

        let builder = include_str!("lib.rs");
        assert!(
            builder.contains(".level(log::LevelFilter::Off)"),
            "the log no longer starts closed, so whichever dependency calls log::info! next \
             writes into a file the person hands to somebody else"
        );
        assert!(
            builder.contains("level_for(develop::TARGET, log::LevelFilter::Info)"),
            "the log no longer opens for this crate alone"
        );
        assert_eq!(
            builder.matches("level_for(").count(),
            1,
            "something other than this crate has been let into the log"
        );
    }
}
