import { useEffect, useState, type JSX, type RefObject } from "react";
import { ArrowUp } from "lucide-react";
import { useTranslation } from "../i18n";

const SHOW_AFTER_SCROLL_PX = 600;

/**
 * Floating "scroll to top" button shown once the given scroll container has
 * been scrolled down far enough. Rendered fixed to the viewport corner;
 * the container must be the element that actually scrolls (the page root).
 */
export function ScrollToTopFab({
  containerRef,
}: {
  containerRef: RefObject<HTMLElement | null>;
}): JSX.Element {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return undefined;
    const onScroll = (): void => {
      setVisible(container.scrollTop > SHOW_AFTER_SCROLL_PX);
    };
    onScroll();
    container.addEventListener("scroll", onScroll, { passive: true });
    return () => container.removeEventListener("scroll", onScroll);
  }, [containerRef]);

  const scrollToTop = (): void => {
    containerRef.current?.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <button
      type="button"
      className={`scroll-to-top-fab${visible ? " visible" : ""}`}
      onClick={scrollToTop}
      aria-label={t("app.actions.scrollToTop")}
      title={t("app.actions.scrollToTop")}
      tabIndex={visible ? 0 : -1}
    >
      <ArrowUp size={16} />
    </button>
  );
}
