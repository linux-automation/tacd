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
import Link from "@cloudscape-design/components/link";
import Header from "@cloudscape-design/components/header";
import Container from "@cloudscape-design/components/container";
import Popover from "@cloudscape-design/components/popover";
import SpaceBetween from "@cloudscape-design/components/space-between";
import ColumnLayout from "@cloudscape-design/components/column-layout";
import ExpandableSection from "@cloudscape-design/components/expandable-section";

import { ReactElement, useState } from "react";

import {
  MqttBox,
  MqttToggleConv,
  MqttToggle,
  MqttBarMeter,
  MqttChart,
} from "./MqttComponents";

type IOBusServerStatus = {
  hostname: string;
  started: string;
  can_interface: string;
  can_interface_is_up: boolean;
  lss_state: string;
  can_tx_error: boolean;
};

type IOBusNodes = {
  code: number;
  error_message: string;
  result: Array<string>;
};

type Measurement = {
  ts: number;
  value: number;
};

type UsbDevice = {
  id_product: string;
  id_vendor: string;
  manufacturer: string;
  product: string;
} | null;

interface UnloadingSectionProps {
  children: ReactElement;
  header?: ReactElement | string;
}

function UnloadingSection(props: UnloadingSectionProps) {
  const [active, setActive] = useState(false);

  return (
    <ExpandableSection
      expanded={active}
      header={props.header}
      onChange={(ev) => setActive(ev.detail.expanded)}
    >
      {active ? props.children : <span />}
    </ExpandableSection>
  );
}

export default function DashboardDut() {
  return (
    <SpaceBetween size="m">
      <Header
        variant="h1"
        description="Control the Device Under Test facing aspects of the LXA TAC"
      >
        LXA TAC / Device Under Test Dashboard
      </Header>

      <Container
        header={
          <Header
            variant="h2"
            description="Turn the Device Under Test off or on"
          >
            DUT Power
          </Header>
        }
      >
        <ColumnLayout columns={2} variant="text-grid">
          <Box>
            <Box variant="awsui-key-label">DUT voltage (V)</Box>
            <MqttChart topic="/v1/dut/feedback/voltage" />
          </Box>
          <Box>
            <Box variant="awsui-key-label">DUT current (A)</Box>
            <MqttChart topic="/v1/dut/feedback/current" />
          </Box>
        </ColumnLayout>
        <ColumnLayout columns={4} variant="text-grid">
          <Box>
            <Box variant="awsui-key-label">Power Supply</Box>
            <MqttToggleConv
              topic="/v1/dut/powered"
              to_bool={(status: string) => status === "On"}
              from_bool={(on: boolean) => (on ? "On" : "Off")}
            >
              Power On
            </MqttToggleConv>
          </Box>
          <Box>
            <Box variant="awsui-key-label">Status</Box>
            <MqttBox topic="/v1/dut/powered" format={(msg: string) => msg} />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Voltage</Box>
            <MqttBox
              topic="/v1/dut/feedback/voltage"
              format={(obj: Measurement) => {
                return `${obj.value.toFixed(1)}V / 48.0V`;
              }}
            />
          </Box>
          <Box>
            <Box variant="awsui-key-label">Current</Box>
            <MqttBox
              topic="/v1/dut/feedback/current"
              format={(obj: Measurement) => {
                return `${obj.value.toFixed(2)}A / 5.00A`;
              }}
            />
          </Box>
        </ColumnLayout>
      </Container>

      <Container
        header={
          <Header
            variant="h2"
            description="Set boot modes or reset the DUT via jumper pins"
          >
            Isolated Outputs
          </Header>
        }
      >
        {["0", "1"].map((port, idx) => (
          <ColumnLayout key={`.${idx}`} columns={2} variant="text-grid">
            <SpaceBetween size="xs">
              <Box variant="awsui-key-label">Assert</Box>
              <MqttToggle topic={`/v1/output/out_${port}/asserted`}>
                OUT_{port} Asserted
              </MqttToggle>
              <Box variant="awsui-key-label">Help</Box>
              <ColumnLayout columns={2} variant="text-grid">
                <Popover
                  dismissAriaLabel="Close"
                  fixedWidth
                  header="About Assert/Deassert"
                  triggerType="text"
                  size="large"
                  content={
                    <>
                      <p>
                        Asserting OUT_{port} creates a short circuit between the
                        two OUT_{port} pins. What that means in terms of input
                        voltage at the respective DUT pin is dependent on how
                        the external connections are made.
                      </p>
                      <p>
                        If the output is connected between a pin and GND
                        asserting it will result in the pin going LOW.
                      </p>
                      <p>
                        If the output is connected between a pin and e.g. 3.3V
                        asserting it will result in the pin going HIGH.
                      </p>
                    </>
                  }
                >
                  About assert/deassert
                </Popover>
                <Popover
                  dismissAriaLabel="Close"
                  fixedWidth
                  header="About the voltages"
                  triggerType="text"
                  size="large"
                  content={
                    <>
                      <p>
                        The OUT_{port} voltage is measured between the two OUT_
                        {port} pins. The bar graph and the text representation
                        show the absolute voltage, because the two pins can
                        generally be used interchangeably and we assume, that
                        you will most likely not care about the polarity of this
                        voltage.
                      </p>
                      <p>
                        However, if you want to see the polarity you can refer
                        to the plots.
                      </p>
                      <p>
                        The voltages on the OUT_{port} pins is isolated from the
                        TACs GND and may float up to ±25V from it. This floating
                        of the pins is not measured. It is your responsibility
                        to make sure that the voltage stays within the specified
                        limit.
                      </p>
                    </>
                  }
                >
                  About the voltages
                </Popover>
              </ColumnLayout>
            </SpaceBetween>
            <SpaceBetween size="xs">
              <MqttBarMeter
                topic={`/v1/output/out_${port}/feedback/voltage`}
                label={`Absolute OUT_${port} voltage`}
                description={(obj: Measurement) => {
                  return `${Math.abs(obj.value).toFixed(1)}V / 5.0V`;
                }}
                to_percent={(obj: Measurement) => {
                  return (100 * Math.abs(obj.value)) / 5.0;
                }}
                additionalInfo="The absolute voltage is independent of pin orientation"
              />
              <UnloadingSection header={`OUT_${port} voltage plot (V)`}>
                <MqttChart topic={`/v1/output/out_${port}/feedback/voltage`} />
              </UnloadingSection>
            </SpaceBetween>
          </ColumnLayout>
        ))}
      </Container>

      <Container
        header={
          <Header
            variant="h2"
            description="The CAN based LXA TAC expansion interface"
          >
            LXA IOBus
          </Header>
        }
      >
        <ColumnLayout columns={2} variant="text-grid">
          <SpaceBetween key="iobus-l" size="xs">
            <Box variant="awsui-key-label">Power Supply</Box>
            <MqttToggle topic="/v1/iobus/powered">
              IOBus power supply
            </MqttToggle>
            <Box variant="awsui-key-label">Connected Devices</Box>
            <MqttBox
              topic="/v1/iobus/server/nodes"
              format={(obj: IOBusNodes) => `${obj.result.length}`}
            />
            <Box variant="awsui-key-label">Hostname / CAN interface</Box>
            <MqttBox
              topic="/v1/iobus/server/info"
              format={(obj: IOBusServerStatus) =>
                `${obj.hostname} / ${obj.can_interface}`
              }
            />
            <Box variant="awsui-key-label">Scan / Interface Status</Box>
            <MqttBox
              topic="/v1/iobus/server/info"
              format={(obj: IOBusServerStatus) => {
                let can_ok = obj.can_tx_error ? "Error" : "Okay";
                return `${obj.lss_state} / ${can_ok}`;
              }}
            />
            <Box variant="awsui-key-label">Web Interface</Box>
            <Link
              external
              externalIconAriaLabel="Opens in a new tab"
              href={`http://${window.location.hostname}:8080/`}
            >
              IOBus Server Webinterface
            </Link>
          </SpaceBetween>
          <SpaceBetween key="iobus-r" size="l">
            <MqttBarMeter
              topic="/v1/iobus/feedback/current"
              label="IOBus current"
              description={(obj: Measurement) => {
                return `${(obj.value * 1000).toFixed(0)}mA / 200mA`;
              }}
              to_percent={(obj: Measurement) => {
                return (100 * obj["value"]) / 0.2;
              }}
              additionalInfo="Too many devices may overload the bus"
            />
            <UnloadingSection header="IOBus current plot">
              <MqttChart topic="/v1/iobus/feedback/current" />
            </UnloadingSection>
            <MqttBarMeter
              topic="/v1/iobus/feedback/voltage"
              label="IOBus voltage"
              description={(obj: Measurement) => {
                return `${obj.value.toFixed(1)}V / 12.0V`;
              }}
              to_percent={(obj: Measurement) => {
                return (100 * obj.value) / 12;
              }}
              additionalInfo="The voltage will go down if the bus is overloaded"
            />
            <UnloadingSection header="IOBus voltage plot">
              <MqttChart topic="/v1/iobus/feedback/voltage" />
            </UnloadingSection>
          </SpaceBetween>
        </ColumnLayout>
      </Container>

      <Container
        header={
          <Header
            variant="h2"
            description="Control and observe the USB host port status"
          >
            USB Host
          </Header>
        }
      >
        <MqttBarMeter
          topic="/v1/usb/host/total/feedback/current"
          label={`Overall USB host current`}
          description={(obj: Measurement) => {
            return `${(obj.value * 1000).toFixed(0)}mA / 700mA`;
          }}
          to_percent={(obj: Measurement) => {
            return (100 * obj.value) / 0.7;
          }}
        />
        <ExpandableSection header="Per-port details">
          {["1", "2", "3"].map((port, idx) => (
            <Box variant="div" key={`port.${idx}`}>
              <Header variant="h3">Port {port}</Header>
              <ColumnLayout columns={2} variant="text-grid">
                <SpaceBetween size="xs">
                  <Box variant="awsui-key-label">Power Supply</Box>
                  <MqttToggle topic={`/v1/usb/host/port${port}/powered`}>
                    Port {port} power supply
                  </MqttToggle>
                  <Box variant="awsui-key-label">Connected Device</Box>
                  <MqttBox
                    topic={`/v1/usb/host/port${port}/device`}
                    format={(msg: UsbDevice) => {
                      if (msg !== null) {
                        return `${msg.id_vendor}:${msg.id_product} ${msg.manufacturer} ${msg.product}`;
                      } else {
                        return "-";
                      }
                    }}
                  />
                </SpaceBetween>
                <SpaceBetween size="l">
                  <MqttBarMeter
                    topic={`/v1/usb/host/port${port}/feedback/current`}
                    label={`USB Port ${port} current`}
                    description={(obj: Measurement) => {
                      return `${(obj.value * 1000).toFixed(0)}mA / 500mA`;
                    }}
                    to_percent={(obj: Measurement) => {
                      return (100 * obj.value) / 0.5;
                    }}
                    additionalInfo="The overall current limit takes precedence over this per-port limit"
                  />
                  <UnloadingSection header={`USB Port ${port} current plot`}>
                    <MqttChart
                      topic={`/v1/usb/host/port${port}/feedback/current`}
                    />
                  </UnloadingSection>
                </SpaceBetween>
              </ColumnLayout>
            </Box>
          ))}
        </ExpandableSection>
      </Container>

      <Container
        header={
          <Header
            variant="h2"
            description="Disable outputs and pull-up resistors for leakage-prone devices"
          >
            DUT UART
          </Header>
        }
      >
        <ColumnLayout columns={4} variant="text-grid">
          <Box>
            <Box variant="awsui-key-label">TX Enable</Box>
            <MqttToggle topic="/v1/uart/tx/enabled">TX Enable</MqttToggle>
          </Box>
          <Box>
            <Box variant="awsui-key-label">RX Enable</Box>
            <MqttToggle topic="/v1/uart/rx/enabled">RX Enable</MqttToggle>
          </Box>
        </ColumnLayout>
      </Container>
    </SpaceBetween>
  );
}
