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
            items: [{ type: "link", text: "REST API", href: "#/docs/api" }],
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
  const [runningVersion, setRunningVersion] = useState<string | undefined>();
  const hostname = useMqttSubscription("/v1/tac/network/hostname");
  const tacd_version = useMqttSubscription<string>("/v1/tac/info/tacd/version");

  useEffect(() => {
    document.title =
      hostname === undefined
        ? "LXA TAC (connecting …)"
        : `LXA TAC (${hostname})`;
  }, [hostname]);

  useEffect(() => {
    if (tacd_version !== undefined) {
      if (runningVersion !== undefined && runningVersion !== tacd_version) {
        // We have seen a previous version but then it changed.
        // This can happen if someone installed a new bundle and clicked reboot.
        // Make sure to load the new web interface in that case.
        window.location.reload();
      }

      setRunningVersion(tacd_version);
    }
  }, [runningVersion, tacd_version]);

  return (
    <AppLayout
      navigation={<Navigation />}
      content={<Outlet />}
      toolsHide={true}
    />
  );
}
