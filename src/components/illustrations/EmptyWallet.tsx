/**
 * An open wallet with an empty card slot drawn in dashes over it: Home with
 * nothing held yet. Drawn in the identity colour and its soft tint
 * (`.empty-wallet` in `global.css`), so it follows Appearance and the theme.
 */
export function EmptyWallet({ className }: { className?: string }) {
  return (
    <svg
      className={className ? `empty-wallet ${className}` : "empty-wallet"}
      viewBox="0 0 200 150"
      aria-hidden="true"
      focusable="false"
    >
      {/* The card that is not there yet, standing out of the pocket. */}
      <rect className="empty-wallet__slot" x="46" y="14" width="108" height="68" rx="10" />
      <path className="empty-wallet__line" d="M60 34h22M60 44h34" />
      <circle className="empty-wallet__line" cx="136" cy="32" r="7" />
      {/* The wallet: its back, then its front pocket over the card. */}
      <rect className="empty-wallet__back" x="22" y="46" width="156" height="92" rx="16" />
      <path
        className="empty-wallet__pocket"
        d="M22 74h156v48a16 16 0 0 1-16 16H38a16 16 0 0 1-16-16Z"
      />
      <rect className="empty-wallet__clasp" x="140" y="92" width="38" height="24" rx="8" />
      <circle className="empty-wallet__stud" cx="154" cy="104" r="4" />
      {/* A spark or two: something will arrive here. */}
      <path className="empty-wallet__spark" d="M176 18v10M171 23h10M30 26v6M27 29h6" />
    </svg>
  );
}
