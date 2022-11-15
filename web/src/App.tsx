import React from "react";
import AppLayout from "@cloudscape-design/components/app-layout";
import SideNavigation from "@cloudscape-design/components/side-navigation";

import { Outlet } from "react-router-dom";

import "@cloudscape-design/global-styles/index.css";

import "./App.css";

function Navigation() {
  return (
    <SideNavigation
      header={{
        href: "#",
        logo: {
          alt: "LXA TAC",
          src: "/logo.svg",
        },
      }}
      items={[
        {
          type: "section",
          text: "Dashboards",
          items: [
            {
              type: "link",
              text: "Device Under Test",
              href: "#/dashboard/dut",
            },
            { type: "link", text: "LXA TAC System", href: "#/dashboard/tac" },
          ],
        },
        {
          type: "section",
          text: "Settings",
          items: [
            { type: "link", text: "Labgrid", href: "#/settings/labgrid" },
          ],
        },
      ]}
    />
  );
}

export default function App() {
  return (
    <AppLayout
      navigation={<Navigation />}
      content={<Outlet />}
      toolsHide={true}
    />
  );
}
