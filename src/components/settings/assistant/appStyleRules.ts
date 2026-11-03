/** Styles per app: editing rules, kept plain so they can be checked alone. */

/** "Outlook", "OUTLOOK.EXE" and "C:\…\outlook.exe" are the same program. */
export const normalizeApp = (name: string): string => {
  const file = name.trim().split(/[\\/]/).pop()?.trim().toLowerCase() ?? "";
  if (!file) return "";
  return file.includes(".") ? file : `${file}.exe`;
};

/** `apps` with `name` added, or the same array if there's nothing new. */
export const addApp = (apps: string[], name: string): string[] => {
  const app = normalizeApp(name);
  if (!app || apps.includes(app)) return apps;
  return [...apps, app];
};

export const removeApp = (apps: string[], app: string): string[] =>
  apps.filter((a) => a !== app);

/** An id no style has yet. */
export const newStyleId = (ids: string[]): string => {
  let n = 1;
  while (ids.includes(`custom-${n}`)) n++;
  return `custom-${n}`;
};
