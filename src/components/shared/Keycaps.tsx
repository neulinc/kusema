import React from "react";
import {
  bindingKeys,
  isOnlyModifiers,
  keycapLabel,
  keycapLabels,
  type OSType,
} from "@/lib/utils/keyboard";

interface KeycapsProps {
  /** The binding to draw, e.g. "option+space". */
  binding: string;
  /** Kusema: draw these keys instead of the binding's (the Kusema key). */
  labels?: string[];
  osType: OSType;
  /** sm: standalone; screen: Demo's drawn screen; line: inline in body text. */
  size?: "sm" | "screen" | "line";
  /** Draw every key as held down. */
  pressed?: boolean;
  /** A combination still being captured: dashed, no drop. */
  partial?: boolean;
  /** Raise the keys on hover/focus of a parent `group/keycaps` control. */
  lift?: boolean;
  className?: string;
}

const SIZES = {
  sm: "h-[28px] min-w-[28px] px-[9px] rounded-[6px] text-[13px]/5",
  screen: "h-[24px] min-w-[24px] px-[7px] rounded-[6px] text-[11px]",
  line: "h-[22px] min-w-[22px] px-[7px] rounded-[5px] text-[12px]",
};

/** A shortcut binding drawn as physical keycaps. */
export const Keycaps: React.FC<KeycapsProps> = ({
  binding,
  labels: given,
  osType,
  size = "sm",
  pressed = false,
  partial = false,
  lift = false,
  className = "",
}) => {
  const labels = given ?? keycapLabels(binding, osType);
  // Kusema: a quiet chip, not a drawn key: a subtle surface and a hairline
  // edge, with the faintest drop.
  const state = pressed
    ? "translate-y-[1px] shadow-none bg-selected text-text border-line-strong"
    : partial
      ? "shadow-none border-dashed bg-surface text-text border-line-strong"
      : `shadow-[0_1px_0_0_var(--color-line-strong)] bg-hover text-text border-line ${
          lift
            ? "group-hover/keycaps:-translate-y-[1px] group-hover/keycaps:shadow-[0_2px_0_0_var(--color-line-strong)] group-focus-visible/keycaps:-translate-y-[1px] group-focus-visible/keycaps:shadow-[0_2px_0_0_var(--color-line-strong)]"
            : ""
        }`;

  return (
    <span
      className={`inline-flex items-center gap-[4px] align-middle ${className}`}
      role="img"
      aria-label={given ? given.join(" + ") : keycapLabel(binding, osType)}
    >
      {labels.map((label, i) => (
        <span
          key={i}
          aria-hidden="true"
          className={`relative inline-flex items-center justify-center border font-ui font-medium tabular-nums leading-none select-none transition-[transform,box-shadow,background-color] duration-[70ms] ease-out motion-reduce:transition-none ${SIZES[size]} ${state}`}
        >
          {label}
        </span>
      ))}
    </span>
  );
};

const SLOT_SIZES = {
  sm: "h-[28px] min-w-[88px] px-[9px] rounded-[6px] text-[13px]/5",
  line: "h-[22px] min-w-[64px] px-[8px] rounded-[5px] text-[12px]",
};

/** Placeholder key shown while a shortcut is being captured. */
export const KeySlot: React.FC<
  React.HTMLAttributes<HTMLSpanElement> & { size?: "sm" | "line" }
> = ({ size = "sm", className = "", children, ...rest }) => (
  <span
    aria-hidden={children ? undefined : true}
    {...rest}
    className={`inline-flex items-center justify-center whitespace-nowrap border border-dashed border-line-strong font-ui font-medium leading-none text-ink-2 animate-key-slot motion-reduce:animate-none ${SLOT_SIZES[size]} ${className}`}
  >
    {children}
  </span>
);

/** The instruction while capturing: modifiers alone still need a key. */
export const captureHintKey = (partial: string) =>
  isOnlyModifiers(bindingKeys(partial))
    ? "onboarding.demo.addKey"
    : "onboarding.demo.pressKeys";

/** While capturing: the keys pressed so far, dashed, or an empty slot before any. */
export const CapturedKeys: React.FC<{
  partial: string;
  osType: OSType;
  size?: "sm" | "line";
  /** On the keys and the slot alike. */
  className?: string;
  keysClassName?: string;
  slotClassName?: string;
  /** Words in the empty slot, announced. */
  slotLabel?: string;
}> = ({
  partial,
  osType,
  size = "sm",
  className = "",
  keysClassName = "",
  slotClassName = "",
  slotLabel,
}) =>
  bindingKeys(partial).length ? (
    <Keycaps
      binding={partial}
      osType={osType}
      size={size}
      partial
      className={`${className} ${keysClassName}`}
    />
  ) : (
    <KeySlot
      size={size}
      role={slotLabel ? "status" : undefined}
      className={`${className} ${slotClassName}`}
    >
      {slotLabel}
    </KeySlot>
  );
