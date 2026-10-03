//! Live delivery: a WebSocket to the mediator while the wallet is in front, so
//! messages arrive as they are sent instead of when somebody asks.
//!
//! **The socket is how messages arrive, not where they are handled.** What the
//! mediator pushes is a `delivery` like the ones a sync asks for, and it goes
//! through the same [`super::handle`] and [`super::apply`], under the same
//! [`super::Gate`], and is acknowledged the same way — only over the socket.
//! The mediator keeps a pushed message queued until it is acknowledged, so a
//! socket that drops loses nothing: the next sync picks it up.
//!
//! What was queued before live delivery began is not pushed, so each session
//! starts with a sync. After that the socket is quiet until something arrives;
//! a ping every [`PING_EVERY`] tells a socket that died without closing from
//! one that has nothing to say.
//!
//! The interface starts it when the wallet comes to the front and stops it
//! when it goes to the back (the mediator's SPEC.md §7.2); signing out
//! stops it too. Between sessions it waits, [`BACKOFF_MIN`] doubling up to
//! [`BACKOFF_MAX`], and tries again — unless there is nothing to be live for:
//! no identity open, no mediation, or a mediator with no socket.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use almena_didcomm::Message;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tauri::async_runtime::JoinHandle;
use tauri::{Emitter, Manager, Runtime, State};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message as Frame;
use tokio_tungstenite::{
    connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream,
};

use super::mediator::{self, Mediator};
use super::peer::Peer;
use super::{apply, handle, listed, seed, state, sync, Gate, MessagingError, Outcome, Synced};
use crate::identity::Held;

const LIVE_DELIVERY_CHANGE: &str = "https://didcomm.org/messagepickup/3.0/live-delivery-change";
const MESSAGES_RECEIVED: &str = "https://didcomm.org/messagepickup/3.0/messages-received";

/// The event the interface listens to: a [`Synced`], sent whenever what
/// arrived changed something.
pub const CHANGED: &str = "messaging-changed";

/// How long connecting, and the mediator's answer to going live, may take.
const HANDSHAKE: Duration = Duration::from_secs(20);
/// How long the socket may be quiet before it is pinged.
const PING_EVERY: Duration = Duration::from_secs(30);
/// How long without hearing anything — a ping's pong included — before the
/// socket counts as dead.
const SILENCE: Duration = Duration::from_secs(75);
const BACKOFF_MIN: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// A session that lasted this long was a good one: the wait before the next
/// starts again from [`BACKOFF_MIN`].
const STEADY: Duration = Duration::from_secs(60);

/// The running session, if any.
#[derive(Default)]
pub struct Live(Mutex<Option<JoinHandle<()>>>);

impl Live {
    /// Starts a session, ending the one running, if any.
    pub fn restart<R: Runtime>(&self, app: tauri::AppHandle<R>) {
        // One lock across the abort, the spawn and the store: two restarts at
        // once would otherwise each stop nothing and both start, and the one
        // stored second would orphan the other's socket.
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(running) = slot.take() {
            running.abort();
        }
        *slot = Some(tauri::async_runtime::spawn(run(app)));
    }

    /// Ends the session, if there is one.
    pub fn stop(&self) {
        let running = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(running) = running {
            running.abort();
        }
    }
}

/// Starts live delivery, replacing a session already running: a wallet coming
/// back to the front may be holding a socket the system closed under it.
#[tauri::command]
pub fn live_start<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    live: State<'_, Live>,
) -> Result<(), MessagingError> {
    seed(&held)?;
    live.restart(app);
    Ok(())
}

/// Stops live delivery.
#[tauri::command]
pub fn live_stop(live: State<'_, Live>) {
    live.stop();
}

/// Sessions, one after another, until there is nothing to be live for.
async fn run<R: Runtime>(app: tauri::AppHandle<R>) {
    let mut backoff = BACKOFF_MIN;
    loop {
        let started = Instant::now();
        match session(&app).await {
            Ok(()) => return,
            Err(
                MessagingError::Locked | MessagingError::NotConnected | MessagingError::Unreadable,
            ) => return,
            Err(_) => {}
        }
        if started.elapsed() >= STEADY {
            backoff = BACKOFF_MIN;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// One socket, from connecting until it fails. Returns `Ok` only when the
/// mediator has no socket to open.
async fn session<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), MessagingError> {
    let held = app.state::<Held>();
    let gate = app.state::<Gate>();

    let seed = seed(&held)?;
    let mediation = state::read(app, &seed)?
        .mediation
        .ok_or(MessagingError::NotConnected)?;
    let mediator = Mediator::resolve(&mediation.mediator).await?;
    if mediator.socket.is_none() {
        return Ok(());
    }
    let inbox = Peer::inbox(&seed, &mediator.did)?;
    let card = Peer::card(&seed, &mediator.did)?;
    mediator.ensure(&inbox, None).await?;

    let mut socket = Socket::open(&mediator, &inbox).await?;

    // Live first, then the backlog: a message sent in between is pushed, and
    // one that is both pushed and fetched is recognised by its id.
    {
        let _guard = gate.0.lock().await;
        let mut state = state::read(app, &seed)?;
        let outcome = sync(&seed, &mut state).await?;
        let applied = apply(app, &seed, &mut state, outcome)?;
        announce(app, applied.changed, &state);
        crate::notify::arrived(app, &applied.fresh);
    }

    loop {
        let deliveries = socket.deliveries(&mediator, &inbox).await?;
        // Signed out while this was waiting: nothing more is handled.
        let seed = self::seed(&held)?;

        let received = {
            let _guard = gate.0.lock().await;
            let mut state = state::read(app, &seed)?;
            let mut outcome = Outcome::default();
            let received = handle(
                &seed,
                &mut state,
                &mediator,
                &inbox,
                &card,
                deliveries,
                &mut outcome,
            )
            .await?;
            let applied = apply(app, &seed, &mut state, outcome)?;
            announce(app, applied.changed, &state);
            crate::notify::arrived(app, &applied.fresh);
            received
        };
        socket.acknowledge(&mediator, &inbox, &received).await?;
    }
}

/// Tells the interface, when anything changed.
fn announce<R: Runtime>(app: &tauri::AppHandle<R>, changed: usize, state: &state::State) {
    if changed > 0 {
        let _ = app.emit(
            CHANGED,
            Synced {
                changed,
                contacts: listed(&state.relationships),
            },
        );
    }
}

/// A WebSocket to the mediator with live delivery on for the inbox's
/// mediation.
pub struct Socket {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    heard: Instant,
}

impl Socket {
    /// Connects to the mediator's socket and turns live delivery on.
    ///
    /// # Errors
    ///
    /// [`MessagingError::NotConnected`] when the mediator has no socket,
    /// [`MessagingError::MediatorUnreachable`] when it cannot be reached or
    /// does not answer in time, and [`MessagingError::MediatorRefused`] when it
    /// will not go live.
    pub async fn open(mediator: &Mediator, inbox: &Peer) -> Result<Self, MessagingError> {
        let uri = mediator
            .socket
            .as_deref()
            .ok_or(MessagingError::NotConnected)?;
        let connector = if uri.starts_with("wss:") {
            Connector::Rustls(mediator::tls()?)
        } else {
            Connector::Plain
        };
        let (stream, _) = timeout(
            HANDSHAKE,
            connect_async_tls_with_config(uri, None, true, Some(connector)),
        )
        .await
        .map_err(|_| MessagingError::MediatorUnreachable)?
        .map_err(|_| MessagingError::MediatorUnreachable)?;

        let mut socket = Self {
            stream,
            heard: Instant::now(),
        };
        socket
            .send(
                mediator,
                inbox,
                Message::new(LIVE_DELIVERY_CHANGE, json!({"live_delivery": true})),
            )
            .await?;
        let status = timeout(HANDSHAKE, socket.next(mediator, inbox))
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)??;
        if !status.type_.ends_with("/status") || status.body["live_delivery"] != true {
            return Err(MessagingError::MediatorRefused);
        }
        Ok(socket)
    }

    /// Waits for the next `delivery` the mediator pushes.
    pub async fn deliveries(
        &mut self,
        mediator: &Mediator,
        inbox: &Peer,
    ) -> Result<Vec<(String, String)>, MessagingError> {
        loop {
            match self.next(mediator, inbox).await {
                Ok(message) => {
                    let deliveries = mediator::delivered(message);
                    if !deliveries.is_empty() {
                        return Ok(deliveries);
                    }
                }
                // A problem report — about an acknowledgement, say — is not a
                // reason to close the socket.
                Err(MessagingError::MediatorRefused) => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Acknowledges messages, which removes them from the queue.
    pub async fn acknowledge(
        &mut self,
        mediator: &Mediator,
        inbox: &Peer,
        ids: &[String],
    ) -> Result<(), MessagingError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.send(
            mediator,
            inbox,
            Message::new(MESSAGES_RECEIVED, json!({"message_id_list": ids})),
        )
        .await
    }

    async fn send(
        &mut self,
        mediator: &Mediator,
        inbox: &Peer,
        message: Message,
    ) -> Result<(), MessagingError> {
        let packed = mediator.pack(inbox, message).await?;
        self.stream
            .send(Frame::text(packed))
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)
    }

    /// The next message from the mediator, pinging it while it is quiet. What
    /// does not open is skipped; a problem report is
    /// [`MessagingError::MediatorRefused`].
    async fn next(&mut self, mediator: &Mediator, inbox: &Peer) -> Result<Message, MessagingError> {
        loop {
            let frame = match timeout(PING_EVERY, self.stream.next()).await {
                Ok(frame) => frame,
                Err(_) if self.heard.elapsed() >= SILENCE => {
                    return Err(MessagingError::MediatorUnreachable)
                }
                Err(_) => {
                    self.stream
                        .send(Frame::Ping(Default::default()))
                        .await
                        .map_err(|_| MessagingError::MediatorUnreachable)?;
                    continue;
                }
            };
            let frame = match frame {
                Some(Ok(frame)) => frame,
                Some(Err(_)) | None => return Err(MessagingError::MediatorUnreachable),
            };
            self.heard = Instant::now();

            let envelope = match &frame {
                Frame::Text(text) => text.as_str(),
                Frame::Binary(bytes) => match std::str::from_utf8(bytes) {
                    Ok(text) => text,
                    Err(_) => continue,
                },
                Frame::Close(_) => return Err(MessagingError::MediatorUnreachable),
                _ => continue,
            };
            match mediator.open(inbox, envelope).await {
                Ok(message) => return Ok(message),
                Err(MessagingError::MediatorRefused) => {
                    return Err(MessagingError::MediatorRefused)
                }
                Err(_) => {}
            }
        }
    }
}
