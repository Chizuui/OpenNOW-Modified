import { X } from "lucide-react";
import { useLayoutEffect, useRef, useState, type JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { ModalSurface } from "./ui/ModalSurface";
import { useTranslation } from "../i18n";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { getStoreOptions } from "../lib/gameCardStores";
import {
  getControllerHeroBackgroundCandidates,
  getPlayerSummary,
} from "../lib/controllerCatalogUi";

export interface GameDetailModalProps {
  open: boolean;
  game: GameInfo | null;
  selectedVariantId?: string;
  onClose: () => void;
  onPlay: (game: GameInfo) => void;
  onSelectVariant: (variantId: string) => void;
}

export function GameDetailModal({
  open,
  game,
  selectedVariantId,
  onClose,
  onPlay,
  onSelectVariant,
}: GameDetailModalProps): JSX.Element {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [isClamped, setIsClamped] = useState(false);
  const descriptionRef = useRef<HTMLParagraphElement | null>(null);
  const gameId = game?.id;

  // Reset expansion + re-measure clamping whenever the shown game changes.
  useLayoutEffect(() => {
    setExpanded(false);
    const el = descriptionRef.current;
    if (!el) {
      setIsClamped(false);
      return;
    }
    setIsClamped(el.scrollHeight - el.clientHeight > 2);
  }, [gameId]);

  if (!game) {
    return (
      <ModalSurface
        open={false}
        onClose={onClose}
        overlayClassName="game-detail-overlay"
        backdropClassName="game-detail-backdrop"
        panelClassName="game-detail-panel"
        motion="large"
      >
        <div />
      </ModalSurface>
    );
  }

  const heroBackgrounds = getControllerHeroBackgroundCandidates(game);
  const heroUrl = heroBackgrounds[0] ?? game.imageUrl;
  const storeOptions = getStoreOptions(game, selectedVariantId);
  const activeStoreOption = storeOptions.find((opt) => opt.isActive) ?? storeOptions[0];
  const playerSummary = getPlayerSummary(game);

  const handlePlay = (): void => {
    onPlay(game);
  };

  const metaRows: string[] = [];
  if (game.developerName) {
    metaRows.push(t("library.developer", { developer: game.developerName }));
  }
  if (game.publisherName) {
    metaRows.push(t("library.publisher", { publisher: game.publisherName }));
  }
  if (playerSummary) {
    metaRows.push(t("library.players", { players: playerSummary }));
  }
  if (game.genres?.length) {
    metaRows.push(t("library.genres", { genres: game.genres.slice(0, 4).join(", ") }));
  }
  if (game.supportedControls?.length) {
    metaRows.push(t("library.controls", { controls: game.supportedControls.slice(0, 4).join(", ") }));
  }
  if (game.nvidiaTech?.length) {
    metaRows.push(t("library.nvidiaTech", { tech: game.nvidiaTech.slice(0, 4).join(", ") }));
  }
  if (game.contentRatings?.length) {
    metaRows.push(t("library.rating", { rating: game.contentRatings.slice(0, 2).join(", ") }));
  }

  const description = game.description || game.longDescription || game.featureLabels?.join(" / ") || t("library.loadingGameDetails");

  return (
    <ModalSurface
      open={open}
      onClose={onClose}
      onConfirm={handlePlay}
      overlayClassName="game-detail-overlay"
      backdropClassName="game-detail-backdrop"
      panelClassName="game-detail-panel"
      motion="large"
      ariaLabel={game.title}
      backdropLabel={t("app.actions.close")}
    >
      <div className="game-detail-hero">
        {heroUrl ? (
          <img className="game-detail-hero-img" src={heroUrl} alt={game.title} />
        ) : (
          <div className="game-detail-hero-img game-detail-hero-img--empty" />
        )}
        <div className="game-detail-hero-scrim" />
        <button
          type="button"
          className="game-detail-close"
          onClick={onClose}
          aria-label={t("app.actions.close")}
        >
          <X size={18} />
        </button>
        <h2 className="game-detail-title">{game.title}</h2>
      </div>

      <div className="game-detail-body">
        <p
          ref={descriptionRef}
          className={`game-detail-description${expanded ? " is-expanded" : ""}`}
        >
          {description}
        </p>
        {(isClamped || expanded) && (
          <button
            type="button"
            className="game-detail-readmore"
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? t("app.actions.showLess") : t("app.actions.readMore")}
          </button>
        )}

        {metaRows.length > 0 && (
          <ul className="game-detail-meta">
            {metaRows.map((row) => (
              <li key={row}>{row}</li>
            ))}
          </ul>
        )}

        {storeOptions.length > 0 && (
          <div className="game-detail-stores">
            <span className="game-detail-stores-label">{t("library.chooseStore")}</span>
            <div className="game-detail-stores-row">
              {storeOptions.map((option) => {
                const StoreIcon = getStoreIconComponent(option.store);
                const className = [
                  "game-detail-store-chip",
                  option.isActive ? "active" : "",
                  option.isOwned ? "owned" : "",
                ].filter(Boolean).join(" ");
                const titleParts = [getStoreDisplayName(option.store)];
                if (option.isOwned) titleParts.push(t("gameCard.owned"));
                return (
                  <button
                    key={option.storeKey}
                    type="button"
                    className={className}
                    title={titleParts.join(" · ")}
                    aria-pressed={option.isActive}
                    onClick={() => onSelectVariant(option.variantId)}
                  >
                    <StoreIcon />
                    <span>{getStoreDisplayName(option.store)}</span>
                  </button>
                );
              })}
            </div>
          </div>
        )}
      </div>

      <div className="game-detail-actions">
        <button type="button" className="game-detail-play" onClick={handlePlay}>
          {activeStoreOption
            ? `${t("app.actions.play")} · ${getStoreDisplayName(activeStoreOption.store)}`
            : t("app.actions.play")}
        </button>
        <button type="button" className="game-detail-cancel" onClick={onClose}>
          {t("app.actions.close")}
        </button>
      </div>
    </ModalSurface>
  );
}
