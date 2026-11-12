# Almena Wallet Specification

Version: draft 3 (2026-11-11) · Status: describes `wallet` as implemented.

This document specifies the protocols Almena wallets speak **to each other**, on top of [DIDComm Messaging v2.0][didcomm]. What a wallet says to its mediator, and what the mediator requires of it, is the mediator's specification (`SPEC.md` in [almena-id/mediator][mediator]); this one does not restate it.

For now it holds only the rule every such protocol shares (§2). The protocols the wallet speaks — the handshake that opens a relationship, Basic Message 2.0, User Profile 1.0 — are described in `AGENTS.md` until they are written down here. Calls (Call 1.0, `https://almena.id/protocols/call/1.0`) were specified here until 2026-11-10, when they were removed from the wallet; a message of that protocol from an older wallet is now a type this wallet does not speak, acknowledged and dropped.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as described in [RFC 2119][rfc2119] and [RFC 8174][rfc8174].

## 1. Sources

| Ref | Document | Used for |
|---|---|---|
| [DIDComm] | [DIF DIDComm Messaging v2.0][didcomm] | Messages, envelopes, threads (`thid`), `expires_time`. |
| [Mediator] | The Almena Mediator Specification | How messages reach a wallet through its mediator. |

## 2. Relationships

Everything in this document travels inside a relationship: from this wallet's pairwise DID to the counterparty's, authcrypted, wrapped in `forward`s for the counterparty's mediator. A message is taken only when it is authcrypted and its `from` is the counterparty's current pairwise DID; anything else is ignored.

## 3. Moving a relationship

Each relationship goes through the wallet's mediator: its pairwise DID is a `did:peer:2` whose service is that mediator, with keys derived for it alone. When the wallet changes mediator, it moves each relationship:

1. derives the pairwise DID for the same counterparty at the new mediator and registers it there;
2. sends the counterparty, from the new DID, a message carrying `from_prior` ([DIDComm] "DID Rotation"): a JWT whose `iss` is the old pairwise DID and `sub` the new one, signed by the old DID's authentication key. The message is a User Profile 1.0 `profile` with the wallet's current name, so the move also says who is moving;
3. keeps collecting the old DID at the old mediator for a grace period — the mediator's queue lifetime, 30 days — so whatever the counterparty sent before it learned of the move still arrives; then it unregisters the old DID there.

A wallet receiving any message whose `from_prior` is valid, whose `iss` is the counterparty's current DID in a relationship the message is for, and whose `sub` is the message's `from`, MUST take `sub` as the counterparty's DID from then on, and SHOULD write to it alone. The rotation the contact card signs when a relationship opens is the same rule. A move that cannot be told — the counterparty unreachable — is not made yet: it is tried again while the old mediator is still collected.

[didcomm]: https://identity.foundation/didcomm-messaging/spec/v2.0/
[mediator]: https://github.com/almena-id/mediator
[rfc2119]: https://www.rfc-editor.org/rfc/rfc2119
[rfc8174]: https://www.rfc-editor.org/rfc/rfc8174
