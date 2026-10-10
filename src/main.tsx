import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { initReliability } from "./lib/reliabilityInit";

initReliability();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
