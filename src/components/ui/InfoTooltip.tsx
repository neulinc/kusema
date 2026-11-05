import React from "react";
import { InfoCircle } from "iconoir-react";
import { Tooltip } from "./Tooltip";

interface InfoTooltipProps {
  title?: React.ReactNode;
  text: React.ReactNode;
  placement?: "above" | "below";
}

/** An info icon that explains something in a tooltip. */
export const InfoTooltip: React.FC<InfoTooltipProps> = ({
  title,
  text,
  placement = "above",
}) => (
  <Tooltip title={title} text={text} placement={placement} showOnPress>
    <button
      type="button"
      aria-label="More information"
      className="grid cursor-help text-ink-3 transition-colors duration-200 hover:text-text"
    >
      <InfoCircle width={16} height={16} aria-hidden="true" />
    </button>
  </Tooltip>
);
