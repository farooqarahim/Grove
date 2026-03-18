import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { EventBridge } from "@/lib/EventBridge";
import { queryClient } from "@/lib/queryClient";
import "./index.css";

// Disable right-click context menu in production builds.
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => e.preventDefault());
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <EventBridge />
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
