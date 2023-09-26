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

import Alert from "@cloudscape-design/components/alert";
import Button from "@cloudscape-design/components/button";
import AppLayout from "@cloudscape-design/components/app-layout";
import SideNavigation from "@cloudscape-design/components/side-navigation";

import { useEffect, useState } from "react";
import { Outlet } from "react-router-dom";

import "@cloudscape-design/global-styles/index.css";

import "./App.css";
import { useMqttSubscription } from "./mqtt";
import { ApiPickerButton, MqttButton } from "./MqttComponents";
import {
  IOBusFaultNotification,
  RebootNotification,
  UpdateNotification,
  ProgressNotification,
  LocatorNotification,
  OverTemperatureNotification,
} from "./TacComponents";

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
          {
            type: "section",
            text: "External Links",
            items: [
              {
                type: "link",
                text: "Files in /srv/www",
                href: `/srv`,
              },
              {
                type: "link",
                text: "LXA IOBus Server",
                href: `http://${window.location.hostname}:8080/`,
              },
              {
                type: "link",
                text: "LXA TAC Manual",
                href: "https://www.linux-automation.com/lxatac-M02/index.html",
              },
            ],
          },
        ]}
      />
      <div className="nav_footer">
        <MqttButton
          iconName="search"
          topic="/v1/tac/display/locator"
          send={true}
        >
          Find this TAC
        </MqttButton>
        <ApiPickerButton />
      </div>
    </>
  );
}

function ConnectionNotification() {
  const hostname = useMqttSubscription("/v1/tac/network/hostname");

  return (
    <Alert
      statusIconAriaLabel="Info"
      visible={hostname === undefined}
      action={
        <Button onClick={(ev) => window.location.reload()}>Reload</Button>
      }
      header="Connection Lost"
    >
      There is currently no connection to the TAC. Wait for the connection to be
      re-established or reload the page.
    </Alert>
  );
}

function Notifications() {
  return (
    <>
      <ConnectionNotification />
      <RebootNotification />
      <OverTemperatureNotification />
      <ProgressNotification />
      <UpdateNotification />
      <LocatorNotification />
      <IOBusFaultNotification />
    </>
  );
}

export default function App() {
  const [runningVersion, setRunningVersion] = useState<string | undefined>();
  const hostname = useMqttSubscription("/v1/tac/network/hostname");
  const setup_mode = useMqttSubscription("/v1/tac/setup_mode");
  const tacd_version = useMqttSubscription<string>("/v1/tac/info/tacd/version");

  useEffect(() => {
    document.title =
      hostname === undefined
        ? "LXA TAC (connecting …)"
        : `LXA TAC (${hostname})`;
  }, [hostname]);

  useEffect(() => {
    // Redirect to the setup wizard if the TAC has not gone through initial
    // setup yet.
    if (setup_mode === true) {
      window.location.replace("/#/setup");
    }
  }, [setup_mode]);

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
      notifications={<Notifications />}
      stickyNotifications={true}
      navigation={<Navigation />}
      content={<Outlet />}
      toolsHide={true}
    />
  );
}
