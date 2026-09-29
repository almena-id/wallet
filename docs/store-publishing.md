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
  - iOS, in `src-tauri/Info.ios.plist`: the camera (`NSCameraUsageDescription`)
    and Face ID (`NSFaceIDUsageDescription`). Anything added there must be used,
    and its text must say what for.
  - Android, in the generated `AndroidManifest.xml`: the camera, the microphone
    (`RECORD_AUDIO`) and notifications (`POST_NOTIFICATIONS`, added by the push
    plugin).
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
  - there is no analytics and no advertising.
- **Encryption.** The wallet implements its own encryption (the vault, and
  DIDComm for messages), so the export-compliance questions in App Store
  Connect apply to it, and so does the encryption declaration Google Play asks
  for in some countries. Answer them before the first upload.
- **Removing the identity.** Signing out removes the identity and everything
  the wallet wrote from the device (Profile → Security). Keep it reachable from
  inside the app.
- **Android target API level.** Google Play requires new apps and updates to
  target a recent API level; the project targets API 36
  (`src-tauri/gen/android/app/build.gradle.kts`).
- **Push credentials.** FCM and APNs are configured by the publisher; see
  "Push notifications" in the README.
