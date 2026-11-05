import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { NavArrowDown } from "iconoir-react";

export interface DropdownOption {
  value: string;
  label: React.ReactNode;
  description?: string;
  disabled?: boolean;
}

interface DropdownProps {
  options: DropdownOption[];
  className?: string;
  menuClassName?: string;
  selectedValue: string | null;
  onSelect: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  onOpen?: () => void;
  /** Where the menu opens: below the button (default), or above it. */
  placement?: "below" | "above";
  /** Highlight colour for hover and selection. */
  tone?: "brand" | "neutral";
}

export const Dropdown: React.FC<DropdownProps> = ({
  options,
  selectedValue,
  onSelect,
  className = "",
  menuClassName,
  placeholder = "Select an option...",
  disabled = false,
  onOpen,
  placement = "below",
  tone = "brand",
}) => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Both tones are neutral now; `tone` stays for the callers that pass it.
  const hover = tone === "neutral" ? "hover:bg-hover" : "hover:bg-hover";
  const selected = "bg-selected";

  const selectedOption = options.find(
    (option) => option.value === selectedValue,
  );

  const handleSelect = (value: string) => {
    onSelect(value);
    setIsOpen(false);
  };

  const handleToggle = () => {
    if (disabled) return;
    if (!isOpen) onOpen?.();
    setIsOpen(!isOpen);
  };

  return (
    <div className={`relative ${className}`} ref={dropdownRef}>
      <button
        type="button"
        className={`px-2.5 py-1.5 text-sm font-medium bg-surface border border-line-strong rounded-lg min-w-[200px] w-full text-start grid grid-cols-[1fr_auto] gap-2 items-center transition-colors duration-150 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent ${
          disabled ? "opacity-50 cursor-not-allowed" : `${hover} cursor-pointer`
        }`}
        onClick={handleToggle}
        disabled={disabled}
        aria-expanded={isOpen}
      >
        <span className="truncate">{selectedOption?.label || placeholder}</span>
        <NavArrowDown
          width={16}
          height={16}
          aria-hidden="true"
          className={`text-ink-2 transition-transform duration-200 motion-reduce:transition-none ${isOpen ? "rotate-180" : ""}`}
        />
      </button>
      {isOpen && !disabled && (
        <div
          className={`absolute ${placement === "above" ? "bottom-full mb-1" : "top-full mt-1"} bg-surface border border-line rounded-lg shadow-lg z-50 max-h-60 overflow-y-auto py-1 ${
            menuClassName ?? "left-0 right-0"
          }`}
        >
          {options.length === 0 ? (
            <div className="px-2 py-1 text-sm text-ink-3">
              {t("common.noOptionsFound")}
            </div>
          ) : (
            options.map((option) => (
              <button
                key={option.value}
                type="button"
                className={`w-full text-sm text-start ${hover} transition-colors duration-150 ${
                  option.description ? "px-3 py-2" : "px-2 py-1"
                } ${
                  selectedValue === option.value ? selected : ""
                } ${option.disabled ? "opacity-50 cursor-not-allowed" : ""}`}
                onClick={() => handleSelect(option.value)}
                disabled={option.disabled}
              >
                <span
                  className={`block whitespace-normal break-words ${
                    option.description || selectedValue === option.value
                      ? "font-semibold"
                      : ""
                  }`}
                >
                  {option.label}
                </span>
                {option.description && (
                  <span className="mt-0.5 block whitespace-normal text-xs font-normal leading-snug text-ink-3">
                    {option.description}
                  </span>
                )}
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
};
