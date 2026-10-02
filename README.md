# Almena Wallet

The wallet of Almena ID: the app people use to hold their identity and
exchange [DIDComm Messaging v2.0](https://identity.foundation/didcomm-messaging/spec/v2.0/)
messages through an Almena [mediator](https://github.com/almena-id/mediator).
One code base, built with Tauri v2, Vite and React, runs on Android, iOS,
macOS, Linux and Windows.

It is at an early stage: the shell, the look and the build for every platform
are in place, and so is onboarding — creating an identity from a new BIP-39
phrase or bringing one back from an existing phrase, then keeping it on the
device encrypted behind a PIN — and connecting to a mediator: the wallet asks
it for mediation and registers an inbox DID derived from the phrase. Contacts
are made with an invitation, as a QR code or a link, and each gets a DID of
its own; with them the wallet exchanges names and text messages, kept on the
device encrypted. While the wallet is open they arrive live, over a WebSocket to the mediator;
while it is not, on Android and iOS the mediator sends a push notification that
says only that something is waiting. Contacts can also call each other, by voice or video, while both
wallets are open; the call is relayed through each wallet's mediator, so
neither learns the other's address ([SPEC.md](SPEC.md)).

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
Dock icon on macOS) brings it back; the tray's menu is where it is quit. A
second launch brings the running one to the front instead of starting another,
and the window reopens where it was left. Profile → Security can have it open at
login, minimised and locked.
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
