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

import Form from "@cloudscape-design/components/form";
import Header from "@cloudscape-design/components/header";
import Container from "@cloudscape-design/components/container";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Tabs from "@cloudscape-design/components/tabs";

import { JournalView } from "./DashboardJournal";
import { ConfigEditor } from "./ConfigEditor";
import { ServiceActionButtons, ServiceStatusRow } from "./SystemdServices";

export function LabgridService() {
  return (
    <Form actions={<ServiceActionButtons name="labgrid-exporter" />}>
      <SpaceBetween size="m">
        <ServiceStatusRow name="labgrid-exporter" />
        <JournalView
          history_len={20}
          rows={20}
          unit="labgrid-exporter.service"
        />
      </SpaceBetween>
    </Form>
  );
}

export function LabgridConfig() {
  return (
    <Tabs
      tabs={[
        {
          label: "User Config",
          id: "user",
          content: (
            <ConfigEditor path="/v1/labgrid/userconfig" language="yaml" />
          ),
        },
        {
          label: "Environment",
          id: "env",
          content: (
            <ConfigEditor path="/v1/labgrid/environment" language="sh" />
          ),
        },
        {
          label: "System Config",
          id: "system",
          content: (
            <ConfigEditor path="v1/labgrid/configuration" language="yaml" />
          ),
        },
      ]}
    />
  );
}

export default function SettingsLabgrid() {
  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="Configure the labgrid exporter">
        LXA TAC / Labgrid Settings
      </Header>

      <Container
        header={
          <Header
            variant="h2"
            description="Restart the Labgrid exporter service and view its log"
          >
            Labgrid Exporter Status
          </Header>
        }
      >
        <LabgridService />
      </Container>

      <Container
        header={
          <Header variant="h2" description="Edit the labgrid exporter config">
            Config Files
          </Header>
        }
      >
        <LabgridConfig />
      </Container>
    </SpaceBetween>
  );
}
