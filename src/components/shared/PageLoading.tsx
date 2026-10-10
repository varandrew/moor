import { useTranslation } from "@/contexts/LocaleContext";
interface PageLoadingProps {
  message?: string;
}

export function PageLoading({ message = "Loading..." }: PageLoadingProps) {
  const { text } = useTranslation();
  return (
    <div className="py-16 text-center">
      <div className="h-8 w-8 mx-auto rounded-full border-2 border-[var(--fg-10)] border-t-cursor-orange animate-spin mb-4" />
      <p className="font-body text-sm text-[var(--fg-40)]">{text(message)}</p>
    </div>
  );
}
