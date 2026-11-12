# Releasing

Releases are built on this machine with `task`, into `release/` or the bundle
folder, and handed to the stores by hand. Nothing runs on GitHub, which only
keeps the code.

| Platform | Task | What is built | Signed with | Goes to |
|---|---|---|---|---|
| macOS Apple silicon | `task release:macos` | `.dmg` | Developer ID, notarized and stapled | wherever it is offered for download |
| Android | `task release:android VERSION=…` | `.aab` in `release/` | the upload key (Google re-signs) | Play Console, uploaded by hand |
| iOS | `task release:ios VERSION=…` | `.ipa` in `release/` | Apple Distribution | App Store Connect (Transporter), by hand |
| Windows x86_64 | `task build`, on Windows | `-setup.exe` (NSIS), `.msi` | not signed | — |
| Linux x86_64 | `task build`, on Linux | `.deb`, `.rpm`, `.AppImage` | `SHA256SUMS` signed by hand (below) | — |

The version is `year.month.sequence` (`2026.11.1`). Inside the app it is
`2026.11.1`, the MSI's is `26.11.1` and Android's `versionCode` is
`2026011001`; see [scripts/stamp-version.mjs](../scripts/stamp-version.mjs).
The release tasks stamp it for the build alone and put the files back.

The stores take a build into their pipeline and no further. Promoting an
Android release, and sending an iOS build to review, is done in Play Console
and App Store Connect.

## What `.env.local` holds

The release tasks read their keys from `wallet/.env.local` (ignored; see
`.env.example`), under names Tauri does not read, so a plain `task build`
never signs. Paths are absolute.

| Name | Used by | |
|---|---|---|
| `APPLE_DEVELOPMENT_TEAM` | macOS, iOS | Apple Team ID |
| `MACOS_RELEASE_IDENTITY` | macOS | the "Developer ID Application: …" certificate, as the keychain names it |
| `MACOS_RELEASE_PROFILE` | macOS | the Developer ID provisioning profile |
| `NOTARY_API_KEY`, `NOTARY_API_ISSUER`, `NOTARY_API_KEY_PATH` | macOS | App Store Connect API key ID, its issuer ID, the `AuthKey_….p8` |
| `IOS_RELEASE_CERTIFICATE`, `IOS_RELEASE_CERTIFICATE_PASSWORD` | iOS | the Apple Distribution `.p12` and its password |
| `IOS_RELEASE_PROFILE` | iOS | the App Store Connect provisioning profile |
| `ANDROID_UPLOAD_KEYSTORE`, `ANDROID_UPLOAD_KEY_ALIAS` | Android | `upload.jks` and its key's alias |
| `ANDROID_UPLOAD_KEYSTORE_PASSWORD`, `ANDROID_UPLOAD_KEY_PASSWORD` | Android | optional: asked for when missing |

Android also needs `src-tauri/gen/android/app/google-services.json`, the
publisher's, never committed.

## Apple

Needs an Apple Developer Program membership as an organization. The Account
Holder is the only role that can create Developer ID certificates.

1. **Team ID** → `APPLE_DEVELOPMENT_TEAM`: developer.apple.com → Account →
   Membership details.
2. **App ID**: Certificates, Identifiers & Profiles → Identifiers → +,
   App IDs, App. Explicit bundle ID `id.almena.wallet`, platforms iOS and macOS,
   capabilities **Push Notifications** and **Associated Domains**. One App ID
   serves both.
3. **App Store Connect API key** → `NOTARY_API_KEY`, `NOTARY_API_ISSUER`,
   `NOTARY_API_KEY_PATH`: App Store Connect → Users and Access → Integrations →
   App Store Connect API → Team Keys → +, access **App Manager**. Note the
   Issuer ID above the list and the Key ID; download the `.p8` (only once).
4. **Developer ID Application certificate** → `MACOS_RELEASE_IDENTITY`:
   1. Keychain Access → Certificate Assistant → Request a Certificate From a
      Certificate Authority, saved to disk.
   2. developer.apple.com → Certificates → + → **Developer ID Application**
      (profile type G2 Sub-CA), upload the request, download the `.cer`, open
      it: it lands in the login keychain with its key.
5. **Developer ID provisioning profile** → `MACOS_RELEASE_PROFILE`: Profiles
   → + → Distribution → **Developer ID**, App ID `id.almena.wallet`, platform
   macOS, the certificate from step 4. Without it macOS keeps a distributed
   wallet out of the data protection keychain (no Touch ID).
6. **Apple Distribution certificate** → `IOS_RELEASE_CERTIFICATE`: as step 4,
   choosing **Apple Distribution**, then Keychain Access → My Certificates →
   Export as `.p12` with a strong password (`IOS_RELEASE_CERTIFICATE_PASSWORD`).
7. **App Store provisioning profile** → `IOS_RELEASE_PROFILE`: Profiles → +
   → Distribution → **App Store Connect**, App ID `id.almena.wallet`, platform
   iOS, the certificate from step 6. Make it after step 2's capabilities, or
   the profile lacks them.
8. **The app in App Store Connect**: Apps → + → New App, iOS, bundle ID
   `id.almena.wallet`, SKU of choice. Fill in the privacy details and export
   compliance (see [store-publishing.md](store-publishing.md)) before the first
   review. Upload the `.ipa` with Transporter.
9. **APNs** belongs to the mediator: Keys → + → Apple Push Notifications
   service, for the mediator's configuration.

Certificates last 5 years (Developer ID) or 1 year (Apple Distribution);
profiles expire with their certificate. Renew → repeat the step → point
`.env.local` at the new files.

## Windows

`task build` on a Windows machine makes the installers, unsigned:
SmartScreen warns about them. Authenticode would need a code signing
certificate in an HSM — [Azure Trusted Signing](https://learn.microsoft.com/azure/trusted-signing/)
is the usual way —, and is not set up.

## Linux

`task build` on a Linux machine. Linux has no notarization: downloads are
vouched for by an OpenPGP signature over their checksums.

1. Once: `gpg --full-generate-key` — ECC (sign only), Curve 25519, expiry 2y,
   name `Almena ID Releases`, email `security@almena.id`, a passphrase.
   Publish the public key (`gpg --armor --export <fingerprint>`) where people
   find it (almena.id, keys.openpgp.org), and keep a revocation certificate
   offline.
2. Each release, in the bundle folder: `sha256sum *.deb *.rpm *.AppImage >
   SHA256SUMS`, then `gpg --armor --detach-sign SHA256SUMS`.

Checking a download: `gpg --verify SHA256SUMS.asc SHA256SUMS`, then
`sha256sum -c --ignore-missing SHA256SUMS`.

## Android

Needs a Google Play developer account as an organization.

1. **Upload key** → `ANDROID_UPLOAD_KEYSTORE`, `ANDROID_UPLOAD_KEY_ALIAS`:
   `keytool -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000`.
   Keep a copy offline: Google can reset a lost upload key, but it takes days.
2. **The app in Play Console**: Create app, `Almena Wallet`. Play App Signing
   is on by default (Google keeps the app signing key). Fill in the store
   listing, Data safety and content rating (see
   [store-publishing.md](store-publishing.md)).
3. **Firebase** → `google-services.json`: Firebase console → the project →
   Project settings → Add app → Android, package `id.almena.wallet`; download
   it into `src-tauri/gen/android/app/`. The FCM service account for the
   mediator comes from the same project.
4. **Each release**: Testing → Internal testing (or another track) → Create
   release, with the `.aab` from `release/`.
