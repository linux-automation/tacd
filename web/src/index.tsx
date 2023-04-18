// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2022 Pengutronix e.K.
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

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
import Setup from "./Setup";

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
        <Route path="/setup" element={<Setup />} />
      </Routes>
    </HashRouter>
  </React.StrictMode>
);
