import CallKit
import Foundation
import PushKit
import Tauri
import UIKit
import WebKit

/// The wallet's side of a call that rings while it is closed (see the
/// plugin's `src/lib.rs`).
///
/// **PushKit and CallKit together, as Apple requires.** The mediator rings this
/// iPhone with a VoIP push, and every VoIP push must be reported to CallKit as
/// an incoming call before its handler returns, or iOS stops delivering them.
/// The call can only say that somebody is calling: nothing here can read who.
/// Answering opens nothing by itself — the system's call screen offers the app
/// — and the wallet, once unlocked, picks the offer up and answers it
/// (`take`). The CallKit call is ended as soon as the wallet's own call takes
/// over (`settle`), or after as long as the caller rings.
class CallPlugin: Plugin, PKPushRegistryDelegate, CXProviderDelegate {
  private var registry: PKPushRegistry?
  private var provider: CXProvider?
  private var pushKitToken: String?
  private var ringing: UUID?
  private var pending: (action: String, at: Date)?

  /// As long as the caller's wallet rings (`RING_MS` in `src/call.ts`).
  private let ringSeconds: TimeInterval = 45
  /// As long as an offer lives (`OFFER_TTL` in `messaging/call.rs`).
  private let freshSeconds: TimeInterval = 60

  public override func load(webview: WKWebView) {
    let configuration = CXProviderConfiguration()
    configuration.supportsVideo = true
    configuration.maximumCallGroups = 1
    configuration.maximumCallsPerCallGroup = 1
    configuration.supportedHandleTypes = [.generic]
    let provider = CXProvider(configuration: configuration)
    provider.setDelegate(self, queue: nil)
    self.provider = provider

    let registry = PKPushRegistry(queue: .main)
    registry.delegate = self
    registry.desiredPushTypes = [.voIP]
    self.registry = registry
  }

  // MARK: PushKit

  func pushRegistry(_ registry: PKPushRegistry, didUpdate credentials: PKPushCredentials, for type: PKPushType) {
    pushKitToken = credentials.token.map { String(format: "%02x", $0) }.joined()
  }

  func pushRegistry(_ registry: PKPushRegistry, didInvalidatePushTokenFor type: PKPushType) {
    pushKitToken = nil
  }

  func pushRegistry(
    _ registry: PKPushRegistry,
    didReceiveIncomingPushWith payload: PKPushPayload,
    for type: PKPushType,
    completion: @escaping () -> Void
  ) {
    let call = UUID()
    let update = CXCallUpdate()
    update.remoteHandle = CXHandle(type: .generic, value: "Almena")
    update.localizedCallerName = NSLocalizedString("almena_call_title", comment: "")
    update.hasVideo = false
    update.supportsHolding = false
    update.supportsGrouping = false
    update.supportsUngrouping = false
    update.supportsDTMF = false
    // Reported whatever else is ringing: an unreported VoIP push is one iOS
    // holds against the app. A second call while one rings is ended at once.
    provider?.reportNewIncomingCall(with: call, update: update) { [weak self] error in
      guard let self = self else { return completion() }
      if error == nil {
        if self.ringing != nil {
          self.provider?.reportCall(with: call, endedAt: nil, reason: .unanswered)
        } else {
          self.ringing = call
          DispatchQueue.main.asyncAfter(deadline: .now() + self.ringSeconds) { [weak self] in
            self?.endRinging(call, reason: .unanswered)
          }
        }
      }
      completion()
    }
  }

  // MARK: CallKit

  func providerDidReset(_ provider: CXProvider) {
    ringing = nil
  }

  func provider(_ provider: CXProvider, perform action: CXAnswerCallAction) {
    pending = ("answer", Date())
    action.fulfill()
    trigger("call", data: ["action": "answer"])
  }

  func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
    // Declined, or hung up from the system's screen: nothing to send — a
    // closed wallet has no key to say it with — so the caller rings out.
    if pending?.action != "answer" {
      pending = nil
    }
    if ringing == action.callUUID {
      ringing = nil
    }
    action.fulfill()
  }

  private func endRinging(_ call: UUID, reason: CXCallEndedReason) {
    guard ringing == call else { return }
    ringing = nil
    provider?.reportCall(with: call, endedAt: Date(), reason: reason)
  }

  // MARK: Commands

  @objc func take(_ invoke: Invoke) {
    guard let taken = pending, Date().timeIntervalSince(taken.at) <= freshSeconds else {
      pending = nil
      invoke.resolve([:])
      return
    }
    pending = nil
    invoke.resolve([
      "action": taken.action,
      "at": Int(taken.at.timeIntervalSince1970 * 1000),
    ])
  }

  @objc func settle(_ invoke: Invoke) {
    pending = nil
    if let call = ringing {
      endRinging(call, reason: .answeredElsewhere)
    }
    invoke.resolve()
  }

  @objc func voipToken(_ invoke: Invoke) {
    invoke.resolve(["token": pushKitToken as Any])
  }
}

@_cdecl("init_plugin_almena_call")
func initPlugin() -> Plugin {
  return CallPlugin()
}
