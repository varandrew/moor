import { LocaleContext } from "@/contexts/LocaleContext";
import { translate, type MessageKey } from "@/lib/messages";
import { Component, type ErrorInfo, type ReactNode, type ContextType } from "react";
import { Link } from "react-router-dom";
import { Button, buttonVariants } from "@/components/ui/button";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  static contextType = LocaleContext;
  declare context: ContextType<typeof LocaleContext>;
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("ErrorBoundary caught:", error, errorInfo);
  }

  private resetError = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      const t = (key: MessageKey) => translate(this.context ?? "en", key);
      return (
        <div className="flex flex-col items-center justify-center min-h-[50vh] gap-4 p-8">
          <h2 className="font-headline text-xl text-[var(--fg-80)]">{t("Something went wrong")}</h2>
          <p className="text-sm text-[var(--fg-50)] max-w-md text-center">
            {t("Moor hit an unexpected UI error. Try reloading this view.")}
          </p>
          <div className="flex flex-wrap items-center justify-center gap-2">
            <Button onClick={this.resetError} variant="outline">
              {t("Try again")}
            </Button>
            <Link className={buttonVariants({ variant: "ghost" })} onClick={this.resetError} to="/">
              {t("Back to home")}
            </Link>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
