# Almena Wallet

The wallet of Almena ID: the app people use to hold their identity and
exchange [DIDComm Messaging v2.0](https://identity.foundation/didcomm-messaging/spec/v2.0/)
messages through an Almena [mediator](https://github.com/almena-id/mediator).
One code base, built with Tauri v2, Vite and React, runs on Android, iOS,
macOS, Linux and Windows.

It is at an early stage: the shell, the look and the build for every platform
are in place, and so is onboarding — creating an identity from a new BIP-39
phrase or bringing one back from an existing phrase, then keeping it on the
device encrypted behind a PIN (six digits on a computer, where the record
could be copied off), which biometrics can stand in for where the system
guards the key itself: Face ID or Touch ID on iOS and a signed macOS build,
a fingerprint or face on Android, Windows Hello on Windows (not on Linux,
which has no standard way to ask) — and connecting to a mediator: the wallet asks
it for mediation and registers an inbox DID derived from the phrase. Contacts
are made with an invitation, as a QR code or a link, and each gets a DID of
its own; with them the wallet exchanges names and text messages, kept on the
device encrypted. While the wallet is open they arrive live, over a WebSocket to the mediator;
while it is not, on Android and iOS the mediator sends a push notification that
says only that something is waiting. Contacts can also call each other, by voice or video, while both
wallets are open; the call is relayed through each wallet's mediator, so
neither learns the other's address ([SPEC.md](SPEC.md)).

## Credentials and verification

The wallet holds verifiable credentials (SD-JWT VCs) that issuers of an
Almena Registry grant, and presents them when asked. Everything goes through
an `almena://auth?request_uri=…` code or link that a registry portal shows;
the wallet reads the request, puts it to the person on a sheet, and does
nothing until Accept:

- **Signing in** to a registry portal, or linking the wallet to an account
  there, with a DID made for that registry alone.
- **Applying** for a credential at catalog.almena.id: pairing with the issuer,
  presenting credentials the form asks for, and signing the answers. Issuers
  send no notices for now, so the wallet says to come back to the catalog to
  see how an application stands and to collect the credential. Once the
  issuer has signed it, the credential is **received** and checked — bound to
  this wallet's key for that issuer, signed by a key the issuer's `did:webvh`
  lists, every disclosure accounted for — and kept on the device, encrypted.
  Home lists what is held, with each credential's status read from its
  issuer's status list.
- **Presenting to a verifier**: a verifier's portal asks for credentials. The
  wallet checks the verifier on its own — its `did:webvh` and its
  organisation's resolve on that registry — and says so, or says it could
  not; then it shows exactly what would go. Only the claims asked for are
  disclosed, each presentation bound to the request; where several held
  credentials would do, the person picks one. What was presented, to whom
  and when is kept on the device and shown on the credential's screen.
- **Signing** (for registry administrators): did:webvh log entries, an
  issuer's credentials and its status lists.

A backup file (Profile → Backup) carries the credentials, the contacts and
the conversations, which the phrase alone does not bring back; Home suggests
one when a new credential is in none yet. The phrase itself cannot be shown
again, but Profile → Security checks whether typed words are this wallet's
phrase, after the PIN or the device. The wallet is read in the system's
language, or in English or Spanish as chosen in Profile → Appearance.
`AGENTS.md` has the details, and
[docs/store-publishing.md](docs/store-publishing.md) what each flow sends.

## Requirements

- Node 22+ with [pnpm](https://pnpm.io), stable Rust and [Task](https://taskfile.dev)
- Desktop: the [Tauri prerequisites](https://tauri.app/start/prerequisites/) of
  the host — WebView2 and the MSVC build tools on Windows,
  `libwebkit2gtk-4.1-dev` and friends on Linux, the Xcode command line tools on
  macOS
- Android: Android Studio (SDK and NDK), a JDK between 17 and 24 — Gradle
  rejects newer ones, including the JDK inside Android Studio
  (`brew install openjdk@17` is enough) — and the Rust targets
  `aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`
- iOS (macOS only): Xcode, CocoaPods and the Rust targets
  `aarch64-apple-ios aarch64-apple-ios-sim`

The Android tasks find the SDK, NDK and a suitable JDK on their own; nothing
needs to be exported.

## Desktop

```bash
task dev        # the app with hot reload
task dev:web    # only the front end, in a browser (no Tauri APIs)
task build      # installers for this host
```

Installers are built on the platform they are for: `.app`/`.dmg` on macOS,
`.deb`/`.rpm`/`.AppImage` on Linux, `.msi`/`.exe` on Windows. They land in
`src-tauri/target/release/bundle`.

### Signed macOS builds

`task build` signs nothing, so it runs anywhere — and macOS keeps an unsigned
wallet out of the keychain that holds the Touch ID key, so there the PIN is
the only way in. Two tasks sign, with what `.env.local` names (see the top of
`Taskfile.yml` for every variable):

```bash
task build:macos     # Apple Development + a development profile: Touch ID on this Mac
task release:macos   # Developer ID + hardened runtime, notarized and stapled: to hand out
```

`release:macos` notarizes with an App Store Connect API key and checks the
`.app` and the `.dmg` with Gatekeeper before it finishes.

## Android

```bash
task deploy:android                # pick a device or emulator, build, install
task deploy:android DEVICE=Pixel   # skip the question
```

Biometric unlock needs a strong (Class 3) fingerprint or face enrolled; an
emulator has one once a fingerprint is added in its extended controls
(`adb -e emu finger touch 1` answers the prompt).

The native project is committed under `src-tauri/gen/android`; `task
init:android` regenerates it. A deploy lists what `adb` can reach — cabled,
over Wi-Fi and emulators — builds only for that target's ABI, and installs the
debug APK with the front end packaged inside.

## iOS

```bash
task deploy:ios                    # pick a device or simulator, build, install
task deploy:ios DEVICE="iPhone 17" # skip the question
```

The native project is committed under `src-tauri/gen/apple`; `task init:ios`
regenerates it. A simulator signs nothing. A physical iPhone needs an Apple
development team, written once into `.env.local` (not committed):

```bash
APPLE_DEVELOPMENT_TEAM=XXXXXXXXXX
```

It is the Team ID under developer.apple.com > Membership; `TEAM=XXXXXXXXXX` on
the command line overrides it.

## On a computer

The wallet lives on the system tray. Closing the window puts it away rather
than ending it — messages keep arriving — and clicking the tray icon (or the
Dock icon on macOS, or Open in the tray's menu) brings it back; that menu is
also where it is quit. A second launch brings the running one to the front
instead of starting another, and the window reopens where it was left. Profile → Security can have it open at
login, minimised and locked, and on Windows open it with Windows Hello once
Hello is set up for the account (turning it on asks Hello twice: once to make
the key, once to sign with it).
Messages that arrive while it is not in front become system notifications,
saying as much as Profile → Notifications allows: everything, who sent it, or
nothing identifying.

## Links and codes

`almena://` links — the wallet's own invitations are written that way — open
the wallet. On a phone the Scan QR tab reads the same invitations, and a
mediator's, from a code. Who tells the system that the wallet opens them:

| Platform | Registered by | Try it |
|---|---|---|
| iOS | `CFBundleURLTypes`, written into the app by the build | `xcrun simctl openurl booted "almena://invite?_oob=…"` |
| Android | an `intent-filter` for the scheme, written by the build | `adb shell am start -a android.intent.action.VIEW -d "almena://invite?_oob=…"` |
| macOS | `CFBundleURLSchemes` in the bundle; macOS learns it once the `.app` has been opened | `open "almena://invite?_oob=…"` |
| Windows | the `.msi`/`.exe` installer (registry); a development build registers itself | open the link from Run (Win+R) or a browser |
| Linux | the `.deb`/`.rpm` desktop entry; an AppImage registers itself at startup (needs `xdg-mime`) | `xdg-open "almena://invite?_oob=…"` |

`task dev` on macOS is not a bundle, so links do not reach it. A link opened
while the wallet runs goes to the running one: the phones deliver it, and on a
computer `single-instance` hands it over.

On a phone the same links are also written `https://almena.id/w/…` — whatever
follows `almena://`, under `/w/` on the identity domain — and that is what the
registry and catalog portals show (link and QR code). Any app may claim a custom
scheme; only this one may claim that host, because `almena.id` names it: iOS
universal links (the `applinks:almena.id` Associated Domain, against
`/.well-known/apple-app-site-association`) and Android verified App Links (an
`autoVerify` intent filter for `https://almena.id/w/`, against
`/.well-known/assetlinks.json`, which must list the SHA-256 of the certificate
the installed build is signed with). The deep-link plugin writes both from
`tauri.conf.json` at every build, however often `gen/` is regenerated, and
`push/ios/wallet_iOS.entitlements` carries the domain too. A camera that scans
such a code opens the wallet; without it, the browser shows a page that offers
the `almena://` link and where to get the wallet. The wallet reads both forms
alike (`src-tauri/src/links.rs`), and only that host and prefix: any other
https link is not a wallet link. A computer has no universal links and keeps
`almena://`.

| Platform | Try it |
|---|---|
| iOS | `xcrun simctl openurl booted "https://almena.id/w/invite?_oob=…"` (the simulator fetches the association file; a build signed with the team whose App ID has Associated Domains on) |
| Android | `adb shell pm verify-app-links --re-verify id.almena.wallet`, then `adb shell pm get-app-links id.almena.wallet` (`verified` once `assetlinks.json` lists the build's fingerprint) and `adb shell am start -a android.intent.action.VIEW -d "https://almena.id/w/invite?_oob=…"` |

## Push notifications

The mediator notifies a phone through Firebase Cloud Messaging (Android) or
APNs (iOS), with the credentials of whoever publishes the app — see the
mediator's `docs/didcomm.md` §5. On the wallet's side:

- **Android** needs the Firebase project's `google-services.json` in
  `src-tauri/gen/android/app/` (not committed). Without it the app builds and
  simply receives no pushes.
- **iOS** needs an App ID with the Push Notifications capability for
  `id.almena.wallet`; the entitlement is already in the project.

The words of the notification are in `src-tauri/push/`, in the app's own
strings, so the phone shows them in its language. `task init:android` and
`task init:ios` put them, the entitlement and the Firebase setup back into the
regenerated projects.

## Branding

Every icon comes from `assets/branding`:

```bash
task icons   # regenerate the desktop and iOS icons and sync the native projects
```

The Android launcher icons under `src-tauri/icons/android` are the exception:
they are layered and drawn by hand, and only ever copied forward.

## Development

`task --list` shows every task. Before sending a change, `task check`
(TypeScript, formatting, clippy, tests) must pass; see
[CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md) for the code
layout.

## Releases

Every merge into `main` builds the wallet for every platform, with a
`year.month.sequence` version (e.g. `2026.10.1`): the Linux, Windows and macOS
installers, signed (and notarized on macOS), become a GitHub release with
signed checksums; the Android and iOS builds are uploaded to Google Play and
App Store Connect. The secrets it needs, and how each one is obtained, are in
[docs/releasing.md](docs/releasing.md).

## Publishing to the stores

What the App Store and Google Play require — review guidelines, privacy
declarations, legal agreements — and what they recommend, with how this wallet
meets each, is in [docs/store-publishing.md](docs/store-publishing.md).

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities privately as
described in [SECURITY.md](SECURITY.md).

## License

Licensed under the [Apache License 2.0](LICENSE).
