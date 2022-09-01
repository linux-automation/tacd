import React from "react";
import ReactDOM from "react-dom/client";

import { HashRouter, Routes, Route } from "react-router-dom";

import "./index.css";

import ApiDocs from "./ApiDocs";
import App from "./App";
import DashboardDut from "./DashboardDut";
import DashboardJournal from "./DashboardJournal";
import DashboardTac from "./DashboardTac";
import LandingPage from "./LandingPage";
import SettingsLabgrid from "./SettingsLabgrid";

const root = ReactDOM.createRoot(
  document.getElementById("root") as HTMLElement
);
root.render(
  <React.StrictMode>
    <HashRouter>
      <Routes>
        <Route path="/" element={<App />}>
          <Route path="" element={<LandingPage />} />
          <Route path="/dashboard/dut" element={<DashboardDut />} />
          <Route path="/dashboard/journal" element={<DashboardJournal />} />
          <Route path="/dashboard/tac" element={<DashboardTac />} />
          <Route path="/settings/labgrid" element={<SettingsLabgrid />} />
          <Route path="/docs/api" element={<ApiDocs />} />
        </Route>
      </Routes>
    </HashRouter>
  </React.StrictMode>
);
