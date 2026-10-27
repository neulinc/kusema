// Kusema has no mascot; this draws the app-icon mark instead, so Mascot.tsx keeps working.
const HandyMascot = ({ className }: { className?: string }) => (
  <svg
    className={className}
    viewBox="0 0 64 64"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <path
      d="M20 8H44A14 14 0 0 1 58 22V34A14 14 0 0 1 44 48H27L13.5 57.5L15.5 47.26A14 14 0 0 1 6 34V22A14 14 0 0 1 20 8Z"
      fill="var(--color-logo-primary)"
      stroke="var(--color-wordmark-stroke)"
      strokeWidth={2.5}
      strokeLinejoin="round"
    />
    <path
      d="M22.0 22.0H22.000000000000004A3.2 3.2 0 0 1 25.200000000000003 25.2V30.8A3.2 3.2 0 0 1 22.000000000000004 34.0H22.0A3.2 3.2 0 0 1 18.8 30.8V25.2A3.2 3.2 0 0 1 22.0 22.0Z"
      fill="var(--color-wordmark-stroke)"
    />
    <path
      d="M32.0 17.5H32.0A3.2 3.2 0 0 1 35.2 20.7V35.3A3.2 3.2 0 0 1 32.0 38.5H32.0A3.2 3.2 0 0 1 28.8 35.3V20.7A3.2 3.2 0 0 1 32.0 17.5Z"
      fill="#ffb547"
    />
    <path
      d="M42.0 20.5H41.99999999999999A3.2 3.2 0 0 1 45.199999999999996 23.7V32.3A3.2 3.2 0 0 1 41.99999999999999 35.5H42.0A3.2 3.2 0 0 1 38.8 32.3V23.7A3.2 3.2 0 0 1 42.0 20.5Z"
      fill="var(--color-wordmark-stroke)"
    />
  </svg>
);

export default HandyMascot;
