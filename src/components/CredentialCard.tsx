import type { ReactNode } from "react";

import type { Standing } from "../credentials";
import { BrandMark } from "./BrandMark";

type CredentialCardProps = {
  typeName: string;
  issuerName: string;
  /** What kind of credential it is: a word and its glyph. */
  category: { label: string; icon: ReactNode };
  /** `full` on its own line; `compact` where several share a row. */
  size?: "full" | "compact";
  /** The chip at the bottom right: how it stands (`StatusChip`). */
  status?: ReactNode;
  /**
   * Makes the whole card a button, named by `label` — the card on Home, which
   * opens its detail. Without it the card is a picture with words on it.
   */
  onPress?: () => void;
  label?: string;
};

/**
 * A credential drawn as the card it stands for: ID-1 proportions, the
 * person's identity colour from corner to corner, fine guilloché arcs and the
 * Almena mark behind the words, its kind at the top, its name and issuer at
 * the foot. Every colour is a token (`global.css`), so the card follows the
 * colour chosen in Appearance — dark ink on yellow and cyan included.
 */
export function CredentialCard({
  typeName,
  issuerName,
  category,
  size = "full",
  status,
  onPress,
  label,
}: CredentialCardProps) {
  const className = `credential-card credential-card--${size}`;
  const face = (
    <>
      <Guilloche />
      <BrandMark className="credential-card__mark" />
      <span className="credential-card__top">
        <span className="credential-card__lead">
          <span className="credential-card__icon">{category.icon}</span>
          <ChipGlyph />
        </span>
        <span className="credential-card__category">{category.label}</span>
      </span>
      <span className="credential-card__foot">
        <span className="credential-card__names">
          <span className="credential-card__type">{typeName}</span>
          <span className="credential-card__issuer">{issuerName}</span>
        </span>
        {status}
      </span>
    </>
  );

  return onPress ? (
    <button type="button" className={className} onClick={onPress} aria-label={label}>
      {face}
    </button>
  ) : (
    <div className={className}>{face}</div>
  );
}

/** How a credential stands, as the white pill on its card. */
export function StatusChip({ standing, label }: { standing: Standing; label: string }) {
  return (
    <span className={`status-chip status-chip--${standing}`}>
      <span className="status-chip__dot" aria-hidden="true" />
      {label}
    </span>
  );
}

/** Concentric arcs off the top right corner, the way a printed card is guarded. */
function Guilloche() {
  const rings = [40, 62, 84, 106, 128, 150, 172, 194, 216, 238];
  return (
    <svg
      className="credential-card__pattern"
      viewBox="0 0 317 200"
      preserveAspectRatio="xMaxYMin slice"
      aria-hidden="true"
      focusable="false"
    >
      {rings.map((r, index) => (
        <ellipse
          key={r}
          cx={300 - index * 3}
          cy={-10 + index * 2}
          rx={r * 1.15}
          ry={r}
          fill="none"
          stroke="currentColor"
          strokeWidth="1"
        />
      ))}
    </svg>
  );
}

/** The contact chip of a smart card: a rounded plate with its seams. */
function ChipGlyph() {
  return (
    <svg className="credential-card__chip" viewBox="0 0 34 26" aria-hidden="true" focusable="false">
      <rect x="0.5" y="0.5" width="33" height="25" rx="5" fill="currentColor" />
      <path d="M0.5 9h10M0.5 17h10M23.5 9h10M23.5 17h10M10.5 0.5v25M23.5 0.5v25" stroke="currentColor" strokeWidth="1" fill="none" className="credential-card__chip-seams" />
    </svg>
  );
}
