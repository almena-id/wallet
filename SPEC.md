# Almena Wallet Specification

Version: draft 1 (2026-09-25) · Status: describes `wallet` as implemented.

This document specifies the protocols Almena wallets speak **to each other**, on top of [DIDComm Messaging v2.0][didcomm]. What a wallet says to its mediator, and what the mediator requires of it, is the mediator's specification (`SPEC.md` in [almena-network/mediator][mediator]); this one does not restate it.

For now it holds one protocol, calls (§3). The others the wallet speaks — the handshake that opens a relationship, Basic Message 2.0, User Profile 1.0 — are described in `AGENTS.md` until they are written down here.

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as described in [RFC 2119][rfc2119] and [RFC 8174][rfc8174].

## 1. Sources

### 1.1 Normative

| Ref | Document | Used for |
|---|---|---|
| [DIDComm] | [DIF DIDComm Messaging v2.0][didcomm] | Messages, envelopes, threads (`thid`), `expires_time`. |
| [WebRTC] | [W3C WebRTC: Real-Time Communication in Browsers][webrtc] | The API calls are placed with: `RTCPeerConnection`, `RTCIceServer`, `iceTransportPolicy`. |
| [JSEP] | [RFC 8829][rfc8829], JavaScript Session Establishment Protocol | Offers and answers as SDP. |
| [ICE] | [RFC 8445][rfc8445] | Connectivity; here, relayed candidates only. |
| [TURN] | [RFC 8656][rfc8656] | The relay every call goes through. |
| [Mediator] | The Almena Mediator Specification, §6.9 (TURN 1.0) | Where a wallet gets its TURN credentials. |

### 1.2 Informative

- [RFC 8827][rfc8827] (WebRTC security architecture) and [RFC 5763][rfc5763] (DTLS-SRTP keyed by SDP fingerprints) — why an authenticated SDP authenticates the media.

## 2. Relationships

Everything in this document travels inside a relationship: from this wallet's pairwise DID to the counterparty's, authcrypted, wrapped in `forward`s for the counterparty's mediator. A message is taken only when it is authcrypted and its `from` is the counterparty's current pairwise DID; anything else is ignored.

## 3. Calls

Voice and video calls between the two wallets of a relationship, one to one. The media flows directly between the two `RTCPeerConnection`s, encrypted end to end with DTLS-SRTP; this protocol carries only the signalling that sets them up.

| Protocol | PIURI | Messages |
|---|---|---|
| Call 1.0 | `https://almena.id/protocols/call/1.0` | `offer`, `answer`, `hangup` |

The PIURI is a fixed name, the same in every environment.

### 3.1 Relay only

- A wallet MUST create its `RTCPeerConnection` with `iceTransportPolicy: "relay"` and the `ice_servers` its own mediator gave it (TURN 1.0, [Mediator] §6.9), asked for before each call.
- It MUST NOT put a candidate other than `typ relay` in the SDP it sends, and MUST ignore any other in the SDP it receives.

So neither wallet ever learns the other's IP address: each sees only the other's relay. Each wallet's own address is seen only by its own mediator's TURN server.

### 3.2 Messages

A call is a thread: the `offer`'s `id` is the call's id, and `answer` and `hangup` carry it as `thid`.

**`offer`** — the caller asks for a call.

```json
{
  "type": "https://almena.id/protocols/call/1.0/offer",
  "id": "<call id>",
  "created_time": 1760000000,
  "expires_time": 1760000060,
  "body": { "media": "video", "sdp": "v=0\r\n…" }
}
```

- `media`: `audio` or `video`. A video call sends and receives a camera track besides the microphone; an audio call only the microphone.
- `sdp`: the complete offer ([JSEP]), its relayed candidates included — ICE gathering is finished before it is sent; there is no trickle ICE.
- `expires_time` MUST be at most 60 seconds after `created_time`. An offer received after its `expires_time` is ignored: it was a call that is no longer ringing.

**`answer`** — the callee accepts.

```json
{ "type": "https://almena.id/protocols/call/1.0/answer", "thid": "<call id>", "body": { "sdp": "v=0\r\n…" } }
```

`sdp` is the complete answer, gathered in full like the offer.

**`hangup`** — either side ends the call, or declines it before it starts.

```json
{ "type": "https://almena.id/protocols/call/1.0/hangup", "thid": "<call id>", "body": { "reason": "declined" } }
```

| `reason` | Sent by | When |
|---|---|---|
| `ended` | either | The call was hung up after it was answered. |
| `cancelled` | caller | Hung up before an answer. |
| `unanswered` | caller | Nobody answered within 45 seconds. |
| `declined` | callee | Declined. |
| `busy` | callee | Already in another call. |
| `failed` | either | The media could not be set up or was lost. |

An unknown `reason` is read as `ended`. A `hangup` or `answer` for a call that is not the current one is ignored.

### 3.3 Rules

- **One call at a time.** An `offer` that arrives during another call is answered with `hangup` `busy`.
- **Both calling at once.** When a wallet calling a contact receives that contact's `offer` before an answer, the call whose id sorts lower (byte-wise) is kept: the wallet whose own offer has the lower id ignores the other's; the other drops its own offer, sending nothing, and rings with the incoming one.
- **Ringing.** The callee rings until it answers or declines, the caller hangs up, or the offer expires. The caller gives up after 45 seconds with `unanswered`.
- **Media.** Microphone and camera can be muted during the call by disabling the track; that is not signalled and needs no renegotiation. An audio call stays audio.
- **Delivery.** Signalling goes through both mediators like any message, so a call only rings on a wallet that is receiving live (online). Waking a wallet for a call is not specified yet.

### 3.4 Security

- The SDP carries the DTLS fingerprints that key the media ([RFC 5763][rfc5763]). It arrives authcrypted from the counterparty's pairwise DID, so the media is authenticated to that relationship with no further step.
- The TURN servers relay DTLS-SRTP they cannot read. Each sees the addresses of its own wallet and of the other side's relay, and the timing and volume of the call.
- The mediators see only `forward`s: they do not learn that a call took place.

[didcomm]: https://identity.foundation/didcomm-messaging/spec/v2.0/
[mediator]: https://github.com/almena-network/mediator
[webrtc]: https://www.w3.org/TR/webrtc/
[rfc2119]: https://www.rfc-editor.org/rfc/rfc2119
[rfc8174]: https://www.rfc-editor.org/rfc/rfc8174
[rfc8829]: https://www.rfc-editor.org/rfc/rfc8829
[rfc8445]: https://www.rfc-editor.org/rfc/rfc8445
[rfc8656]: https://www.rfc-editor.org/rfc/rfc8656
[rfc8827]: https://www.rfc-editor.org/rfc/rfc8827
[rfc5763]: https://www.rfc-editor.org/rfc/rfc5763
