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

import Box from "@cloudscape-design/components/box";
import Header from "@cloudscape-design/components/header";
import Container from "@cloudscape-design/components/container";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Tabs from "@cloudscape-design/components/tabs";

import { ConfigEditor } from "./ConfigEditor";

export default function SettingsSSH() {
  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="Configure the SSH server">
        LXA TAC / SSH Settings
      </Header>

      <Container
        header={
          <Header variant="h2" description="Edit the SSH server config">
            Config Files
          </Header>
        }
      >
        <Tabs
          tabs={[
            {
              label: "Authorized Keys for root",
              id: "user",
              content: (
                <ConfigEditor path="/v1/tac/ssh/authorized_keys" language="text" />
              ),
            },
          ]}
        />
      </Container>
    </SpaceBetween>
  );
}
