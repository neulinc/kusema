import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

export const Input: React.FC<InputProps> = ({
  className = "",
  variant = "default",
  disabled,
  ...props
}) => {
  const baseClasses =
    "px-2 py-1 text-sm bg-surface border border-line-strong rounded-lg text-start transition-colors duration-150 placeholder:text-ink-3";

  const interactiveClasses = disabled
    ? "opacity-60 cursor-not-allowed border-line"
    : "hover:border-ink-2 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent focus:border-accent";

  const variantClasses = {
    default: "px-3 py-2",
    compact: "px-2 py-1",
  } as const;

  return (
    <input
      className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
      disabled={disabled}
      {...props}
    />
  );
};
