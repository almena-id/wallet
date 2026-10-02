# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub: on
[almena-id/wallet](https://github.com/almena-id/wallet), open the
**Security** tab and choose **Report a vulnerability**. Do not open a public
issue, pull request or discussion about it.

Include what you can of:

- the platform (Android, iOS, macOS, Linux or Windows), the version or commit,
  and whether it was a debug build or a release bundle;
- what an attacker can do, and what they need to begin with — the device in
  hand, a malicious link or code, a network position, another app on the same
  device;
- steps or a proof of concept to reproduce it.

**Never send real keys or secrets from a wallet you use.** If a report needs
them, make a throwaway wallet and say so.

We aim to acknowledge a report within 3 working days and to agree on a
disclosure date with you once the issue is understood. We credit reporters in
the release notes unless you prefer otherwise.

## Supported versions

The project is before its first release: only the `main` branch receives
security fixes.

## Scope

In scope, among others:

- anything that exposes key material or message content held by the wallet;
- the webview reaching Rust commands or plugin permissions it was not granted
  (`src-tauri/capabilities/`);
- content loaded from outside the app bundle, or links and codes that make the
  wallet act without the person's say-so.

Out of scope:

- anything that needs the device already unlocked and in hand;
- rooted or jailbroken devices, and debug builds installed deliberately;
- the mediator, which has its own policy in
  [almena-id/mediator](https://github.com/almena-id/mediator);
- automated scanner output with no reachable consequence attached.
