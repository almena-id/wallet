# Almena ID wallet

Cross-platform wallet client for the Almena ID platform, built with Tauri v2,
Vite and React. It targets phones and tablets on iOS and Android, and desktops
on Windows, macOS and Linux.

## What is in it

The first screen is a dashboard with a floating menu in the iOS liquid glass
idiom: **Home**, **Scan QR** and **Settings**. Scanning opens the camera with a
framing assistant and a back button that returns to the dashboard home. Every
string comes from a catalogue in `src/i18n/messages/`; English is the fallback
and Spanish ships with it. Adding a language is adding a JSON file there —
no code changes.

## Getting in

The first screen offers the two ways there are: make an identity, or bring one
back. Both end at the same place, because both are the same act — an identity
here is what twelve words produce, and there is no account anywhere to look up.

Making one shows the twelve words behind a checkbox that has to be ticked
deliberately, then asks for three of them back: a position, and six words of the
phrase to choose from. The five wrong ones are words from the same phrase, so
the question can only be answered from the copy somebody keeps, not from what
the screen looked like a moment ago.

**One wrong answer ends that phrase.** Twelve new words are drawn and have to be
written down again — a phrase somebody could not read back is a phrase they do
not have, and letting them past it would be the wallet agreeing to lose their
identity later. When all three land, the mark turns while the identity is
derived and the wallet opens on the dashboard, which shows the identifier.

The wordlist follows the interface. BIP-39 defines wordlists for ten languages
and the wallet carries all of them, so a phrase read in Spanish is written in
Spanish, one read in Japanese in Japanese, and one read in a language the
standard has no words for — German, say — in English, which is the list every
implementation carries. It also means a phrase somebody made in another wallet,
in any of the ten, is one this wallet reads back: restoring tries every list.

The interface language decides it, not the operating system directly: an English
screen showing French words would be an odd thing to hand somebody. Today the
two agree wherever the wallet has a catalogue, which is `en` and `es`.

What the words produce, and how, is pinned:

| Step | What it is |
| --- | --- |
| Phrase | BIP-39, twelve words, 128 bits of entropy from the operating system |
| Seed | BIP-39 seed, no passphrase |
| Key | SLIP-0010 over ed25519, one hardened step at `m/0'` |
| Identifier | `did:key:z…` — multicodec `ed25519-pub`, multibase base58btc |
| Document | A `did:key` DID document, `Multikey` verification method |

**The derivation path is frozen.** Change it after anybody has created an
identity and the same twelve words start opening a different, empty one, with no
error anywhere to explain it. `src-tauri/src/identity/keys.rs` says so at the
top, and a test pins a phrase to the identifier it has to keep producing.

**What is written down is the seed, and nothing else.** The words are the
backup and the backup belongs to the person: a wallet that held them would hold
something that can be read aloud, photographed off a screen or typed into
somebody else's wallet. The seed derives every key this wallet will ever sign
with, so keeping it is enough, and it cannot be turned back into words.

The seed is encrypted by a random data key, and that data key is wrapped twice:
once by a key the PIN derives with Argon2id, and — where the platform has a
secret store — once by the device, which is what lets a face open the wallet
without the digits. Either wrap opens it and neither is the wallet, so a sensor
that stops recognising somebody is not the end of an identity and a forgotten
PIN is not either.

The record carries its own version, cost parameters, salt and nonces, because a
wallet installed over an older one has to read what that one wrote; a record
from a newer wallet is refused by name rather than misread.

Where it is kept is the platform's own secret store, named rather than guessed —
Keychain on Apple platforms, the Credential Manager on Windows, the Secret
Service on Linux. It is not chosen for secrecy alone, since the record is
already encrypted, but for what it survives: a Keychain item outlives the app
being deleted, so a wallet reinstalled on the same phone finds its identity
where it left it.

**Android has no store this side can reach**, so the record goes to the
application's private storage instead. Reaching the Android Keystore needs a
Kotlin bridge and a JNI symbol the wallet does not have yet; until it does, this
is said out loud rather than implied.

On Apple platforms the record is pinned to the device it was written on —
`when-unlocked-this-device-only`, so it is in no backup and on no other phone —
and the device key sits behind `kSecAccessControlUserPresence`, which means the
prompt is the operating system asking, not a screen this wallet drew. One
consequence worth knowing before it bites: a Keychain item belongs to the team
and the bundle identifier together, so changing `APPLE_DEVELOPMENT_TEAM` or the
identifier gives the next build a different access group and the record already
on the phone becomes somebody else's. The way back is the phrase.

Nothing about the identity is sent to the platform's API. `src-tauri/src/vault/`
holds all of it.

Settings is a list of places rather than a page of switches — **Appearance**,
where the identity colour is picked from a row of the colours themselves and the
mode is set to system, dark or light, **Permissions**, where what the device has
granted is read back from the device itself and can be asked for,
**Security**, which holds the lock and signing out, and **Development**, which holds
what this build is and what it can do on the device it is running on.

Both appearance choices are remembered on the device and applied before the
first frame. `system` is not a third palette: it is the absence of a choice,
which leaves `prefers-color-scheme` to answer.

### The lock

Security offers a PIN of four or six digits and, where the platform has a reader
for it, biometrics standing in for that PIN. **The PIN comes first and
biometrics cannot be turned on without one**: a face the sensor stops
recognising has to have something behind it, or the only way back into the
wallet is signing out and writing twelve words again.

It closes when the wallet leaves the screen — backgrounded on a phone, put away
on the tray on a computer, which the webview only learns because the Rust side
says so — and opens with the keypad or the reader.

**The lock is the record.** There is no PIN hash anywhere, because a PIN is
right if and only if what it derives opens what was written down — so there is
nothing stored to steal and compare against, and the check is a message
authentication code rather than a comparison.

**Every PIN goes through one door.** Exactly one function checks digits and it
is the one that spends an attempt, out of ten, under a single lock — because a
check that does not count is a wallet that can be guessed at forever through
whichever screen forgot to count, and two answers arriving at once must not both
start from nine attempts left.

Argon2id is the whole of the protection on four digits, so its cost is written
into the record rather than assumed: 64 MiB over three passes, about a third of
a second on a mid-range phone, which is a wait somebody accepts once per launch
and an attacker pays for every one of a million guesses. Raising it later must
not lock out a wallet that was sealed with less.

Biometrics are mobile only. Tauri's plugin covers Android and iOS; Windows Hello
and Touch ID have no plugin, so on a computer the lock is the PIN alone and the
screen says so.

Signing out is that fact made visible: it lets go of the identity,
and the twelve words are the only way back. The screen says so, refuses to arm
its confirmation for ten seconds, and then asks for a deliberate drag rather
than a tap — a destructive act a stray finger can complete is one stray fingers
will complete.

The interface language is the system's, with no switch in the wallet. Somebody
who reads in Spanish has already said so once, to their device; an app that asks
again is an app that can be left saying something they did not choose.

## Platform features

Each plugin is registered only where the platform provides it, and the frontend
asks the Rust side what it got rather than sniffing the user agent:

| Plugin | Windows, macOS, Linux | Android, iOS | What it does |
| --- | --- | --- | --- |
| `window-state` | yes | no | Remembers window size, position and maximised state |
| `single-instance` | yes | no | A second launch brings back the window already open |
| tray icon | yes | no | Keeps the wallet running with no window on screen |
| `notification` | yes | yes | Notifies through the host system's own mechanism |
| `barcode-scanner` | no | yes | Reads QR codes with the camera |
| `deep-link` | yes | yes | Opens the wallet on an `almena://` link |

On a computer the Scan QR section says so plainly instead of failing: the
camera scanner is provided by iOS and Android.

### almena:// links

The wallet answers the `almena://` scheme on all five platforms. A link opens
it, brings the window back if it was on the tray, and lands on a screen that
shows what arrived.

**Nothing a link carries is acted on.** A link comes from outside — a page, a
message, anything that can put a URL in front of somebody — and nothing outside
gets to tell a wallet what to do with the identity it holds. A link that is not
a sign-in request is shown and no more.

`almena://signin?…` is the one thing the wallet knows how to answer, and it
still decides nothing on its own. **The platform is pinned at build time**, not
taken from the link: a wallet that followed whichever address a QR code named
could be pointed at a stranger's server, and the person scanning would have no
way to see the difference. The request has to sit under that platform, carry
that platform's signature and still be in time; then who is asking is shown, and
the answer goes out only when somebody says so. A scanned code takes the same
road as a link — the camera is a way of typing a URL, not a second kind of
trust.

**The answer is signed with a key that belongs to that verifier alone**, derived
from the digest of the verifier's own identifier. Every verifier is shown a
different public key, and the only thing that knows they belong together is this
device — nothing is stored to make that work, so the same phrase on a new phone
derives exactly the same keys again. The identifier the key hangs off is the DID
in the signed request, not a label anybody asserted in passing.

Where each platform gets the scheme from, all of it generated from
`plugins.deep-link` in `tauri.conf.json` at build time:

| Platform | Registered through |
| --- | --- |
| macOS, iOS | `CFBundleURLTypes` in the bundle's `Info.plist` |
| Android | An intent filter in `AndroidManifest.xml` |
| Windows, Linux | The installer — and, in a debug build, `register_all()` at startup, so links can be tried before there is one |

macOS only knows about the scheme once the app bundle has been installed and
launched at least once: `task dev` runs a bare executable, which is not one.

### Closing the window is not quitting

On a computer the wallet puts a monochrome mark on the system tray, and closing
the window only takes it off the screen — the wallet goes on running. It comes
back from the tray icon, from launching the wallet again, or from the Dock icon
on macOS, and it ends through **Quit** in the tray menu. Where a tray could not
be installed — a Linux desktop serving none — closing the window closes the
wallet, as it always did.

The tray's one menu entry is a word somebody reads, so it comes from the same
catalogues as the rest and is handed to the Rust side by the frontend; changing
the language renames it.

## Requirements

- Node 22+ and [pnpm](https://pnpm.io)
- Rust (stable) and [Task](https://taskfile.dev)
- Android: Android SDK, NDK and a JDK between 17 and 24 — Gradle in the
  generated project rejects anything newer, including the JDK that ships inside
  Android Studio today (`brew install openjdk@17` is enough)
- iOS: Xcode, its command line tools and CocoaPods (macOS only)

`ANDROID_HOME`, `NDK_HOME` and `JAVA_HOME` do not need to be exported: the
Android tasks resolve them and export them into the build themselves, picking
the newest JDK Gradle actually supports. They do it inside the command rather
than through the Taskfile's `env`, because `env` loses to whatever the shell
already exports — and a shell managed by jenv or asdf usually exports a JDK far
newer than Gradle can read.

## Desktop

```sh
task install    # install the Node dependencies
task dev        # run the app in development mode
task build      # bundle for the host platform
task dev:web    # only the web layer, in a browser
```

## Android

```sh
task init:android     # one-off: generate src-tauri/gen/android
task deploy:android   # pick a target, build, install and launch
```

A deploy lists what this computer can reach, asks which target to use, and
then builds and installs the app there. It goes on the device complete: the
frontend is built and packaged inside it, so it keeps working once the device
is unplugged and off the network. A plain `tauri android dev` cannot do that —
on mobile it always points the device's webview back at the Vite server running
here, whatever flags it is given.

The cost is that a frontend change needs another deploy. For a fast edit loop
use `task dev` on this computer, which keeps hot reload.

`deploy:android` lists everything adb can reach — cabled devices, devices
paired over Wi-Fi (`adb pair` / `adb connect`) and running emulators — and
builds only for the ABI of the one you pick, which keeps the wait short. It
then installs the debug APK and launches it. Pass `DEVICE` to skip the
question: `task deploy:android DEVICE=Pixel`.

## iOS

```sh
task init:ios     # one-off: generate src-tauri/gen/apple
task deploy:ios   # pick a target, build, install and launch
```

Signing for a physical device needs an Apple development team, and Xcode
refuses to build a step without one. Write it once into `wallet/.env.local`,
which is not committed:

```sh
APPLE_DEVELOPMENT_TEAM=XXXXXXXXXX
```

It is the Team ID under developer.apple.com > Membership. `TEAM=XXXXXXXXXX` on
the command line overrides it, and the tasks say all of this when it is
missing rather than failing inside xcodebuild. A simulator signs nothing, so it
needs none of it.

`deploy:ios` lists the paired devices and the installed simulators and asks
which one to use, and takes `DEVICE` the same way. A simulator gets an
`aarch64-sim` build installed with `simctl`; a physical device gets a signed
debug build installed with `devicectl`.

The device half of that listing comes from `devicectl`, which is also what
installs. It is deliberately not `xctrace list devices`: xctrace shows what is
awake at that second, so a device paired over Wi-Fi drops off the list whenever
its tunnel is asleep — most of the time — which looks exactly like the device
being gone. Each one is listed with how it is attached, cable or Wi-Fi.

For a device over Wi-Fi, tick "Connect via network" for it in Xcode > Window >
Devices and Simulators. It is then listed like any other.

## Everything else

```sh
task              # list every task
task check        # TypeScript and Rust static checks
task clean        # drop build output and dependencies
```

## Branding

Every icon is derived from `assets/branding`, and the whole set is regenerated
with one task:

```sh
task icons
```

| Where | What it holds | Regenerated |
| --- | --- | --- |
| `src-tauri/icons/*.png`, `icon.icns`, `icon.ico` | Windows, macOS and Linux | yes, from `icon-manifest.json` — the `.icns` from `app-icon-macos.png`, which sizes the mark the way macOS expects |
| `src-tauri/icons/ios/` | iOS app icons, flattened onto the brand background because iOS refuses transparency | yes, from `icon-manifest.json` |
| `src-tauri/icons/android/` | The launcher's three layers — background, foreground, monochrome — plus the legacy and round icons | **no, drawn by hand** |
| `src-tauri/icons/tray.png`, `tray-light.png` | The tray glyph: black for macOS, which fills it as a template image, and white for the dark bars Windows and Linux default to | yes, on macOS — `sips` resizes them |

Android is the exception on purpose. Its launcher icon is layered, and the
foreground has to stay inside the 66/108 safe zone that the launcher mask crops
to; a single square source cannot be turned into that, and `tauri icon`
flattens it into one oversized layer that then gets clipped. That artwork is
committed and is only ever copied forward.

`tauri android init` and `tauri ios init` write their own icons over the
generated projects, so `init:android`, `init:ios` and `icons` all finish by
copying the committed artwork back into `src-tauri/gen`. That is what the built
app reads, and it is committed with the rest of the native projects.

One quirk worth knowing: `icon.icns` comes out byte-different on every run
because the Tauri CLI writes its entries in a random order. The image is the
same — the entries and their sizes match — so a diff limited to that file after
`task icons` can be dropped.

## More

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [SECURITY.md](SECURITY.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [LICENSE](LICENSE) — Apache-2.0
