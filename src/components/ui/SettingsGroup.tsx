import React from "react";

interface SettingsGroupProps {
  title?: string;
  description?: string;
  children: React.ReactNode;
}

export const SettingsGroup: React.FC<SettingsGroupProps> = ({
  title,
  description,
  children,
}) => {
  return (
    <div className="space-y-2">
      {title && (
        <div className="px-4">
          <h2 className="text-xs font-medium uppercase tracking-wider text-ink-3">
            {title}
          </h2>
          {description && (
            <p className="mt-1 text-[13px]/5 text-ink-3">{description}</p>
          )}
        </div>
      )}
      <div className="overflow-visible rounded-xl border border-line bg-surface">
        <div className="divide-y divide-line">{children}</div>
      </div>
    </div>
  );
};
