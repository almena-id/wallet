# Publishing to the App Store and Google Play

What the wallet has to comply with, and what it should follow, to be published
in Apple's App Store and in Google Play. The documents below are the
authority: they change, so read them at the source before a submission rather
than trusting a summary of them — including this one.

**Mandatory** means a submission is rejected, or the app removed, without it.
**Recommended** means the store reviews against it or ranks by it, but does not
reject on it alone.

## Apple — iOS and the App Store

| Document | What it is | |
|---|---|---|
| [App Store Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) | The rules every submission is reviewed and accepted against. | Mandatory |
| [App Privacy Details](https://developer.apple.com/app-store/app-privacy-details/) | The declaration, in App Store Connect, of what data the app collects and how it is used. | Mandatory |
| Apple Developer Program License Agreement | The legal agreement for distributing through Apple. Signed inside the account at [developer.apple.com](https://developer.apple.com/account). | Mandatory |
| [Human Interface Guidelines](https://developer.apple.com/design/human-interface-guidelines) | Apple's design and UX practice. | Recommended |

## Google — Android and Google Play

| Document | What it is | |
|---|---|---|
| [Google Play Developer Program Policies](https://play.google.com/about/developer-content-policy/) | The content, security and data policies every app must meet. | Mandatory |
| [Google Play Developer Distribution Agreement](https://play.google.com/about/developer-distribution-agreement.html) | The legal agreement for distributing through Google Play. | Mandatory |
| [Launch checklist](https://support.google.com/googleplay/android-developer/answer/9859152) | Google's own checklist before a release, in the Play Console help. | Recommended |
| [Core app quality guidelines](https://developer.android.com/docs/quality-guidelines/core-app-quality) | The technical quality standards Google recommends. | Recommended |
| [Material Design](https://m3.material.io) | Google's design system, the counterpart of Apple's HIG. | Recommended |

## Where this wallet meets them

Things in this code base that a submission will be asked about, so they are
answered deliberately rather than discovered during review. Keep this list
current when a permission, a data flow or a capability is added.

- **Permissions and their reasons.** Every permission the app asks for needs a
  reason the person reads when it is asked:
  - iOS, in `src-tauri/Info.ios.plist`: the camera (`NSCameraUsageDescription`:
    scanning codes, video calls), the microphone (`NSMicrophoneUsageDescription`:
    calls) and Face ID (`NSFaceIDUsageDescription`). Anything added there must
    be used, and its text must say what for.
  - Android, in the generated `AndroidManifest.xml`: `INTERNET`, the camera
    (`CAMERA`, and `VIBRATE` from the scanner plugin), the microphone and the
    audio route during calls (`RECORD_AUDIO`, `MODIFY_AUDIO_SETTINGS`) and
    notifications (`POST_NOTIFICATIONS`, added by the push plugin), and
    `USE_FULL_SCREEN_INTENT` (`plugins/call`): an incoming call takes the screen.
    Google Play grants it only to calling and alarm apps, from Android 14: the
    Play Console's full-screen intent declaration must say the app makes calls.
- **Calls while closed.** On iOS the app declares the `voip` background mode
  (`Info.ios.plist`) and rings through PushKit and CallKit; App Review checks
  that every VoIP push shows a call, which it does. CallKit is not allowed on
  the China storefront: leave it out of that storefront, or ship a build
  without it there.
- **The iOS privacy manifest.** `src-tauri/PrivacyInfo.xcprivacy`, put into the
  Xcode project by `push/sync.sh`: no tracking, no data types collected, and the
  required-reason APIs the runtime reaches (file timestamps, `C617.1`; the
  uptime clock, `35F9.1`). If App Store Connect answers an upload with
  ITMS-91053, it names the category missing: add it there with its reason.
  What it declares must agree with the privacy details in App Store Connect.
- **Data the app sends off the device** — the input to Apple's privacy details
  and Google's Data safety form:
  - messages go end-to-end encrypted through the mediator, which cannot read
    them (`src-tauri/src/messaging/`);
  - the push token of the device is registered with the mediator
    (`messaging/push.rs`);
  - the name the person chose is sent to each contact; the profile picture
    stays on the device;
  - signing in to (or linking to) an Almena Registry portal sends that portal
    a DID made for it alone and a signature, only when the person accepts the
    request (`src-tauri/src/registry.rs`); nothing else about them;
  - receiving a credential fetches its issuer's DID log first
    (`src-tauri/src/issuers.rs`), and opening Profile → Credentials fetches,
    for each credential held, its issuer's status list and DID log
    (`src-tauri/src/status.rs`):
    plain GETs that send nothing about the person — the list holds every
    credential of that issuer, so reading it does not say which one is theirs
    (the registry sees the device's address asking);
  - applying for a credential sends the issuer, through the registry, the
    answers the person typed and the claims of any credential they chose to
    present — only those the request asks for, shown on the sheet before
    Accept (`registry.rs`, `presentation.rs`); nothing goes without it;
  - a call's audio and video go end-to-end encrypted (DTLS-SRTP) through TURN
    relays: the wallet's own mediator's relay sees the device's IP address, the
    other side sees only that relay (`messaging/call.rs`, `SPEC.md` §3);
  - a call offer is marked as a call on its `forward`, so the callee's
    mediator learns that a call is coming and when (not from whom, nor
    anything in it), and sends a call push to the callee's phone;
  - a backup (Profile → Backup) is a file written only where the person
    chooses in the system's save sheet, sealed so that only their phrase opens
    it; the wallet sends it nowhere;
  - there is no analytics and no advertising.
- **Backups.** On Android the vault's record (`vault.json`) is excluded from the
  cloud backup and from device-to-device transfer (`src-tauri/backup/`): it is
  sealed under a Keystore key that never leaves the phone. Everything else is
  backed up as the system decides; it is sealed under the seed.
- **Encryption.** The wallet implements its own encryption (the vault, and
  DIDComm for messages), so the export-compliance questions in App Store
  Connect apply to it, and so does the encryption declaration Google Play asks
  for in some countries. Answer them before the first upload.
- **Removing the identity.** Signing out removes the identity and everything
  the wallet wrote from the device (Profile → Security). Keep it reachable from
  inside the app. The wallet creates no account of its own; an account at an
  Almena Registry it was linked to belongs to that registry and is deleted
  there, from its portal.
- **Android target API level.** Google Play requires new apps and updates to
  target a recent API level; the project targets API 36
  (`src-tauri/gen/android/app/build.gradle.kts`).
- **Push credentials.** FCM and APNs are configured by the publisher; see
  "Push notifications" in the README.
