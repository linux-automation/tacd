import React from "react";
import AppLayout from "@cloudscape-design/components/app-layout";
import SideNavigation from "@cloudscape-design/components/side-navigation";

import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";

import "@cloudscape-design/global-styles/index.css";

import "./App.css";
import { useMqttSubscription } from "./mqtt";
import { ApiPickerButton } from "./MqttComponents";

function Navigation() {
  const [activeHref, setActiveHref] = useState("#/");

  useEffect(() => {
    function update_hash() {
      setActiveHref(window.location.hash);
    }

    update_hash();
    window.addEventListener("hashchange", update_hash);

    return () => {
      window.removeEventListener("hashchange", update_hash);
    };
  }, []);

  return (
    <>
      <SideNavigation
        activeHref={activeHref}
        onFollow={(ev) => setActiveHref(ev.detail.href)}
        header={{
          href: "#/",
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
              {
                type: "link",
                text: "Systemd Journal",
                href: "#/dashboard/journal",
              },
            ],
          },
          {
            type: "section",
            text: "Settings",
            items: [
              { type: "link", text: "Labgrid", href: "#/settings/labgrid" },
            ],
          },
          {
            type: "section",
            text: "Documentation",
            items: [{ type: "link", text: "API", href: "#/docs/api" }],
          },
        ]}
      />
      <div className="nav_footer">
        <ApiPickerButton />
      </div>
    </>
  );
}

export default function App() {
  const hostname = useMqttSubscription("/v1/tac/network/hostname");

  useEffect(() => {
    document.title =
      hostname === undefined ? "LXA TAC" : `LXA TAC (${hostname})`;
  }, [hostname]);

  return (
    <AppLayout
      navigation={<Navigation />}
      content={<Outlet />}
      toolsHide={true}
    />
  );
}
