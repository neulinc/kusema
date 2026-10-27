/** The Kusema tray mark, matching src-tauri/resources/tray_*.png; uses currentColor like a macOS template image. */
type TrayIconState = "idle" | "recording" | "transcribing";

const BUBBLE =
  "M20 8H44A14 14 0 0 1 58 22V34A14 14 0 0 1 44 48H27L13.5 57.5L15.5 47.26A14 14 0 0 1 6 34V22A14 14 0 0 1 20 8Z";

/** Voice bars drawn inside the outlined idle bubble. */
const IDLE_BARS =
  "M22.0 22.0H22.0A2.75 2.75 0 0 1 24.75 24.75V31.25A2.75 2.75 0 0 1 22.0 34.0H22.0A2.75 2.75 0 0 1 19.25 31.25V24.75A2.75 2.75 0 0 1 22.0 22.0ZM32.0 17.5H32.0A2.75 2.75 0 0 1 34.75 20.25V35.75A2.75 2.75 0 0 1 32.0 38.5H32.0A2.75 2.75 0 0 1 29.25 35.75V20.25A2.75 2.75 0 0 1 32.0 17.5ZM42.0 20.5H42.0A2.75 2.75 0 0 1 44.75 23.25V32.75A2.75 2.75 0 0 1 42.0 35.5H42.0A2.75 2.75 0 0 1 39.25 32.75V23.25A2.75 2.75 0 0 1 42.0 20.5Z";

/** Filled bubbles with the bars (recording) or dots (transcribing) cut out. */
const FILLED_PATHS: Record<Exclude<TrayIconState, "idle">, string> = {
  recording:
    "M20 8H44A14 14 0 0 1 58 22V34A14 14 0 0 1 44 48H27L13.5 57.5L15.5 47.26A14 14 0 0 1 6 34V22A14 14 0 0 1 20 8ZM22.0 22.0H22.0A3.0 3.0 0 0 1 25.0 25.0V31.0A3.0 3.0 0 0 1 22.0 34.0H22.0A3.0 3.0 0 0 1 19.0 31.0V25.0A3.0 3.0 0 0 1 22.0 22.0ZM32.0 17.5H32.0A3.0 3.0 0 0 1 35.0 20.5V35.5A3.0 3.0 0 0 1 32.0 38.5H32.0A3.0 3.0 0 0 1 29.0 35.5V20.5A3.0 3.0 0 0 1 32.0 17.5ZM42.0 20.5H42.0A3.0 3.0 0 0 1 45.0 23.5V32.5A3.0 3.0 0 0 1 42.0 35.5H42.0A3.0 3.0 0 0 1 39.0 32.5V23.5A3.0 3.0 0 0 1 42.0 20.5Z",
  transcribing:
    "M20 8H44A14 14 0 0 1 58 22V34A14 14 0 0 1 44 48H27L13.5 57.5L15.5 47.26A14 14 0 0 1 6 34V22A14 14 0 0 1 20 8ZM16.3 28A4.2 4.2 0 1 0 24.7 28A4.2 4.2 0 1 0 16.3 28ZM27.8 28A4.2 4.2 0 1 0 36.2 28A4.2 4.2 0 1 0 27.8 28ZM39.3 28A4.2 4.2 0 1 0 47.7 28A4.2 4.2 0 1 0 39.3 28Z",
};

const TrayIcon = ({
  state,
  size,
}: {
  state: TrayIconState;
  size: number | string;
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 64 64"
    className="fill-current"
    xmlns="http://www.w3.org/2000/svg"
  >
    {state === "idle" ? (
      <>
        <path
          d={BUBBLE}
          fill="none"
          stroke="currentColor"
          strokeWidth={5}
          strokeLinejoin="round"
        />
        <path d={IDLE_BARS} />
      </>
    ) : (
      <path fillRule="evenodd" d={FILLED_PATHS[state]} />
    )}
  </svg>
);

export default TrayIcon;
