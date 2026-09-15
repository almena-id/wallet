# Contributing

This repository is public to read. It is not open to unsolicited pull requests.

Almena ID is built by one maintainer, and the reasoning behind every decision
lives in the code and in the tests rather than in a specification anybody could
be handed. A patch that arrives without that reasoning costs more to place than
to write, so pull requests that were not asked for are closed — not because
they are unwelcome as work, but because there is nowhere to put them yet.

## What is useful instead

- **Bugs.** Open an issue with the platform, the version, what you did and what
  happened. **Never include a recovery phrase** — not yours, not one you found
  in a log.
- **Vulnerabilities.** Not an issue: [SECURITY.md](SECURITY.md).
- **Wording that is wrong in Spanish or in English.** Say which key in
  `src/i18n/messages/` and what it should say.

If a change is agreed on an issue first, a pull request for it is welcome. The
rest of this file is what that change has to hold to.

## The rules a change is held to

**Everything that lands is English** — identifiers, file names, comments, commit
messages, and every translation key. The Spanish in this repository is inside
`src/i18n/messages/es.json` and nowhere else.

**`task check` passes.** TypeScript, the Rust checks — including
`rust:check:mobile`, which covers the code only the mobile builds compile — and
the Rust tests.

**The derivation path is frozen.** BIP-39 to the seed, SLIP-0010 over ed25519,
one hardened step at `m/0'`, `did:key` for the identifier. A test pins a phrase
to the identifier it has to keep producing. If you find yourself needing to
change it, the change is not this one.

**Secrets are wrapped and wiped.** A seed, a phrase or a derived key lives in
`Zeroizing`, is never logged, never crosses to the frontend unless the screen
exists to show it, and is never written anywhere but the record.

**The phrase is never persisted.** It is held between being shown and being
confirmed and then it is gone. The seed is the only thing the wallet writes
down.

**Every PIN check goes through the one function that spends an attempt.** A
check that does not count is a wallet that can be guessed at forever through
whichever screen forgot to count.

**The record's format carries its own parameters.** A wallet installed over an
older one has to read what that one wrote, so nothing is implied by the code
that reads it, and a record from a newer wallet is refused by name rather than
misread. Changing the format means bumping the version and reading the old one.

**Failures are codes, not prose.** The Rust side names what went wrong; the
catalogues say it in the language somebody is reading.

**No hardcoded user-facing string, ever.** A new string is a key in
`src/i18n/messages/en.json` and `es.json`, English first, named after what it is
rather than after what it currently says. A feature with strings in only one
catalogue is not done. The BIP-39 wordlists are a separate thing and follow the
interface language, not the operating system directly.

**No explanatory copy.** Labels, instructions, consequences and errors are what
a screen is allowed to say. A key that exists to explain a control should not
have been created — fix the control instead.

**A destructive action is deliberate.** Signing out lets go of the identity and
the twelve words are the only way back, so the screen says so, refuses to arm
for ten seconds and then asks for a drag rather than a tap. Anything with that
weight is built the same way.

**A plugin is registered where the platform provides it**, and the frontend asks
the Rust side what it got rather than sniffing the user agent.

**Android's launcher icons are drawn by hand and are only ever copied forward.**
`task icons` regenerates everything else. `icon.icns` comes out byte-different
on every run because the Tauri CLI writes its entries in a random order; a diff
limited to that file can be dropped.

## Getting set up

[README.md](README.md) has the requirements, the desktop loop, and the one-off
`init:android` and `init:ios` steps. `task` on its own lists everything.

By contributing you agree that your contribution is licensed under
[Apache-2.0](LICENSE), and that you behave as [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
describes.
