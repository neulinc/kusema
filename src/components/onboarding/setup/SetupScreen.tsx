import React, { useEffect, useRef } from "react";
import { OnboardingShell } from "../OnboardingShell";

export const FIELD_CLASS =
  "w-full cursor-pointer rounded-lg border border-line-strong bg-background px-[12px] py-[8px] text-body text-text focus:outline-none focus-visible:ring-2 focus-visible:ring-logo-primary";

interface SetupScreenProps {
  progress: { index: number; count: number };
  title: React.ReactNode;
  sub?: React.ReactNode;
  pill?: React.ReactNode;
  onBack?: () => void;
  hint?: React.ReactNode;
  primary?: React.ReactNode;
  /** Off when the screen puts the cursor somewhere itself. */
  focusTitle?: boolean;
  children?: React.ReactNode;
}

/** One setup screen. Grows and scrolls with large text. */
export const SetupScreen: React.FC<SetupScreenProps> = ({
  progress,
  title,
  sub,
  pill,
  onBack,
  hint,
  primary,
  focusTitle = true,
  children,
}) => {
  const heading = useRef<HTMLHeadingElement>(null);
  // A screen reader announces the new screen, and Tab starts from its top.
  useEffect(() => {
    if (focusTitle) heading.current?.focus({ preventScroll: true });
  }, [focusTitle]);
  return (
    <OnboardingShell
      fluid
      progress={progress}
      pill={pill}
      onBack={onBack}
      hint={hint}
      primary={primary}
    >
      <div className="flex flex-col items-center px-[28px] py-[12px] text-center">
        <h1
          ref={heading}
          tabIndex={-1}
          className="m-0 font-display text-title leading-[1.15] font-[650] tracking-[-0.015em] text-balance focus:outline-none"
        >
          {title}
        </h1>
        {sub && <p className="mt-[6px] mb-0 text-ink-2 text-pretty">{sub}</p>}
        <div className="mt-[20px] flex w-full max-w-[560px] flex-col gap-[14px] text-start">
          {children}
        </div>
      </div>
    </OnboardingShell>
  );
};
