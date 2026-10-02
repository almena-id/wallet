# Releasing

Every merge into `main` runs [.github/workflows/release.yml](../.github/workflows/release.yml):

| Platform | What is built | Signed with | Goes to |
|---|---|---|---|
| Linux x86_64 | `.deb`, `.AppImage` | the release OpenPGP key, over `SHA256SUMS` | GitHub release |
| Windows x86_64 | `-setup.exe` (NSIS), `.msi` | Authenticode, Azure Trusted Signing | GitHub release |
| macOS Apple silicon | `.dmg` | Developer ID, notarized and stapled | GitHub release |
| Android | `.aab` | the upload key (Google re-signs) | Google Play |
| iOS | `.ipa` | Apple Distribution | App Store Connect / TestFlight |

The version is `year.month.sequence` (tag `v2026.09.1`). Inside the app it is
`2026.9.1`, the MSI's is `26.9.1` and Android's `versionCode` is `2026009001`;
see [.github/scripts/stamp-version.mjs](../.github/scripts/stamp-version.mjs).
The GitHub release is made once every platform has built; the stores come
after it. A failed store upload is re-run on its own job: merging again would
take a new number for nothing.

The stores take a build into their pipeline and no further. Promoting an
Android release past `PLAY_TRACK`, and sending an iOS build to review, is done
in Play Console and App Store Connect.

## Secrets and variables

Set in the repository (Settings → Secrets and variables → Actions), or in the
organization when another repository needs the same value (marked *org*: the
`almena` CLI's release signs and notarizes with them too, and an organization
secret must be granted to each repository that reads it).
Variables are not secret; secrets are never shown again once saved.

| Name | Kind | Used by | |
|---|---|---|---|
| `APPLE_TEAM_ID` | variable, *org* | macOS, iOS | Apple Team ID |
| `ASC_API_KEY_ID` | variable, *org* | macOS (notary), App Store | App Store Connect API key ID |
| `ASC_API_ISSUER_ID` | variable, *org* | macOS (notary), App Store | its issuer ID |
| `ASC_API_KEY_P8` | secret, *org* | macOS (notary), App Store | the `AuthKey_….p8`, as text |
| `MACOS_CERTIFICATE_P12` | secret, *org* | macOS | Developer ID Application `.p12`, base64 |
| `MACOS_CERTIFICATE_PASSWORD` | secret, *org* | macOS | its password |
| `MACOS_PROVISION_PROFILE` | secret | macOS | Developer ID profile, base64 |
| `IOS_CERTIFICATE_P12` | secret | iOS | Apple Distribution `.p12`, base64 |
| `IOS_CERTIFICATE_PASSWORD` | secret | iOS | its password |
| `IOS_PROVISION_PROFILE` | secret | iOS | App Store profile, base64 |
| `AZURE_TENANT_ID` | secret, *org* | Windows | Entra ID tenant |
| `AZURE_CLIENT_ID` | secret, *org* | Windows | the signing app registration |
| `AZURE_CLIENT_SECRET` | secret, *org* | Windows | its client secret |
| `AZURE_SIGNING_ENDPOINT` | variable, *org* | Windows | e.g. `https://weu.codesigning.azure.net` |
| `AZURE_SIGNING_ACCOUNT` | variable, *org* | Windows | Trusted Signing account name |
| `AZURE_SIGNING_PROFILE` | variable, *org* | Windows | its certificate profile name |
| `RELEASE_GPG_PRIVATE_KEY` | secret, *org* | release | armoured private key |
| `RELEASE_GPG_PASSPHRASE` | secret, *org* | release | its passphrase |
| `ANDROID_UPLOAD_KEYSTORE` | secret | Android | upload keystore `.jks`, base64 |
| `ANDROID_UPLOAD_KEYSTORE_PASSWORD` | secret | Android | keystore password |
| `ANDROID_UPLOAD_KEY_ALIAS` | variable | Android | key alias, e.g. `upload` |
| `ANDROID_UPLOAD_KEY_PASSWORD` | secret | Android | key password |
| `GOOGLE_SERVICES_JSON` | secret | Android | Firebase `google-services.json`, as text |
| `PLAY_SERVICE_ACCOUNT_JSON` | secret | Google Play | service account key, as text |
| `PLAY_TRACK` | variable | Google Play | `internal` (default), `alpha`, `beta`, `production` |
| `PLAY_RELEASE_STATUS` | variable | Google Play | `completed` (default); `draft` until the app's first review |

Base64 on macOS: `base64 -i file | pbcopy`. On Linux: `base64 -w0 file`.

## Apple

Needs an Apple Developer Program membership as an organization. The Account
Holder is the only role that can create Developer ID certificates.

1. **Team ID** → `APPLE_TEAM_ID`: developer.apple.com → Account → Membership
   details.
2. **App ID**: Certificates, Identifiers & Profiles → Identifiers → +,
   App IDs, App. Explicit bundle ID `id.almena.wallet`, platforms iOS and macOS,
   capability **Push Notifications**. One App ID serves both.
3. **App Store Connect API key** → `ASC_API_KEY_ID`, `ASC_API_ISSUER_ID`,
   `ASC_API_KEY_P8`: App Store Connect → Users and Access → Integrations →
   App Store Connect API → Team Keys → +, access **App Manager** (it uploads
   builds and notarizes). Note the Issuer ID above the list and the Key ID;
   download the `.p8` (only once) and paste its whole text.
4. **Developer ID Application certificate** → `MACOS_CERTIFICATE_P12`,
   `MACOS_CERTIFICATE_PASSWORD`:
   1. On a Mac: Keychain Access → Certificate Assistant → Request a Certificate
      From a Certificate Authority, saved to disk.
   2. developer.apple.com → Certificates → + → **Developer ID Application**
      (profile type G2 Sub-CA), upload the request, download the `.cer`, open it.
   3. Keychain Access → My Certificates → the certificate with its key →
      Export as `.p12` with a strong password; base64 it.
5. **Developer ID provisioning profile** → `MACOS_PROVISION_PROFILE`: Profiles
   → + → Distribution → **Developer ID**, App ID `id.almena.wallet`, platform
   macOS, the certificate from step 4. Download, base64. Without it macOS keeps
   a distributed wallet out of the data protection keychain (no Touch ID).
6. **Apple Distribution certificate** → `IOS_CERTIFICATE_P12`,
   `IOS_CERTIFICATE_PASSWORD`: as step 4, choosing **Apple Distribution**.
7. **App Store provisioning profile** → `IOS_PROVISION_PROFILE`: Profiles → +
   → Distribution → **App Store Connect**, App ID `id.almena.wallet`, platform
   iOS, the certificate from step 6. Make it after step 2's push capability,
   or the profile lacks it. Download, base64.
8. **The app in App Store Connect**: Apps → + → New App, iOS, bundle ID
   `id.almena.wallet`, SKU of choice. Fill in the privacy details and export
   compliance (see [store-publishing.md](store-publishing.md)) before the first
   review.
9. **APNs** belongs to the mediator, not to this workflow: Keys → + → Apple
   Push Notifications service, for the mediator's configuration.

Certificates last 5 years (Developer ID) or 1 year (Apple Distribution);
profiles expire with their certificate. Renew → repeat the step → replace the
secret.

## Windows

Authenticode with [Azure Trusted Signing](https://learn.microsoft.com/azure/trusted-signing/):
the key stays in Microsoft's HSM, which a code signing certificate must be in
today (a `.pfx` in a secret is no longer issued). Check eligibility first: it
validates the organization's identity, and asks for a verifiable history.

1. An Azure subscription → Create resource → **Trusted Signing Account**, in
   the nearest region; its URI is `AZURE_SIGNING_ENDPOINT`
   (`https://weu.codesigning.azure.net` for West Europe), its name
   `AZURE_SIGNING_ACCOUNT`.
2. Give yourself the role **Trusted Signing Identity Verifier** on it; then
   Identity validation → New → Organization, and wait for approval.
3. Certificate profiles → + → **Public Trust**, with the validated identity;
   its name is `AZURE_SIGNING_PROFILE`.
4. Microsoft Entra ID → App registrations → New registration (e.g.
   `almena-release-signing`): Directory (tenant) ID → `AZURE_TENANT_ID`,
   Application (client) ID → `AZURE_CLIENT_ID`; Certificates & secrets → New
   client secret → its value → `AZURE_CLIENT_SECRET` (note its expiry).
5. On the Trusted Signing account → Access control → Add role assignment →
   **Trusted Signing Certificate Profile Signer** → that app registration.

SmartScreen warns about a new publisher until downloads build its reputation.

## Linux

Linux has no notarization: the downloads are vouched for by the checksums'
OpenPGP signature.

1. `gpg --full-generate-key`: ECC (sign only), Curve 25519, expiry 2y, name
   `Almena ID Releases`, email `security@almena.id`, a passphrase →
   `RELEASE_GPG_PASSPHRASE`.
2. `gpg --armor --export-secret-keys <fingerprint>` → `RELEASE_GPG_PRIVATE_KEY`.
3. `gpg --armor --export <fingerprint>` — the public key: publish it where
   people find it (almena.id, keys.openpgp.org) and keep a revocation
   certificate offline.

Checking a download: `gpg --verify SHA256SUMS.asc SHA256SUMS`, then
`sha256sum -c --ignore-missing SHA256SUMS`.

## Android

Needs a Google Play developer account as an organization.

1. **Upload key** → `ANDROID_UPLOAD_KEYSTORE`, `…_KEYSTORE_PASSWORD`,
   `…_KEY_ALIAS`, `…_KEY_PASSWORD`:
   `keytool -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000`;
   base64 the `.jks`. Keep a copy offline: Google can reset a lost upload key,
   but it takes days.
2. **The app in Play Console**: Create app, `Almena Wallet`. Play App Signing
   is on by default (Google keeps the app signing key). Fill in the store
   listing, Data safety and content rating (see
   [store-publishing.md](store-publishing.md)).
3. **First upload by hand**: the API cannot create the app's first release.
   Testing → Internal testing → Create release, with an `.aab` from a run of
   this workflow (its `android` artifact); this also registers the upload key.
4. **Service account** → `PLAY_SERVICE_ACCOUNT_JSON`: in Google Cloud (any
   project), IAM → Service accounts → Create, no roles; Keys → Add key → JSON.
   Then Play Console → Users and permissions → Invite new users → the service
   account's email, app access to Almena Wallet with **Release to testing
   tracks** (and **Release to production** if `PLAY_TRACK` is `production`).
   Enable the Google Play Android Developer API in that Cloud project.
5. **Firebase** → `GOOGLE_SERVICES_JSON`: Firebase console → the project →
   Project settings → Add app → Android, package `id.almena.wallet`; download
   `google-services.json` and paste its text. The FCM service account for the
   mediator comes from the same project.
