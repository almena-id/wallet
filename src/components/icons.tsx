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

/** A person: the profile, which is the identity this wallet holds. */
export function ProfileIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <circle cx="12" cy="8.5" r="3.6" />
      <path d="M5 19.5c.9-3.3 3.7-5.2 7-5.2s6.1 1.9 7 5.2" />
    </svg>
  );
}

/** A server: a mediator, which holds messages for the wallet. */
export function ServerIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <rect x="4" y="4.5" width="16" height="6" rx="1.5" />
      <rect x="4" y="13.5" width="16" height="6" rx="1.5" />
      <path d="M7.5 7.5h.01M7.5 16.5h.01" />
    </svg>
  );
}

/** Power: the wallet starting with the computer. */
export function PowerIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M12 4v7" />
      <path d="M7.1 6.9a7 7 0 1 0 9.8 0" />
    </svg>
  );
}

export function CameraIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M4 8.5a1.5 1.5 0 0 1 1.5-1.5h2.2l1.5-2h5.6l1.5 2h2.2A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z" />
      <circle cx="12" cy="12.8" r="3.3" />
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

/** A handset: a voice call. */
export function PhoneIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M6.6 4h2.6l1.4 3.8-1.9 1.3a11 11 0 0 0 5.2 5.2l1.3-1.9 3.8 1.4v2.6A1.6 1.6 0 0 1 17.4 18 13.6 13.6 0 0 1 5 5.6 1.6 1.6 0 0 1 6.6 4z" />
    </svg>
  );
}

/** The handset put down: ending a call. */
export function HangUpIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M3.4 13.6c4.8-4.1 12.4-4.1 17.2 0l-1.6 2.7-3.5-1.1-.4-2.4a10 10 0 0 0-6.2 0l-.4 2.4-3.5 1.1z" />
    </svg>
  );
}

/** A camera: a video call. */
export function VideoIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <rect x="3.5" y="6.5" width="12" height="11" rx="2" />
      <path d="m15.5 10.5 5-3v9l-5-3" />
    </svg>
  );
}

/** The camera, crossed out: it is off. */
export function VideoOffIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M15.5 12.8v2.7a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h1.3" />
      <path d="M10 6.5h3.5a2 2 0 0 1 2 2v2l5-3v9" />
      <path d="m4 4 16 16" />
    </svg>
  );
}

export function MicIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <rect x="9" y="3.5" width="6" height="11" rx="3" />
      <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v2.5" />
    </svg>
  );
}

/** The microphone, crossed out: it is muted. */
export function MicOffIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M15 10.5V6.5a3 3 0 0 0-5.8-1.1M9 9v2.5a3 3 0 0 0 4.6 2.5" />
      <path d="M5.5 11.5a6.5 6.5 0 0 0 10.4 5.2M18.3 13.4a6.5 6.5 0 0 0 .2-1.9M12 18v2.5" />
      <path d="m4 4 16 16" />
    </svg>
  );
}

/** A credential with nothing to say what kind: a rosette with a tick (lucide's badge-check). */
export function BadgeCheckIcon({ className }: IconProps) {
  return (
    <svg {...base} className={className}>
      <path d="M3.85 8.62a4 4 0 0 1 4.78-4.77 4 4 0 0 1 6.74 0 4 4 0 0 1 4.78 4.78 4 4 0 0 1 0 6.74 4 4 0 0 1-4.77 4.78 4 4 0 0 1-6.75 0 4 4 0 0 1-4.78-4.77 4 4 0 0 1 0-6.76Z" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  );
}
