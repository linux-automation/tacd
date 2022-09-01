import Box from "@cloudscape-design/components/box";
import Form from "@cloudscape-design/components/form";
import Header from "@cloudscape-design/components/header";
import Container from "@cloudscape-design/components/container";
import SpaceBetween from "@cloudscape-design/components/space-between";
import ColumnLayout from "@cloudscape-design/components/column-layout";

import { MqttBox, MqttToggle, MqttButton } from "./MqttComponents";
import { RaucContainer } from "./TacComponents";

import { useEffect, useState } from "react";

type Measurement = {
  ts: number;
  value: number;
};

type IpList = Array<string>;

type LinkStatus = {
  speed: number;
  carrier: boolean;
};

type Uname = {
  sysname: string;
  nodename: string;
  release: string;
  version: string;
  machine: string;
};

type Bootloader = {
  version: string;
  baseboard_release: string;
  powerboard_release: string;
  baseboard_timestamp: number;
  powerboard_timestamp: number;
};

export default function DashboardTac() {
  const [counter, setCounter] = useState(0);

  useEffect(() => {
    let i = 0;
    let interval = window.setInterval(() => setCounter(i++), 500);

    return () => window.clearInterval(interval);
  }, []);

  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="Observe the LXA TAC system status">
        LXA TAC / System Dashboard
      </Header>

      <Container
        header={
          <Header variant="h2" description="See how your TAC is doing">
            Health
          </Header>
        }
      >
        <ColumnLayout columns={4} variant="text-grid">
          <Box>
            <Box variant="awsui-key-label">Live Display</Box>
            <img
              className="live-display"
              src={"/v1/tac/display/content?c=" + counter}
              alt="Live view"
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">SoC Temperature</Box>
            <MqttBox
              topic="/v1/tac/temperatures/soc"
              format={(msg: Measurement) => {
                return `${msg.value.toFixed(0)}°C`;
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Kernel Version</Box>
            <MqttBox
              topic="/v1/tac/info/uname"
              format={(msg: Uname) => msg.release}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Bootloader Version</Box>
            <MqttBox
              topic="/v1/tac/info/bootloader"
              format={(msg: Bootloader) => msg.version}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Mainboard Release</Box>
            <MqttBox
              topic="/v1/tac/info/bootloader"
              format={(msg: Bootloader) => msg.baseboard_release}
            />
          </Box>

          <Box>
            <Box variant="awsui-key-label">Mainboard Bringup Date</Box>
            <MqttBox
              topic="/v1/tac/info/bootloader"
              format={(msg: Bootloader) => {
                let date = new Date(msg.baseboard_timestamp * 1000);
                return date.toLocaleString();
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Powerboard Release</Box>
            <MqttBox
              topic="/v1/tac/info/bootloader"
              format={(msg: Bootloader) => msg.powerboard_release}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Powerboard Bringup Date</Box>
            <MqttBox
              topic="/v1/tac/info/bootloader"
              format={(msg: Bootloader) => {
                let date = new Date(msg.powerboard_timestamp * 1000);
                return date.toLocaleString();
              }}
            />
          </Box>
        </ColumnLayout>

        <Form
          actions={
            <MqttButton iconName="refresh" topic="/v1/tac/reboot" send={true}>
              Reboot
            </MqttButton>
          }
        />
      </Container>

      <RaucContainer />

      <Container
        header={
          <Header variant="h2" description="Check your online status">
            Network
          </Header>
        }
      >
        <ColumnLayout columns={4} variant="text-grid">
          <Box>
            <Box variant="awsui-key-label">Hostname</Box>
            <MqttBox
              topic="/v1/tac/network/hostname"
              format={(msg: string) => msg}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Uplink Status</Box>
            <MqttBox
              topic="/v1/tac/network/interface/uplink"
              format={(obj: LinkStatus) => {
                return obj.carrier ? `${obj.speed} MBit/s` : "Down";
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">DUT Link Status</Box>
            <MqttBox
              topic="/v1/tac/network/interface/dut"
              format={(obj: LinkStatus) => {
                return obj.carrier ? `${obj.speed} MBit/s` : "Down";
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">IP Adress</Box>
            <MqttBox
              topic="/v1/tac/network/interface/tac-bridge"
              format={(obj: IpList) => {
                return obj.length < 1 ? "-" : obj[0];
              }}
            />
          </Box>
        </ColumnLayout>
      </Container>

      <Container
        header={
          <Header variant="h2" description="Find this TAC and others around it">
            Neighbourhood
          </Header>
        }
      >
        <ColumnLayout columns={4} variant="text-grid">
          <Box>
            <Box variant="awsui-key-label">Locator</Box>
            <MqttToggle topic="/v1/tac/display/locator">Locator</MqttToggle>
          </Box>
        </ColumnLayout>
      </Container>
    </SpaceBetween>
  );
}
