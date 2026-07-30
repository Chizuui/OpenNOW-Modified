import type { JSX } from "react";
import { Minus, Square, X } from "lucide-react";

export function TitleBar(): JSX.Element {
  const handleMinimize = (): void => {
    void window.openNow.minimizeWindow();
  };

  const handleMaximize = (): void => {
    void window.openNow.maximizeWindow();
  };

  const handleClose = (): void => {
    void window.openNow.closeWindow();
  };

  return (
    <header className="custom-title-bar">
      <div className="custom-title-bar__drag-area">
        <span className="custom-title-bar__title">OpenNOW</span>
      </div>
      <div className="custom-title-bar__controls">
        <button
          className="custom-title-bar__button custom-title-bar__button--minimize"
          onClick={handleMinimize}
          title="Minimize"
        >
          <Minus size={14} />
        </button>
        <button
          className="custom-title-bar__button custom-title-bar__button--maximize"
          onClick={handleMaximize}
          title="Maximize"
        >
          <Square size={12} />
        </button>
        <button
          className="custom-title-bar__button custom-title-bar__button--close"
          onClick={handleClose}
          title="Close"
        >
          <X size={14} />
        </button>
      </div>
    </header>
  );
}
