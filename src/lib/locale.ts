import type { LocalePreference } from "@moor/types";

export type Locale = "zh-CN" | "en";
export const LOCALE_CACHE_KEY = "moor-locale";

export function resolveLocale(preference: LocalePreference, languages: readonly string[]): Locale {
  if (preference !== "system") return preference;
  return languages[0]?.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

export function readCachedLocale(storage: Pick<Storage, "getItem">): LocalePreference {
  try {
    const value = storage.getItem(LOCALE_CACHE_KEY);
    return value === "zh-CN" || value === "en" ? value : "system";
  } catch {
    return "system";
  }
}
