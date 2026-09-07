type BrandSpinnerProps = {
  /** What is being waited for, read out to anybody who cannot see the mark. */
  label: string;
};

/**
 * The wait, wearing the brand: the mark itself breathing rather than a generic
 * ring. Under `prefers-reduced-motion` it simply sits there — see the
 * stylesheet.
 *
 * **It brings its own screen.** Every wait in the wallet was the same three
 * lines of wrapper repeated, which is three chances for one of them to end up
 * centred differently from the others. There is one wait now, in one place on
 * the screen, however it was reached.
 */
export function BrandSpinner({ label }: BrandSpinnerProps) {
  return (
    <div className="screen screen--waiting">
      <div className="brand-spinner" role="status" aria-live="polite">
        <img className="brand-spinner__mark" src="/brand/app-icon.png" alt="" />
        <p className="brand-spinner__label">{label}</p>
      </div>
    </div>
  );
}
