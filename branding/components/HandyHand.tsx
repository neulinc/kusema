// Kusema mark (speech bubble with voice bars). Replaces Handy's hand icon;
// name and props kept so callers don't change.
const HandyHand = ({
  width,
  height,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
}) => (
  <svg
    width={width || 48}
    height={height || 48}
    viewBox="0 0 64 64"
    className="text-text"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path
      d="M20 8H44A14 14 0 0 1 58 22V34A14 14 0 0 1 44 48H27L13.5 57.5L15.5 47.26A14 14 0 0 1 6 34V22A14 14 0 0 1 20 8Z"
      fill="none"
      stroke="currentColor"
      strokeWidth={5}
      strokeLinejoin="round"
    />
    <path
      d="M22.0 22.0H22.0A2.75 2.75 0 0 1 24.75 24.75V31.25A2.75 2.75 0 0 1 22.0 34.0H22.0A2.75 2.75 0 0 1 19.25 31.25V24.75A2.75 2.75 0 0 1 22.0 22.0ZM32.0 17.5H32.0A2.75 2.75 0 0 1 34.75 20.25V35.75A2.75 2.75 0 0 1 32.0 38.5H32.0A2.75 2.75 0 0 1 29.25 35.75V20.25A2.75 2.75 0 0 1 32.0 17.5ZM42.0 20.5H42.0A2.75 2.75 0 0 1 44.75 23.25V32.75A2.75 2.75 0 0 1 42.0 35.5H42.0A2.75 2.75 0 0 1 39.25 32.75V23.25A2.75 2.75 0 0 1 42.0 20.5Z"
      fill="currentColor"
    />
  </svg>
);

export default HandyHand;
