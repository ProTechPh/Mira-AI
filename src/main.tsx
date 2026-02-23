import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initI18n } from "./i18n";

function removeBootSplash() {
  const splash = document.getElementById("boot-splash");
  if (splash) {
    splash.remove();
  }
}

async function bootstrap() {
  try {
    const handleBootReady = () => {
      removeBootSplash();
      window.removeEventListener("app:boot-ready", handleBootReady as EventListener);
    };
    window.addEventListener("app:boot-ready", handleBootReady as EventListener);

    await initI18n();
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
  } catch (error) {
    console.error("Bootstrap failed:", error);
    removeBootSplash();
  }
}

void bootstrap();
