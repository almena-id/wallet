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
    `USE_BIOMETRIC` (`plugins/keystore`, and androidx.biometric's own
    manifest, which also carries the older `USE_FINGERPRINT`): a normal
    permission, granted at install with no dialog, which lets the wallet show
    the system's `BiometricPrompt`. It protects the copy of the vault's data
    key that opens the wallet without the PIN; the wallet never sees a
    fingerprint or a face, only whether the system unlocked its Keystore key.
- **Universal links and App Links.** `https://almena.id/w/…` opens the
  wallet, so no other app can take its links as any can take `almena://`.
  iOS: the Associated Domains capability (`applinks:almena.id`, written into
  the entitlements by the deep-link plugin and `push/ios/wallet_iOS.entitlements`)
  must be on for the App ID `id.almena.wallet` in the developer account, and
  its provisioning profiles made again; `almena.id` serves
  `/.well-known/apple-app-site-association` naming `XDUFSW425R.id.almena.wallet`.
  Android: the intent filter carries `android:autoVerify="true"`, and
  `/.well-known/assetlinks.json` must list the SHA-256 of the certificate the
  installed build is signed with — under Play App Signing, the app signing key
  shown in the Play Console (Setup → App integrity), plus the upload key for
  builds installed outside Play (the API's `REGISTRY_ANDROID_APP_FINGERPRINTS`).
  Until it does, Android does not verify the links and the browser opens them
  (a page offering the `almena://` link), so nothing breaks. Neither sends
  anything off the device: the system fetches those files, not the wallet.
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
    (`src-tauri/src/issuers.rs`), and opening Home (when a status was not read
    in the last ten minutes) or its sync button fetches, for each credential
    held, its issuer's status list and DID log
    (`src-tauri/src/status.rs`):
    plain GETs that send nothing about the person — the list holds every
    credential of that issuer, so reading it does not say which one is theirs
    (the registry sees the device's address asking);
  - applying for a credential sends the issuer, through the registry, the
    answers the person typed and the claims of any credential they chose to
    present — only those the request asks for, shown on the sheet before
    Accept (`registry.rs`, `presentation.rs`); nothing goes without it;
  - **answering a verifier** (`purpose: verify`, a code or link a verifier's
    portal shows): before the sheet is drawn the wallet fetches the
    verifier's DID log and its organisation's from the registry's identity
    domain (`issuers.rs`, `vouch`) — plain GETs that say nothing about the
    person. On Accept, and only then, the registry — which passes it to that
    verifier — receives, for each credential set the verifier asks for, one
    credential the person holds (the one they picked where several would do):
    the issuer's signed credential with the disclosures of **only the claims
    asked for**, shown on the sheet, and a key-binding signature for that
    request's nonce by a key made for that issuer alone (`presentation.rs`).
    The DID it is bound to is the one that issuer already knows; nothing else
    this wallet holds goes, and no identifier common to two verifiers or two
    issuers;
  - **what was presented** is written down on the device (when, to whom,
    which credentials and the names of the claims), sealed like the
    credentials (`presented.rs`), shown on the credential's screen; it is sent
    nowhere, travels only inside a backup the person makes, and goes with the
    identity;
  - a call's audio and video go end-to-end encrypted (DTLS-SRTP) through TURN
    relays: the wallet's own mediator's relay sees the device's IP address, the
    other side sees only that relay — the related address a relayed candidate
    would carry is written as `0.0.0.0` (`messaging/call.rs`, `SPEC.md` §3);
  - a call offer is marked as a call on its `forward`, so the callee's
    mediator learns that a call is coming and when (not from whom, nor
    anything in it), and sends a call push to the callee's phone;
  - a backup (Profile → Backup, or the reminder Home shows once a new
    credential is in no backup yet) is a file written only where the person
    chooses in the system's save sheet, sealed so that only their phrase opens
    it; the wallet sends it nowhere. The reminder's dates are kept on the
    device and sent nowhere;
  - checking the recovery phrase (Profile → Security, after the PIN or the
    device's own check) happens on the device alone: the words typed are
    derived and compared with the open identity, then dropped — never stored,
    logged or sent (`identity::identity_check_phrase`);
  - the language chosen in Profile → Appearance, like the other device
    preferences, is kept on the device and sent nowhere;
  - there is no analytics and no advertising.
- **Backups.** On Android the vault's record (`vault.json`) is excluded from the
  cloud backup and from device-to-device transfer (`src-tauri/backup/`): it is
  sealed under a Keystore key that never leaves the phone. Everything else is
  backed up as the system decides; it is sealed under the seed.
- **Biometrics.** Opening without the PIN is offered only where the system
  itself holds the key behind its own check, and only after the PIN has been
  typed to turn it on (Profile → Security). Nothing biometric is read, stored
  or sent by the wallet: the system answers yes or no and unlocks a key.
  - iOS and macOS: a Keychain item behind `userPresence`; Face ID is declared
    in `NSFaceIDUsageDescription`.
  - Android: a Keystore key (StrongBox where there is one) usable only after
    a strong biometric through `BiometricPrompt`, invalidated when a new
    fingerprint or face is enrolled — the wallet then turns biometrics off and
    says so, and the PIN still opens it. The sealed copy (`vault.biometric`)
    is left out of backups and device transfers like the record
    (`src-tauri/backup/`).
  - Windows: Windows Hello (`KeyCredentialManager`); its key stays in the TPM
    and a wrapped copy of the data key is kept in the Credential Manager. The
    installers declare nothing for it; check again before any MSIX or
    Microsoft Store package.
  - Linux: not offered.
- **The PIN.** On a computer, and on an Android phone whose Keystore would
  not seal the record, a PIN is six digits (`vault/mod.rs`, `shortest`): the
  record can be copied off there and tried offline. An existing four-digit PIN
  is replaced after its next unlock. Nothing about it leaves the device.
- **Encryption.** The wallet implements its own encryption (the vault, and
  DIDComm for messages), so the export-compliance questions in App Store
  Connect apply to it, and so does the encryption declaration Google Play asks
  for in some countries. Answer them before the first upload.
- **Removing the identity.** Signing out removes the identity and everything
  the wallet wrote from the device (Profile → Security), the log of what was
  presented included; the Rust side removes nothing unless the sign-out screen
  confirmed it (`vault_destroy`, `confirm`). Keep it reachable from
  inside the app. The wallet creates no account of its own; an account at an
  Almena Registry it was linked to belongs to that registry and is deleted
  there, from its portal.
- **Android target API level.** Google Play requires new apps and updates to
  target a recent API level; the project targets API 36
  (`src-tauri/gen/android/app/build.gradle.kts`).
- **Push credentials.** FCM and APNs are configured by the publisher; see
  "Push notifications" in the README.
