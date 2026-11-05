import React from "react";
import { Undo } from "iconoir-react";

interface ResetIconProps {
  width?: number;
  height?: number;
  color?: string;
  className?: string;
}

/** Reset to the default: Iconoir's Undo. */
const ResetIcon: React.FC<ResetIconProps> = ({
  width = 18,
  height = 18,
  className = "",
}) => (
  <Undo
    width={width}
    height={height}
    strokeWidth={1.5}
    aria-hidden="true"
    className={className}
  />
);

export default ResetIcon;
