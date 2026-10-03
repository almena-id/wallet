import { useSystemBack } from "../back";
import { useTranslations } from "../i18n";
import { ChevronLeftIcon } from "./icons";

/**
 * A screen's way back: the arrow at the top left, and — while it is on
 * screen — Android's back button and gesture too (`useSystemBack`).
 */
export function BackButton({ onBack }: { onBack: () => void }) {
  const t = useTranslations();
  useSystemBack(onBack);

  return (
    <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
      <ChevronLeftIcon />
    </button>
  );
}
