import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "outline"
    | "warning"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  // Kusema: calm and nearly monochrome. The accent is only the focus ring.
  const baseClasses =
    "font-medium rounded-lg border focus:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-1 focus-visible:ring-offset-background transition-[background-color,border-color,opacity] duration-150 motion-reduce:transition-none disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer";

  const variantClasses = {
    // Near-white with dark text in dark mode, near-black in light mode.
    primary:
      "text-on-primary bg-primary border-primary hover:opacity-90 active:opacity-80",
    "primary-soft": "text-text bg-selected border-transparent hover:bg-hover",
    // A subtle surface.
    secondary:
      "text-text bg-hover border-line hover:bg-selected hover:border-line-strong",
    // For screens where secondary's fill reads as disabled (onboarding)
    outline: "text-text bg-transparent border-line-strong hover:bg-hover",
    // Secondary's neutral resting look, but hover uses the semantic
    // --color-warning token (theme.css) — for buttons sitting on warning
    // surfaces like SecureInputWarning
    warning:
      "text-text bg-hover border-line hover:bg-warning-wash hover:border-warning",
    // Destructive: a muted red, never a loud block.
    danger: "text-error bg-error-wash border-error/30 hover:border-error/60",
    "danger-ghost":
      "text-error border-transparent hover:bg-error-wash focus:bg-error-wash",
    ghost: "text-current border-transparent hover:bg-hover",
  };

  const sizeClasses = {
    sm: "px-2.5 py-1 text-xs",
    md: "px-3.5 py-1.5 text-sm",
    lg: "px-5 py-2 text-base",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
