import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import "./browser.css";
import "./advanced.css";
import "./yamledit.css";
import "./folders.css";
import "./data.css";
import "./apppicker.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
