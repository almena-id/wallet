import { useStack } from "../nav";
import { AcceptInvitationScreen } from "./AcceptInvitationScreen";
import { ContactScreen } from "./ContactScreen";
import { ConversationScreen } from "./ConversationScreen";
import { InviteScreen } from "./InviteScreen";
import { MessagesScreen } from "./MessagesScreen";
import { NewConversationScreen } from "./NewConversationScreen";

/** Where the Messages tab is: the inbox, or one of the screens it leads to. */
type View =
  | { name: "inbox" }
  | { name: "new" }
  | { name: "invite" }
  | { name: "accept" }
  | { name: "conversation"; id: string }
  | { name: "contact"; id: string };

type MessagesTabProps = {
  /** A conversation to open on, when something outside the tab led to it. */
  initialConversation?: string | null;
  /** Puts an `almena://` link to the person (an issuer's credential to collect). */
  onLink: (url: string) => void;
};

/**
 * The Messages tab and the screens behind it, on a stack (`nav.ts`): each
 * screen's way back returns to whichever screen led to it — a conversation
 * opened from the contacts goes back to them, one opened from the inbox to it.
 */
export function MessagesTab({ initialConversation = null, onLink }: MessagesTabProps) {
  const { top, push, pop } = useStack<View>(() =>
    initialConversation
      ? [{ name: "inbox" }, { name: "conversation", id: initialConversation }]
      : [{ name: "inbox" }],
  );

  switch (top.name) {
    case "new":
      return (
        <NewConversationScreen
          onBack={pop}
          onOpen={(id) => push({ name: "conversation", id })}
        />
      );
    case "invite":
      return <InviteScreen onBack={pop} />;
    case "accept":
      return (
        <AcceptInvitationScreen
          onBack={pop}
          // Back to the list, which syncs and shows the new contact as pending.
          onAccepted={pop}
        />
      );
    case "conversation":
      return (
        <ConversationScreen
          id={top.id}
          onBack={pop}
          onContact={() => push({ name: "contact", id: top.id })}
          onLink={onLink}
        />
      );
    case "contact":
      return <ContactScreen id={top.id} onBack={pop} />;
    default:
      return (
        <MessagesScreen
          onNewConversation={() => push({ name: "new" })}
          onOpen={(id) => push({ name: "conversation", id })}
        />
      );
  }
}
