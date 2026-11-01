/**
 * The Almena mark as a drawing — three nodes and the links between them — in
 * `currentColor`, for where the app icon's picture would not take a colour:
 * the watermark on a credential's card.
 */
export function BrandMark({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="136 136 752 752" aria-hidden="true" focusable="false">
      <g stroke="currentColor" strokeWidth="24" strokeLinecap="round">
        <line x1="512" y1="237" x2="237" y2="785" />
        <line x1="512" y1="237" x2="785" y2="785" />
        <line x1="237" y1="785" x2="646" y2="507" />
      </g>
      <g fill="currentColor">
        <circle cx="512" cy="237" r="94" />
        <circle cx="237" cy="785" r="94" />
        <circle cx="785" cy="785" r="94" />
      </g>
    </svg>
  );
}
