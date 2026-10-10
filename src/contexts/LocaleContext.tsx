import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  useRef,
  type ReactNode,
} from "react";
import { useSettings } from "@/hooks/useSettings";
import { syncRuntimeSettings } from "@/lib/tauri";
import { LOCALE_CACHE_KEY, readCachedLocale, resolveLocale, type Locale } from "@/lib/locale";
import { translate, translateText, setMessageLocale, type MessageKey } from "@/lib/messages";

export const LocaleContext = createContext<Locale>("en");

export function LocaleProvider({ children }: { children: ReactNode }) {
  const { settings, isFetched, isError } = useSettings();
  const [cached] = useState(() => readCachedLocale(localStorage));
  const [languages, setLanguages] = useState(() => navigator.languages);
  const savedPreference = useRef(cached);
  if (isFetched && !isError) savedPreference.current = settings.appearance.locale ?? "system";
  const preference = savedPreference.current;
  const locale = resolveLocale(preference, languages);

  useEffect(() => {
    const changed = () => setLanguages([...navigator.languages]);
    window.addEventListener("languagechange", changed);
    return () => window.removeEventListener("languagechange", changed);
  }, []);

  useEffect(() => {
    setMessageLocale(locale);
    document.documentElement.lang = locale;
    document.title = locale === "zh-CN" ? "Moor - MCP 管理器" : "Moor - MCP Manager";
    try {
      localStorage.setItem(LOCALE_CACHE_KEY, preference);
    } catch {
      /* 隐私模式下仍允许切换语言。 */
    }
    void syncRuntimeSettings(locale).catch((error) =>
      console.warn("Unable to sync tray locale", error),
    );
  }, [locale, preference]);

  return <LocaleContext.Provider value={locale}>{children}</LocaleContext.Provider>;
}

export function useTranslation() {
  const locale = useContext(LocaleContext);
  return useMemo(
    () => ({
      locale,
      t: (key: MessageKey, params?: Record<string, string | number>) =>
        translate(locale, key, params),
      text: (value: string) => translateText(locale, value),
    }),
    [locale],
  );
}
