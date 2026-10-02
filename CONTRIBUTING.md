# Contributing

Thanks for helping with the Almena ID wallet. By taking part you agree
to follow the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues go
through [SECURITY.md](SECURITY.md), never through public issues.

## Getting started

You need Node with pnpm, a recent stable Rust and [Task](https://taskfile.dev),
plus the platform toolchains listed in the [README](README.md) for Android and
iOS.

```bash
task dev              # the desktop app with hot reload
task dev:web          # only the front end, in a browser
task deploy:android   # build and install on an Android device or emulator
task deploy:ios       # build and install on an iOS device or simulator
task --list           # everything else
```

## Making a change

- Open an issue first for anything larger than a small fix, so the approach
  can be agreed before the code.
- Everything is written in English: code, comments, docs, commit messages.
- No hard-coded user-facing text. A new string is a key in
  `src/i18n/messages/en.json` and `es.json`; a change with strings in only one
  catalogue is not done.
- Colours live only in `src/styles/global.css`, as tokens; components never
  name a colour of their own.
- Platform differences are decided on the Rust side with `cfg`, not by
  sniffing the user agent in the front end.
- The Android launcher icons under `src-tauri/icons/android` are drawn by
  hand; `task icons` regenerates everything else from `assets/branding`.
- Before sending a change, `task check` must pass: TypeScript, formatting,
  clippy with warnings as errors, and the Rust tests. If you touched code that
  only the phones compile, run `task check:mobile` too.

[AGENTS.md](AGENTS.md) describes the layout of the code.

## Pull requests

Keep a pull request to one topic, describe what it changes and why, and link
the issue it addresses. Say which platforms you ran it on. By contributing you
agree that your contribution is licensed under the
[Apache License 2.0](LICENSE), as the rest of the project.
