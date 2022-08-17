import Box from "@cloudscape-design/components/box";
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
            <Box variant="awsui-key-label">Reboot</Box>
            <MqttButton iconName="refresh" topic="/v1/tac/reboot" send={true}>
              Reboot
            </MqttButton>
          </Box>
        </ColumnLayout>
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
              topic="/v1/tac/network/uplink"
              format={(obj: LinkStatus) => {
                return obj.carrier ? `${obj.speed} MBit/s` : "Down";
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">DUT Link Status</Box>
            <MqttBox
              topic="/v1/tac/network/dut"
              format={(obj: LinkStatus) => {
                return obj.carrier ? `${obj.speed} MBit/s` : "Down";
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">IP Adress</Box>
            <MqttBox
              topic="/v1/tac/network/tac-bridge"
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
