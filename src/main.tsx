import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { I18nProvider } from "./i18n";
// The typefaces, bundled with the app (it works offline): Chakra Petch for the
// brand and the headings, Inter for the interface, JetBrains Mono for DIDs and
// codes. global.css names them in --font-brand, --font-sans and --font-mono.
import "@fontsource/chakra-petch/500.css";
import "@fontsource/chakra-petch/600.css";
import "@fontsource/chakra-petch/700.css";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./styles/global.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <I18nProvider>
      <App />
    </I18nProvider>
  </React.StrictMode>,
);
