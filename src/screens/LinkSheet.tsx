import { useEffect, useState } from "react";

import { ConfirmSheet, type ConfirmDetail } from "../components/ConfirmSheet";
import { CredentialIcon, ProfileIcon, ServerIcon } from "../components/icons";
import { useI18n } from "../i18n";
import { plural } from "../i18n/format";
import { acceptInvitation } from "../contacts";
import {
  choiceToSend,
  invitationKind,
  linkDetails,
  pickedOption,
  registryAnswer,
  registryErrorCode,
  registryRequest,
  type LinkDetails,
  type PresentChoice,
  type Presenting,
  type RegistryRequest,
} from "../links";
import { connectMediator, errorCode, mediatorName } from "../mediator";

/** Where a link or a code came from, said at the top of the sheet. */
export type LinkOrigin = "qr" | "link";

type LinkSheetProps = {
  url: string;
  origin: LinkOrigin;
  onClose: () => void;
  /** A relationship was opened, or an existing one asked for: its conversation. */
  onConversation: (id: string | null) => void;
  /** The wallet now uses the mediator the link named. */
  onMediatorConnected: () => void;
};

/** What the sheet has read: the device's details, or a registry's request. */
type Read =
  | LinkDetails
  | { kind: "registry"; request: RegistryRequest }
  | { kind: "registryFailed"; code: string };

/**
 * What a scanned code or an opened link asks, put to the person over the open
 * screen: somebody's invitation, a mediator's, or a registry portal's request
 * to sign in or to link this wallet. The details come from the Rust side,
 * which reads the link without acting on it; Accept is what acts.
 */
export function LinkSheet({
  url,
  origin,
  onClose,
  onConversation,
  onMediatorConnected,
}: LinkSheetProps) {
  const { t, locale } = useI18n();
  const [details, setDetails] = useState<Read | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Which credential goes, where several would answer: the first until the
  // holder picks another.
  const [choice, setChoice] = useState<PresentChoice>({});

  useEffect(() => {
    void (async () => {
      if ((await invitationKind(url)) === "registry") {
        // Read over the network: who asks, and for what.
        await registryRequest(url)
          .then((request) => setDetails({ kind: "registry", request }))
          .catch((failure: unknown) =>
            setDetails({ kind: "registryFailed", code: registryErrorCode(failure) }),
          );
        return;
      }
      await linkDetails(url)
        .then(setDetails)
        .catch(() => setDetails({ kind: "unknown" }));
    })();
  }, [url]);

  // Nothing the wallet can read: there is no question to ask.
  useEffect(() => {
    if (details?.kind === "unknown") {
      onClose();
    }
  }, [details, onClose]);

  if (details === null || details.kind === "unknown") {
    return null;
  }

  const common = {
    origin: t.confirm.origin[origin],
    cancelLabel: t.confirm.cancel,
    error,
    busy,
    onCancel: onClose,
  };

  async function act(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
      setBusy(false);
    }
  }

  if (details.kind === "registry" || details.kind === "registryFailed") {
    const copy = t.confirm.registry;
    // In this language, else English, else whatever it has.
    const say = (texts: Record<string, string>) =>
      texts[locale] ?? texts.en ?? Object.values(texts)[0] ?? "";
    // What goes, set by set, as the holder has picked it.
    const presentedRows = (presenting: Presenting[]): ConfirmDetail[] =>
      presenting.flatMap((one) => {
        const option = pickedOption(one, choice);
        return [
          { label: copy.presents, value: `${say(option.credential)} · ${option.issuer}` },
          ...option.claims.map((claim) => ({ label: claim.name, value: claim.value })),
        ];
      });
    // Where more than one credential would do, the holder picks which.
    const picker = (presenting: Presenting[]) =>
      presenting
        .filter((one) => one.options.length > 1)
        .map((one) => {
          const picked = pickedOption(one, choice);
          return (
            <div className="sheet__choice" key={one.set} role="radiogroup" aria-label={copy.choose}>
              <p className="card__subtitle">{copy.choose}</p>
              {one.options.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  role="radio"
                  aria-checked={option.id === picked.id}
                  className={
                    option.id === picked.id ? "segmented__option is-active" : "segmented__option"
                  }
                  disabled={busy}
                  onClick={() => setChoice((current) => ({ ...current, [one.set]: option.id }))}
                >
                  {`${say(option.credential)} · ${option.issuer} · ${new Date(
                    option.issuedAt * 1000,
                  ).toLocaleDateString(locale)}`}
                </button>
              ))}
            </div>
          );
        });
    const request = details.kind === "registry" ? details.request : null;
    const failed =
      details.kind === "registryFailed"
        ? copy.errors[details.code as keyof typeof copy.errors]
        : null;
    const verifying = request?.verifying ?? null;
    if (request && verifying) {
      // Nothing held answers it: it is shown, and cannot be accepted.
      const nothing = verifying.presenting.length === 0;
      const asks = say(verifying.form);
      // The name is the registry's word; what the wallet checked itself is
      // said beside it, and so is not having been able to.
      const rows: ConfirmDetail[] = [
        { label: copy.verifier, value: verifying.verifier },
        {
          label: copy.verifierChecked,
          value: !verifying.verified
            ? copy.verifierUnverified
            : verifying.domains.length > 0
              ? verifying.domains.join(", ")
              : copy.verifierResolved,
        },
        { label: copy.verifierDid, value: verifying.verifierDid, mono: true },
        ...(verifying.tenant
          ? [{ label: copy.verifierTenant, value: verifying.tenant, mono: true }]
          : []),
        ...(asks ? [{ label: copy.asks, value: asks }] : []),
        ...presentedRows(verifying.presenting),
        { label: copy.portal, value: request.portal },
      ];
      return (
        <ConfirmSheet
          {...common}
          error={error ?? failed}
          icon={<CredentialIcon />}
          title={copy.verifyTitle.replace("{verifier}", verifying.verifier)}
          lead={copy.verifyLead.replace("{verifier}", verifying.verifier)}
          details={rows}
          note={
            nothing
              ? copy.nothingHeld
              : !verifying.complete
                ? copy.someMissing
                : !verifying.verified
                  ? copy.verifierUnverifiedNote
                  : copy.verifyNote.replace("{verifier}", verifying.verifier)
          }
          confirmDisabled={nothing}
          confirmLabel={copy.present}
          onConfirm={() =>
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await registryAnswer(url, choiceToSend(verifying.presenting, choice));
                onClose();
              } catch (failure) {
                setError(copy.errors[registryErrorCode(failure)]);
                setBusy(false);
              }
            })()
          }
        >
          {picker(verifying.presenting)}
        </ConfirmSheet>
      );
    }
    const applying = request?.applying ?? null;
    if (request && applying) {
      const purpose = request.purpose as "pair" | "present" | "submit" | "receive";
      const presenting = purpose === "present";
      // Nothing held answers the request: it is shown, and cannot be accepted.
      const nothing = presenting && applying.presenting.length === 0;
      const rows: ConfirmDetail[] = [
        { label: copy.issuer, value: applying.issuer },
        { label: copy.credential, value: say(applying.credential) },
        ...(presenting ? presentedRows(applying.presenting) : []),
        ...(purpose === "submit"
          ? applying.answers.map((answer) => ({
              label: say(answer.label),
              value: answer.verified
                ? `${say(answer.text)} · ${copy.verified}`
                : say(answer.text),
            }))
          : []),
        { label: copy.portal, value: request.portal },
        { label: copy.as, value: request.did, mono: true },
      ];
      return (
        <ConfirmSheet
          {...common}
          error={error ?? failed}
          icon={<CredentialIcon />}
          title={copy[`${purpose}Title`].split("{issuer}").join(applying.issuer)}
          lead={copy[`${purpose}Lead`].split("{issuer}").join(applying.issuer)}
          details={rows}
          note={
            nothing
              ? copy.nothingHeld
              : presenting && !applying.complete
                ? copy.someMissing
                : copy.note.replace("{portal}", request.portal)
          }
          confirmDisabled={nothing}
          confirmLabel={copy[purpose]}
          onConfirm={() =>
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await registryAnswer(
                  url,
                  presenting ? choiceToSend(applying.presenting, choice) : undefined,
                );
                onClose();
              } catch (failure) {
                setError(copy.errors[registryErrorCode(failure)]);
                setBusy(false);
              }
            })()
          }
        >
          {presenting ? picker(applying.presenting) : null}
          {/* Issuers' notices are off since the vault went (no server-held
              key sends them), so nothing here will say how it went: the
              catalog is where the holder looks and collects. */}
          {purpose === "submit" ? <p className="card__note">{copy.submitCollect}</p> : null}
        </ConfirmSheet>
      );
    }
    const linking = request?.purpose === "link";
    const signing = request?.signing ?? null;
    const until = signing?.validUntil
      ? new Date(signing.validUntil).toLocaleDateString(locale)
      : null;
    const rows: ConfirmDetail[] = !request
      ? []
      : signing?.kind === "credential"
        ? [
            { label: copy.issuer, value: signing.identity },
            ...(signing.credentialType
              ? [{ label: copy.credential, value: signing.credentialType, mono: true }]
              : []),
            ...signing.claims.map((claim) => ({ label: claim.name, value: claim.value })),
            ...(signing.holder ? [{ label: copy.holder, value: signing.holder, mono: true }] : []),
            ...(until ? [{ label: copy.validUntil, value: until }] : []),
            { label: copy.as, value: request.did, mono: true },
          ]
      : signing?.kind === "status_list"
        ? [
            { label: copy.issuer, value: signing.identity },
            ...(signing.change
              ? [
                  { label: copy.credentialStatus, value: copy.statuses[signing.change.status] },
                  ...(signing.change.holder
                    ? [{ label: copy.holder, value: signing.change.holder, mono: true }]
                    : []),
                  { label: copy.entry, value: String(signing.change.index) },
                ]
              : []),
            { label: copy.identity, value: signing.did ?? "", mono: true },
            { label: copy.as, value: request.did, mono: true },
          ]
        : signing
          ? [
              { label: copy.portal, value: request.portal },
              { label: copy.answerTo, value: request.answerTo },
              ...(signing.tenant ? [{ label: copy.tenant, value: signing.tenant }] : []),
              { label: copy.identity, value: signing.did ?? copy.newDid, mono: signing.did !== null },
              { label: copy.version, value: String(signing.version ?? "") },
              { label: copy.as, value: request.did, mono: true },
            ]
          : [
              { label: copy.portal, value: request.portal },
              { label: copy.answerTo, value: request.answerTo },
              { label: copy.as, value: request.did, mono: true },
            ];
    const title = signing
      ? (signing.kind === "credential"
          ? copy.issueTitle
          : signing.kind === "status_list"
            ? copy.statusTitle
            : copy.signTitle
        ).replace(
          "{identity}",
          signing.identity,
        )
      : (linking ? copy.linkTitle : copy.signInTitle).replace(
          "{name}",
          request?.name ?? copy.aRegistry,
        );
    const lead = signing
      ? signing.kind === "credential"
        ? copy.issueLead
        : signing.kind === "status_list"
        ? (signing.change ? copy.statusChangeLead : copy.statusLead).replace(
            /\{identity\}/g,
            signing.identity,
          )
        : signing.did
          ? copy.signLead
          : copy.signFirstLead
      : linking
        ? copy.linkLead
        : copy.signInLead;
    const notSigner = signing !== null && !signing.signer;
    return (
      <ConfirmSheet
        {...common}
        error={error ?? failed}
        icon={<CredentialIcon />}
        title={title}
        lead={lead}
        details={rows}
        // Somebody could show you their own page's code: accept only what you started.
        note={
          notSigner
            ? copy.notSigner
            : request
              ? copy.note.replace("{portal}", request.portal)
              : null
        }
        confirmDisabled={request === null || notSigner}
        confirmLabel={
          signing?.kind === "credential"
            ? copy.issue
            : signing?.kind === "status_list"
            ? copy.statusSign
            : signing
                ? copy.sign
                : linking
                  ? copy.link
                  : copy.signIn
        }
        onConfirm={() =>
          void (async () => {
            setBusy(true);
            setError(null);
            try {
              await registryAnswer(url);
              onClose();
            } catch (failure) {
              setError(copy.errors[registryErrorCode(failure)]);
              setBusy(false);
            }
          })()
        }
      />
    );
  }

  if (details.kind === "contact") {
    const existing = details.contact;
    const rows: ConfirmDetail[] = [
      { label: t.confirm.contact.fingerprint, value: details.fingerprint, mono: true },
      existing
        ? { label: t.confirm.contact.existing, value: existing.name }
        : { label: t.confirm.contact.reach, value: t.confirm.contact.reachValue },
    ];
    return (
      <ConfirmSheet
        {...common}
        icon={<ProfileIcon />}
        title={t.confirm.contact.title}
        lead={t.confirm.contact.lead}
        details={rows}
        note={details.own ? t.confirm.contact.own : null}
        confirmDisabled={details.own}
        confirmLabel={existing ? t.confirm.contact.open : t.confirm.accept}
        onConfirm={() =>
          existing
            ? onConversation(existing.id)
            : void act(async () => {
                const opened = await acceptInvitation(url);
                onConversation(opened.id);
              })
        }
      />
    );
  }

  const same = details.current === details.mediator;
  // A new mediator would leave every relationship's DIDs routed through the
  // old one, which the wallet would no longer collect from.
  const blocked = !same && details.current !== null && details.contacts > 0;
  const rows: ConfirmDetail[] = [
    { label: t.confirm.mediator.mediator, value: mediatorName(details.mediator) },
    {
      label: t.confirm.mediator.current,
      value: details.current ? mediatorName(details.current) : t.confirm.mediator.none,
    },
  ];
  return (
    <ConfirmSheet
      {...common}
      icon={<ServerIcon />}
      title={t.confirm.mediator.title}
      lead={t.confirm.mediator.lead}
      details={rows}
      note={
        same
          ? t.confirm.mediator.same
          : blocked
            ? plural(t.confirm.mediator.blocked, details.contacts, locale)
            : null
      }
      confirmDisabled={same || blocked}
      confirmLabel={t.confirm.accept}
      onConfirm={() =>
        void act(async () => {
          await connectMediator(url);
          onMediatorConnected();
        })
      }
    />
  );
}
