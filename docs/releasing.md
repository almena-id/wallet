# Releasing

Every merge into `main` releases the wallet: [`.github/workflows/release.yml`](../.github/workflows/release.yml)
works out the version, builds every platform, publishes the computer installers
as a GitHub release and, after an approval, uploads the phone builds to the
stores' testing tracks.

| Platform | Task | What is built | Signed with | Goes to |
|---|---|---|---|---|
| Linux x86_64 | `task release:linux` | `.deb`, `.rpm`, `.AppImage` | `SHA256SUMS.asc` (OpenPGP) | GitHub release |
| Windows x86_64 | `task release:windows` | `-setup.exe` (NSIS), `.msi` | unsigned, or Authenticode via Azure Trusted Signing | GitHub release |
| macOS Apple silicon | `task release:macos` | `.dmg` | Developer ID, notarized and stapled | GitHub release |
| Android | `task release:android` | `.aab` + R8 mapping | the upload key (Google re-signs) | Google Play, internal testing |
| iOS | `task release:ios` | `.ipa` | Apple Distribution | App Store Connect, TestFlight |

Each job of the workflow runs the same task a person runs by hand
(`task release:<platform> VERSION=2026.11.1`): the workflow only installs the
toolchain and turns the secrets below into the files and variables
`.env.local` names locally. A release built on a machine is the same release.
Every task leaves its files in `release/` (ignored) as
`almena-wallet-<version>-<platform>.<ext>`, and puts back whatever it stamped.

## Versions

`year.month.sequence` (`2026.11.1`), as in the mediator and the CLI. The
workflow counts the `v<year>.<month>.*` tags already made and takes the next
number; the sequence starts at 1 each month. Run again on a commit that
already has a tag (re-run, or Actions → Release → Run workflow), it builds that
same version and replaces the release's files instead of taking a new number.
Runs queue up, never in parallel, so two merges cannot take the same number.

Inside the app the version is `2026.11.1`, the MSI's is `26.11.1` and
Android's `versionCode` is `2026011001`; see
[scripts/stamp-version.mjs](../scripts/stamp-version.mjs).

## The workflows

- [`check.yml`](../.github/workflows/check.yml) — every pull request and every
  push to `develop`: `task check` on Linux, and `cargo check` of the Android
  and iOS Rust (`task check:mobile`, split by runner). The release calls it
  first, so nothing unchecked is built.
- [`release.yml`](../.github/workflows/release.yml) — every push to `main`, and
  by hand (Actions → Release → Run workflow; untick *stores* to build without
  uploading):
  1. **version**, then **check**;
  2. the five builds, in parallel, each kept as an artifact for a week;
  3. **GitHub release** `v<version>` on the merged commit, once all five have
     built (so a failed build uses up no number): the computer installers,
     `SHA256SUMS` and its signature `SHA256SUMS.asc`. The phone builds are not
     in it;
  4. **Google Play** and **App Store Connect**, in the `stores` environment,
     which waits for an approval. An upload cannot be undone and a store never
     takes the same build number twice: if one fails, re-run that job alone
     rather than merging again.

The stores take a build into their testing track and no further. Promoting an
Android release to production and sending an iOS build to review are done by
hand in Play Console and App Store Connect.

### Once, in the repository

1. Settings → Environments → **New environment** `stores`, with **Required
   reviewers** (yourself). Its secret is `PLAY_SERVICE_ACCOUNT_JSON`, so only
   an approved job can read it.
2. Settings → Secrets and variables → Actions: the secrets and variables below.
   Those the CLI's repository already has at the organisation
   (`RELEASE_GPG_PRIVATE_KEY`, `RELEASE_GPG_PASSPHRASE`) can stay there,
   granted to this repository.
3. **Google Play refuses an API upload for an app that has never had one.**
   The first `.aab` goes in by hand (below), with `PLAY_RELEASE_STATUS` set to
   `draft` until the app has passed its first review.

### Secrets

Files go in as base64 on one line (`base64 -i <file> | pbcopy` on a Mac).

| Secret | What | Job |
|---|---|---|
| `MACOS_CERTIFICATE_P12` | Developer ID Application certificate with its key, exported from Keychain Access as `.p12`, base64 | macos |
| `MACOS_CERTIFICATE_PASSWORD` | the password it was exported with | macos |
| `MACOS_PROVISION_PROFILE` | the Developer ID provisioning profile, base64 | macos |
| `ASC_API_KEY_P8` | the App Store Connect API key, the text of `AuthKey_….p8` as it is | macos (notarization), app-store |
| `IOS_CERTIFICATE_P12` | Apple Distribution certificate with its key, `.p12`, base64 | ios |
| `IOS_CERTIFICATE_PASSWORD` | its password | ios |
| `IOS_PROVISION_PROFILE` | the App Store Connect provisioning profile, base64 | ios |
| `ANDROID_UPLOAD_KEYSTORE` | `upload.jks`, base64 | android |
| `ANDROID_UPLOAD_KEYSTORE_PASSWORD` | the keystore's password | android |
| `ANDROID_UPLOAD_KEY_PASSWORD` | the key's password (the keystore's, if they are the same) | android |
| `GOOGLE_SERVICES_JSON` | Firebase's `google-services.json`, as it is | android |
| `PLAY_SERVICE_ACCOUNT_JSON` | the Play service account's JSON key, as it is (environment `stores`) | google-play |
| `RELEASE_GPG_PRIVATE_KEY` | the Almena release key, `gpg --armor --export-secret-keys <fingerprint>` | release |
| `RELEASE_GPG_PASSPHRASE` | its passphrase | release |
| `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` | optional: the app registration Trusted Signing accepts | windows |

### Variables

| Variable | What | Default |
|---|---|---|
| `APPLE_TEAM_ID` | the Apple Team ID | — |
| `ASC_API_KEY_ID` | the API key's Key ID | — |
| `ASC_API_ISSUER_ID` | its Issuer ID | — |
| `ANDROID_UPLOAD_KEY_ALIAS` | the upload key's alias | `upload` |
| `PLAY_TRACK` | `internal`, `alpha` (closed), `beta` (open) or `production` | `internal` |
| `PLAY_RELEASE_STATUS` | `draft` until the app's first review, then `completed` | `completed` |
| `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE` | optional: with the endpoint set, the Windows installers are signed | unset: unsigned |

## On this machine

The same tasks, with their keys from `wallet/.env.local` (ignored; see
`.env.example`) under names Tauri does not read, so a plain `task build`
never signs. Paths are absolute. Each platform is built on itself: Linux on
Linux x86_64, Windows on Windows, macOS and iOS on a Mac with Apple silicon,
Android anywhere with its SDK, NDK and a JDK between 17 and 24.

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
| `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE` | Windows | optional, with `trusted-signing-cli` installed and the `AZURE_*` credentials exported |

Android also needs `src-tauri/gen/android/app/google-services.json`, the
publisher's, never committed. A release made here is uploaded by hand: the
`.aab` in Play Console, the `.ipa` with Transporter, the installers to the
GitHub release. On Linux, sign the checksums by hand (below).

## Apple

Needs an Apple Developer Program membership as an organization. The Account
Holder is the only role that can create Developer ID certificates.

1. **Team ID** → `APPLE_TEAM_ID` / `APPLE_DEVELOPMENT_TEAM`: developer.apple.com
   → Account → Membership details.
2. **App ID**: Certificates, Identifiers & Profiles → Identifiers → +,
   App IDs, App. Explicit bundle ID `id.almena.wallet`, platforms iOS and macOS,
   capabilities **Push Notifications** and **Associated Domains**. One App ID
   serves both.
3. **App Store Connect API key** → `ASC_API_KEY_P8`, `ASC_API_KEY_ID`,
   `ASC_API_ISSUER_ID` (locally `NOTARY_API_*`): App Store Connect → Users and
   Access → Integrations → App Store Connect API → Team Keys → +, access **App
   Manager** (it both notarizes and uploads). Note the Issuer ID above the list
   and the Key ID; download the `.p8` (only once).
4. **Developer ID Application certificate** → `MACOS_CERTIFICATE_P12` (locally
   `MACOS_RELEASE_IDENTITY`):
   1. Keychain Access → Certificate Assistant → Request a Certificate From a
      Certificate Authority, saved to disk.
   2. developer.apple.com → Certificates → + → **Developer ID Application**
      (profile type G2 Sub-CA), upload the request, download the `.cer`, open
      it: it lands in the login keychain with its key.
   3. For the workflow: Keychain Access → My Certificates → the certificate →
      Export as `.p12` with a strong password (`MACOS_CERTIFICATE_PASSWORD`).
5. **Developer ID provisioning profile** → `MACOS_PROVISION_PROFILE` (locally
   `MACOS_RELEASE_PROFILE`): Profiles → + → Distribution → **Developer ID**,
   App ID `id.almena.wallet`, platform macOS, the certificate from step 4.
   Without it macOS keeps a distributed wallet out of the data protection
   keychain (no Touch ID).
6. **Apple Distribution certificate** → `IOS_CERTIFICATE_P12` (locally
   `IOS_RELEASE_CERTIFICATE`): as step 4, choosing **Apple Distribution**, then
   exported as `.p12` with a strong password (`IOS_CERTIFICATE_PASSWORD`).
7. **App Store provisioning profile** → `IOS_PROVISION_PROFILE` (locally
   `IOS_RELEASE_PROFILE`): Profiles → + → Distribution → **App Store Connect**,
   App ID `id.almena.wallet`, platform iOS, the certificate from step 6. Make
   it after step 2's capabilities, or the profile lacks them.
8. **The app in App Store Connect**: Apps → + → New App, iOS, bundle ID
   `id.almena.wallet`, SKU of choice. Fill in the privacy details and export
   compliance (see [store-publishing.md](store-publishing.md)) before the first
   review. From then on the workflow uploads each build to TestFlight.
9. **APNs** belongs to the mediator: Keys → + → Apple Push Notifications
   service, for the mediator's configuration.

Certificates last 5 years (Developer ID) or 1 year (Apple Distribution);
profiles expire with their certificate. Renew → repeat the step → replace the
secret (and point `.env.local` at the new files).

## Windows

The installers are built unsigned: SmartScreen warns about them. Authenticode
needs a code signing certificate in an HSM —
[Azure Trusted Signing](https://learn.microsoft.com/azure/trusted-signing/) is
the usual way. Once an account and certificate profile exist there, with an
app registration given the *Trusted Signing Certificate Profile Signer* role,
set the three `AZURE_SIGNING_*` variables and the three `AZURE_*` secrets: the
next release signs the wallet's executable and both installers, and checks
their signatures.

## Linux

Linux has no notarization: downloads are vouched for by an OpenPGP signature
over their checksums, made by the release job with `RELEASE_GPG_PRIVATE_KEY`.

1. Once (if the organisation has no release key yet): `gpg --full-generate-key`
   — ECC (sign only), Curve 25519, expiry 2y, name `Almena ID Releases`, email
   `security@almena.id`, a passphrase. Publish the public key
   (`gpg --armor --export <fingerprint>`) where people find it (almena.id,
   keys.openpgp.org), and keep a revocation certificate offline.
2. By hand, in `release/`: `sha256sum *.deb *.rpm *.AppImage > SHA256SUMS`,
   then `gpg --armor --detach-sign SHA256SUMS`.

Checking a download: `gpg --verify SHA256SUMS.asc SHA256SUMS`, then
`sha256sum -c --ignore-missing SHA256SUMS`.

## Android

Needs a Google Play developer account as an organization.

1. **Upload key** → `ANDROID_UPLOAD_KEYSTORE` (base64), its alias and
   passwords:
   `keytool -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000`.
   Keep a copy offline: Google can reset a lost upload key, but it takes days.
2. **The app in Play Console**: Create app, `Almena Wallet`. Play App Signing
   is on by default (Google keeps the app signing key). Fill in the store
   listing, Data safety and content rating (see
   [store-publishing.md](store-publishing.md)).
3. **Firebase** → `GOOGLE_SERVICES_JSON`: Firebase console → the project →
   Project settings → Add app → Android, package `id.almena.wallet`; download
   `google-services.json` (locally, into `src-tauri/gen/android/app/`). The FCM
   service account for the mediator comes from the same project.
4. **The first upload, by hand**: `task release:android VERSION=…` here, then
   Testing → Internal testing → Create release with the `.aab` from `release/`.
   It registers the upload key; Play takes API uploads only after it.
5. **Service account** → `PLAY_SERVICE_ACCOUNT_JSON`: in Google Cloud, in the
   project linked to Play Console, create a service account and a JSON key for
   it; in Play Console → Users and permissions → Invite new users, its email,
   with the app's **Release apps to testing tracks** permission (and
   **Release to production** only if `PLAY_TRACK` is to be `production`).
