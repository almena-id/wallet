import { useCallback, useEffect, useRef, useState } from "react";

import { PhoneIcon, SyncIcon, VideoIcon } from "../components/icons";
import { calls, callsSupported, useCall } from "../call";
import { useI18n } from "../i18n";
import {
  MESSAGE_CHARS,
  collectCredential,
  markSeen,
  onMessagesChanged,
  readConversation,
  retryMessage,
  sendMessage,
  syncMessages,
  type Conversation,
  type Entry,
  type Notice,
} from "../contacts";
import { errorCode } from "../mediator";
import { usePlatform } from "../platform";
import { fill } from "../i18n/format";
import { moment } from "../when";
import { initial } from "./MessagesScreen";
import { BackButton } from "../components/BackButton";

type ConversationScreenProps = {
  id: string;
  onBack: () => void;
  /** Opens the contact, where it is renamed. */
  onContact: () => void;
  /** Puts an `almena://` link to the person, as one from outside would be. */
  onLink: (url: string) => void;
};

/**
 * One conversation: what was said, oldest first, and the box to say more.
 *
 * What is on the device is drawn at once and the mediator is asked as it
 * opens; a message that did not go stays in the conversation, marked, to be
 * tried again.
 */
export function ConversationScreen({ id, onBack, onContact, onLink }: ConversationScreenProps) {
  const { t, locale } = useI18n();
  const [conversation, setConversation] = useState<Conversation | null>(null);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const end = useRef<HTMLDivElement>(null);
  // Held for the whole send, and read synchronously: `sending` only changes on
  // the next render, so two quick presses of Enter would both see it false.
  const busy = useRef(false);
  const call = useCall();
  const platform = usePlatform();

  // Each read is numbered, and only the latest one is shown: the live event and
  // a sync can both ask, and an older answer must not land after a newer one.
  const reads = useRef(0);
  const load = useCallback(() => {
    const ticket = ++reads.current;
    return readConversation(id)
      .then((read) => {
        if (ticket !== reads.current) {
          return;
        }
        setConversation(read);
        if (read.contact.unread > 0) {
          void markSeen(id).catch(() => undefined);
        }
      })
      .catch((failure) => setError(t.messaging.errors[errorCode(failure)]));
  }, [id, t]);

  const sync = useCallback(async () => {
    setSyncing(true);
    setError(null);
    try {
      if ((await syncMessages()).changed > 0) {
        await load();
      }
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
    } finally {
      setSyncing(false);
    }
  }, [load, t]);

  useEffect(() => {
    void load().then(sync);
  }, [load, sync]);

  // What arrives live while the conversation is open is read here and now.
  useEffect(() => onMessagesChanged(() => void load()), [load]);

  const count = conversation?.entries.length ?? 0;
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [count]);

  function adopt(entry: Entry) {
    setConversation((current) =>
      current
        ? {
            ...current,
            entries: [...current.entries.filter((e) => e.id !== entry.id), entry].sort(
              (a, b) => a.at - b.at,
            ),
          }
        : current,
    );
  }

  async function send() {
    const content = draft.trim();
    if (content.length === 0 || busy.current) {
      return;
    }
    busy.current = true;
    setSending(true);
    setError(null);
    try {
      adopt(await sendMessage(id, content));
      setDraft("");
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
    } finally {
      busy.current = false;
      setSending(false);
    }
  }

  async function retry(entry: Entry) {
    if (busy.current) {
      return;
    }
    busy.current = true;
    setSending(true);
    setError(null);
    try {
      adopt(await retryMessage(id, entry.id));
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
    } finally {
      busy.current = false;
      setSending(false);
    }
  }

  // An issued notice's credential: the issuer answers with the request to
  // receive it, which goes to the sheet that asks, like any link.
  const [collecting, setCollecting] = useState<string | null>(null);
  async function collect(entry: Entry) {
    setCollecting(entry.id);
    setError(null);
    try {
      onLink(await collectCredential(id, entry.id));
    } catch (failure) {
      const errors = t.confirm.registry.errors;
      setError(errors[errorCode(failure) as keyof typeof errors] ?? null);
    } finally {
      setCollecting(null);
    }
  }

  // What an issuer's notice says, in this wallet's language.
  function noticeText(notice: Notice): string {
    const credential =
      notice.credentialName[locale] ?? notice.credentialName.en ?? notice.credentialType;
    return fill(t.conversations.notice[notice.status], { issuer: notice.issuer, credential });
  }

  const contact = conversation?.contact;
  // Once they have answered the invitation, and not while another call is on.
  const callable = !!contact && !contact.pending && (call === null || call.phase === "ended");

  return (
    <div className="screen screen--conversation">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <button
          type="button"
          className="conversation__who"
          onClick={onContact}
          disabled={!contact}
          aria-label={t.conversations.chat.details}
        >
          <span className="row__icon row__icon--initial" aria-hidden="true">
            {contact ? initial(contact.name) : ""}
          </span>
          <span className="conversation__name">{contact?.name ?? ""}</span>
        </button>
        {callsSupported ? (
          <>
            <button
              type="button"
              className="icon-button"
              onClick={() => contact && void calls.start(id, contact.name, "audio")}
              disabled={!callable}
              aria-label={t.calls.audio}
            >
              <PhoneIcon />
            </button>
            <button
              type="button"
              className="icon-button"
              onClick={() => contact && void calls.start(id, contact.name, "video")}
              disabled={!callable}
              aria-label={t.calls.video}
            >
              <VideoIcon />
            </button>
          </>
        ) : null}
        <button
          type="button"
          className="icon-button"
          onClick={() => void sync()}
          disabled={syncing}
          aria-label={t.messages.sync}
        >
          <SyncIcon className={syncing ? "icon-button__spin" : undefined} />
        </button>
      </header>

      {error ? <p className="card__note card__note--warning">{error}</p> : null}

      {conversation === null ? null : (
        <section className="bubbles" role="log" aria-label={t.conversations.chat.history}>
          {conversation.entries.length === 0 ? (
            <p className="bubbles__empty">
              {contact?.pending ? t.conversations.chat.pending : t.conversations.chat.empty}
            </p>
          ) : (
            conversation.entries.map((entry) => (
              <div
                key={entry.id}
                className={`bubble${entry.mine ? " bubble--mine" : ""}${entry.failed ? " bubble--failed" : ""}`}
              >
                {entry.notice ? (
                  <>
                    <p className="bubble__text">{noticeText(entry.notice)}</p>
                    {entry.notice.note ? (
                      <p className="bubble__text bubble__note">
                        {t.conversations.notice.note}: {entry.notice.note}
                      </p>
                    ) : null}
                    {entry.notice.status === "issued" && entry.notice.collect ? (
                      <button
                        type="button"
                        className="bubble__action"
                        onClick={() => void collect(entry)}
                        disabled={collecting !== null}
                      >
                        {collecting === entry.id
                          ? t.conversations.notice.collecting
                          : t.conversations.notice.collect}
                      </button>
                    ) : null}
                  </>
                ) : (
                  <p className="bubble__text">{entry.content}</p>
                )}
                <span className="bubble__time">{moment(entry.at, locale)}</span>
                {entry.failed ? (
                  <button
                    type="button"
                    className="bubble__retry"
                    onClick={() => void retry(entry)}
                    disabled={sending}
                  >
                    {t.conversations.chat.failed}
                  </button>
                ) : null}
              </div>
            ))
          )}
          <div ref={end} />
        </section>
      )}

      <form
        className="composer"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <textarea
          className="field__input composer__input"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            // Enter sends and Shift+Enter breaks the line, where there is a
            // keyboard with both; a phone's return key only ever breaks it.
            if (
              platform.kind !== "mobile" &&
              event.key === "Enter" &&
              !event.shiftKey &&
              !event.nativeEvent.isComposing
            ) {
              event.preventDefault();
              void send();
            }
          }}
          placeholder={contact?.pending ? t.conversations.chat.pendingShort : t.conversations.chat.placeholder}
          aria-label={t.conversations.chat.placeholder}
          maxLength={MESSAGE_CHARS}
          rows={1}
          disabled={!contact || contact.pending}
        />
        <button
          type="submit"
          className="button button--primary"
          disabled={!contact || contact.pending || sending || draft.trim().length === 0}
        >
          {t.conversations.chat.send}
        </button>
      </form>
    </div>
  );
}
