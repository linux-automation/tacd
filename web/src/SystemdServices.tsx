// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2023 Pengutronix e.K.
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
import ColumnLayout from "@cloudscape-design/components/column-layout";
import SpaceBetween from "@cloudscape-design/components/space-between";

import { MqttBox, MqttButton } from "./MqttComponents";

type ServiceStatus = {
  active_state: string;
  sub_state: string;
  active_enter_ts: number;
  active_exit_ts: number;
};

interface ServiceProps {
  name: string;
}

export function ServiceActionButtons(props: ServiceProps) {
  const path = `/v1/tac/service/${props.name}/action`;

  return (
    <SpaceBetween size="xs" direction="horizontal">
      <MqttButton iconName="status-positive" topic={path} send={"Start"}>
        Start
      </MqttButton>
      <MqttButton iconName="status-stopped" topic={path} send={"Stop"}>
        Stop
      </MqttButton>
      <MqttButton iconName="refresh" topic={path} send={"Restart"}>
        Restart
      </MqttButton>
    </SpaceBetween>
  );
}

export function ServiceStatusRow(props: ServiceProps) {
  const path = `/v1/tac/service/${props.name}/status`;

  return (
    <ColumnLayout columns={3} variant="text-grid">
      <Box>
        <Box variant="awsui-key-label">Service Status</Box>
        <MqttBox
          topic={path}
          format={(state: ServiceStatus) => {
            return `${state.active_state} (${state.sub_state})`;
          }}
        />
      </Box>
      <Box>
        <Box variant="awsui-key-label">Last Started</Box>
        <MqttBox
          topic={path}
          format={(state: ServiceStatus) => {
            if (state.active_enter_ts !== 0) {
              let date = new Date(state.active_enter_ts / 1000);
              return date.toLocaleString();
            } else {
              return "never";
            }
          }}
        />
      </Box>
      <Box>
        <Box variant="awsui-key-label">Last Stopped</Box>
        <MqttBox
          topic={path}
          format={(state: ServiceStatus) => {
            if (state.active_exit_ts !== 0) {
              let date = new Date(state.active_exit_ts / 1000);
              return date.toLocaleString();
            } else {
              return "never";
            }
          }}
        />
      </Box>
    </ColumnLayout>
  );
}
