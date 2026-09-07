# Security policy

This is the wallet. It holds the seed every key of an identity is derived from,
and it runs on a device nobody else controls. There is no account to recover
and no administrator to appeal to: a flaw here is somebody's identity, and the
twelve words are the only way back.

## Reporting a vulnerability

Write to **security@almena.id**, or open a private advisory through this
repository's **Security** tab. Do not open a public issue, and do not describe
it anywhere public until there is a fix.

**Never send a real recovery phrase, to us or to anybody.** If a report needs a
phrase, generate one on a wallet you then throw away, and say so.

Useful in a report:

- What an attacker gets, stated plainly, and what they need to begin with —
  the device in hand, a malicious link, a network position, another app on the
  same phone.
- The platform and the version: iOS, Android, Windows, macOS or Linux, and
  whether it was a debug build or a bundle.
- The steps, exactly enough to repeat them.

You will get an acknowledgement within **3 working days** and an assessment
within **10**. If a fix takes longer than that, you will be told where it
stands. Credit is given in the release notes unless you would rather it were
not.

## What is in scope

**The record.** The seed is encrypted by a random data key; the data key is
wrapped by a key the PIN derives with Argon2id, and — where the platform has a
secret store — a second copy is wrapped by the device, which is what lets
biometrics stand in for the digits. Anything that opens it without one of those
two, that gets the seed out in the clear, or that leaves it in memory after the
wallet has let go of it, is a finding.

**The PIN.** Nothing that verifies a PIN is stored: a PIN is right if and only
if what it derives opens the record. There is exactly one function that checks
digits and it is the one that spends an attempt, under a single lock, out of
ten. Anything that checks a PIN without spending an attempt, that resets the
count, or that gets two answers counted as one, is a finding.

**The derivation cost.** The parameters are read from the record before the
record has been authenticated, because they are what produces the key that
would authenticate it. They are bounded for exactly that reason. A record that
makes the wallet allocate what the bounds were meant to refuse is a finding.

**Where the record is kept.** The platform's own secret store, named rather than
guessed — Keychain, Credential Manager, Secret Service. On Apple platforms the
record is `when-unlocked-this-device-only`, so it is in no backup and on no
other phone, and the device key is behind `kSecAccessControlUserPresence`, which
the operating system enforces rather than a screen this wallet drew. Anything
that widens that — a record reachable from a backup, from another app, or from
another device — is a finding.

**Android is the known gap, and it is written down rather than implied.** There
is no store this side can reach without a Kotlin bridge and a JNI symbol the
wallet does not have yet, so the record goes to the application's private
storage. It is still encrypted, and reports about that path are welcome; that
it is not the Keystore is already known.

**The phrase.** It is held only between being shown and being confirmed, and
then it is gone. Anything that writes it down, keeps it, or lets it out — a
screenshot the system was allowed to take, a clipboard, a log, a crash report —
is a finding.

**The derivation itself.** BIP-39 to the seed, SLIP-0010 over ed25519 with one
hardened step at `m/0'` to the key, `did:key` to the identifier. It is frozen: a
change makes the same words open a different, empty identity with no error
anywhere to explain it, so a way to make the wallet derive off that path is a
finding.

**Per-verifier keys.** Every verifier is shown a different public key, derived
from the digest of that verifier's identifier, and the only thing that knows
they belong together is the device. Anything that lets two verifiers recognise
the same person, or that gets a key derived from a name somebody merely
asserted rather than from the DID in a signed request, is a finding.

**Sign-in.** The platform is pinned at build time and is not taken from the
link. A request has to carry that platform's signature and has to be in time,
the answer is good for two minutes, and nothing a link carries is acted on. A
way to get the wallet to fetch from, believe or post to somewhere else is a
finding — and so is a way to get an answer signed for a request nobody
approved.

**Deep links.** `almena://` arrives from outside — a page, a message, anything
that can put a URL in front of somebody. It is untrusted input, and anything it
manages to do beyond being shown is a finding.

**The lock.** It closes when the wallet leaves the screen and opens with the
keypad or the reader. A way past it that does not open the record is a finding.

## What is not

- Anything that needs the device already unlocked and in hand. The wallet is
  not a defence against somebody holding an open phone.
- A rooted or jailbroken device, an unlocked bootloader, or a debug build
  installed deliberately.
- Losing the phrase. There is no recovery and there is not meant to be one.
- Anything about the backend's behaviour. It has its own policy, in its own
  repository.
- Biometrics being absent on Windows, macOS and Linux. Tauri's plugin covers
  Android and iOS; on a computer the lock is the PIN alone and the screen says
  so.
- A Keychain record becoming unreachable after `APPLE_DEVELOPMENT_TEAM` or the
  bundle identifier changed. That is what an access group is, and the way back
  is the phrase.
- Automated scanner output with no reachable consequence attached.

## What this repository expects of itself

- Secrets are wrapped in `Zeroizing` and wiped, not left on a stack.
- Failures are codes, never prose, and never carry a secret in the code.
- The phrase is never persisted. The seed is the only thing written down.
- `.env.local` — which is where `APPLE_DEVELOPMENT_TEAM` lives — is ignored by
  git.
- The platform URL comes from `build.rs` and `env!`, so a build that was not
  told which platform it answers to does not compile.

## Supported versions

There is one line of development and it is `main`. Fixes land there and are
released from there; nothing older is maintained.
