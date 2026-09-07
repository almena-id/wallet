import type { ReactNode } from "react";

import { ChevronLeftIcon } from "./icons";

type RowProps = {
  /** The glyph that says which setting this is, before the words do. */
  icon: ReactNode;
  label: string;
  /** The line under it: the value, or the reason there is nothing to change. */
  hint?: string;
};

/**
 * A setting that is on or off, and is flipped where it is read.
 *
 * **The switch is the whole control and the whole answer.** A row that said what
 * a setting does, then said whether it was on, then offered a button to change
 * it, was three things where there is one — and the button had to be labelled
 * with the opposite of the state, which is the sentence people misread.
 *
 * It is a `switch` and not a checkbox: the change takes effect where it is made
 * rather than being submitted, which is what the role means.
 */
export function ToggleRow({
  icon,
  label,
  hint,
  on,
  disabled = false,
  onChange,
}: RowProps & {
  on: boolean;
  /** Shown, said, and refused — for a setting this device cannot offer. */
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-disabled={disabled}
      className="row"
      onClick={() => {
        if (!disabled) {
          onChange(!on);
        }
      }}
    >
      <span className="row__icon">{icon}</span>
      <span className="row__text">
        <span className="row__label">{label}</span>
        {hint ? <span className="row__hint">{hint}</span> : null}
      </span>
      <span className="switch" aria-hidden="true" />
    </button>
  );
}

/**
 * A setting that leads somewhere, in the same shape as one that is flipped.
 *
 * The PIN is not a switch, because it cannot be turned off: it is one of the two
 * things that open the record. So it keeps the row and trades the switch for the
 * arrow that says there is a screen behind it.
 */
export function ChevronRow({
  icon,
  label,
  hint,
  onClick,
}: RowProps & { onClick: () => void }) {
  return (
    <button type="button" className="row" onClick={onClick}>
      <span className="row__icon">{icon}</span>
      <span className="row__text">
        <span className="row__label">{label}</span>
        {hint ? <span className="row__hint">{hint}</span> : null}
      </span>
      <ChevronLeftIcon className="row__chevron" />
    </button>
  );
}
