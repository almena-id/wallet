/**
 * Inline icons, so the interface carries no icon font and no extra request.
 * They inherit `currentColor` and the surrounding font size.
 */

type IconProps = { className?: string };

const base = {
  width: 24,
  height: 24,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.7,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
  focusable: false,
};

export function HomeIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M4 10.5 12 4l8 6.5" />
      <path d="M6 9.8V19a1 1 0 0 0 1 1h3.5v-4.6h3V20H17a1 1 0 0 0 1-1V9.8" />
    </svg>
  );
}

export function QrIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M4 8.5V5a1 1 0 0 1 1-1h3.5" />
      <path d="M15.5 4H19a1 1 0 0 1 1 1v3.5" />
      <path d="M20 15.5V19a1 1 0 0 1-1 1h-3.5" />
      <path d="M8.5 20H5a1 1 0 0 1-1-1v-3.5" />
      <rect x="8" y="8" width="3.2" height="3.2" rx="0.6" />
      <rect x="12.8" y="12.8" width="3.2" height="3.2" rx="0.6" />
      <path d="M12.8 8h3.2v3.2M8 12.8v3.2h3.2" />
    </svg>
  );
}

export function SettingsIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <circle cx="12" cy="12" r="3.1" />
      <path strokeWidth={1.5} d="M 10.43 5.69 L 10.59 3.11 L 13.41 3.11 L 13.57 5.69 L 15.35 6.43 L 17.29 4.72 L 19.28 6.71 L 17.57 8.65 L 18.31 10.43 L 20.89 10.59 L 20.89 13.41 L 18.31 13.57 L 17.57 15.35 L 19.28 17.29 L 17.29 19.28 L 15.35 17.57 L 13.57 18.31 L 13.41 20.89 L 10.59 20.89 L 10.43 18.31 L 8.65 17.57 L 6.71 19.28 L 4.72 17.29 L 6.43 15.35 L 5.69 13.57 L 3.11 13.41 L 3.11 10.59 L 5.69 10.43 L 6.43 8.65 L 4.72 6.71 L 6.71 4.72 L 8.65 6.43 Z" />
    </svg>
  );
}

export function ChevronLeftIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M14.5 5.5 8 12l6.5 6.5" />
    </svg>
  );
}

export function MessagesIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <rect x="3.5" y="6" width="17" height="12" rx="2.5" />
      <path d="m4.5 7.5 7.5 5.5 7.5-5.5" />
    </svg>
  );
}

export function BellIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M18 15.5V11a6 6 0 1 0-12 0v4.5L4.5 18h15L18 15.5Z" />
      <path d="M9.7 18a2.4 2.4 0 0 0 4.6 0" />
    </svg>
  );
}

export function CredentialIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <rect x="3.2" y="5.5" width="17.6" height="13" rx="2.4" />
      <circle cx="8.7" cy="11" r="1.9" />
      <path d="M5.6 15.9c.5-1.5 1.7-2.3 3.1-2.3s2.6.8 3.1 2.3M14.5 10.4h3.9M14.5 13.4h3.9" />
    </svg>
  );
}

export function CheckIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className} strokeWidth={2.4}>
      <path d="M5 12.5 10 17.5 19 7" />
    </svg>
  );
}

export function KeypadIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <circle cx="7" cy="7" r="1.4" />
      <circle cx="12" cy="7" r="1.4" />
      <circle cx="17" cy="7" r="1.4" />
      <circle cx="7" cy="12" r="1.4" />
      <circle cx="12" cy="12" r="1.4" />
      <circle cx="17" cy="12" r="1.4" />
      <circle cx="7" cy="17" r="1.4" />
      <circle cx="12" cy="17" r="1.4" />
      <circle cx="17" cy="17" r="1.4" />
    </svg>
  );
}

/** A face and a finger at once: whichever this device happens to read. */
export function BiometricIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M12 4.2c-3 0-5.4 1.6-5.4 5.2v3.2" />
      <path d="M12 4.2c3 0 5.4 1.6 5.4 5.2v1.4" />
      <path d="M9.2 9.6a2.8 2.8 0 0 1 5.6 0v4.6" />
      <path d="M12 9.8v5.6" />
      <path d="M6.8 16.4c.6 1.4 1.3 2.5 2 3.4" />
      <path d="M17.2 14.4c-.1 2-.5 3.6-1.1 5" />
    </svg>
  );
}

export function CopyIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <rect x="9" y="9" width="11" height="11" rx="2.4" />
      <path d="M15 6.2V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h.2" />
    </svg>
  );
}

/**
 * Deleting the last digit. A drawing and not the `⌫` character it replaces: the
 * other key on that row is a drawing, the two have to read at the same size,
 * and a glyph's ink is whatever the platform's font decides it is — which is
 * not something a size can be matched against.
 */
export function BackspaceIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M10.2 6.2h7.6a1.6 1.6 0 0 1 1.6 1.6v8.4a1.6 1.6 0 0 1-1.6 1.6h-7.6L4.6 12Z" />
      <path d="M12.6 10.2 16.2 13.8M16.2 10.2l-3.6 3.6" />
    </svg>
  );
}

export function SyncIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M19.5 12a7.5 7.5 0 0 1-12.9 5.2" />
      <path d="M4.5 12a7.5 7.5 0 0 1 12.9-5.2" />
      <path d="M17 3.5v3.5h-3.5" />
      <path d="M7 20.5V17h3.5" />
    </svg>
  );
}

export function PlusIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}
