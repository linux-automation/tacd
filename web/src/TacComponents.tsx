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
// with this library; if not, see <https://www.gnu.org/licenses/>.

import { useEffect, useState, useRef } from "react";

import Alert from "@cloudscape-design/components/alert";
import Box from "@cloudscape-design/components/box";
import Cards from "@cloudscape-design/components/cards";
import Checkbox from "@cloudscape-design/components/checkbox";
import ColumnLayout from "@cloudscape-design/components/column-layout";
import Container from "@cloudscape-design/components/container";
import Form from "@cloudscape-design/components/form";
import Header from "@cloudscape-design/components/header";
import ProgressBar from "@cloudscape-design/components/progress-bar";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Spinner from "@cloudscape-design/components/spinner";
import Table from "@cloudscape-design/components/table";

import { MqttButton, MqttToggle } from "./MqttComponents";
import { useMqttSubscription } from "./mqtt";

type RootfsSlot = {
  activated_count: string;
  activated_timestamp: string;
  bootname: string;
  boot_status: string;
  bundle_build: string;
  bundle_compatible: string;
  bundle_description: string;
  bundle_version: string;
  device: string;
  fs_type: string;
  installed_count: string;
  installed_timestamp: string;
  name: string;
  sha256: string;
  size: string;
  slot_class: string;
  state: string;
  status: string;
};

type BootloaderSlot = {
  bundle_build: string;
  bundle_compatible: string;
  bundle_description: string;
  bundle_version: string;
  device: string;
  fs_type: string;
  installed_count: string;
  installed_timestamp: string;
  name: string;
  sha256: string;
  size: string;
  state: string;
  status: string;
  slot_class: string;
};

type RaucSlots = {
  rootfs_0: RootfsSlot;
  rootfs_1: RootfsSlot;
  bootloader_0: BootloaderSlot;
};

type RaucProgress = {
  percentage: number;
  message: string;
  nesting_depth: number;
};

enum RaucInstallStep {
  Idle,
  Installing,
  Done,
}

enum UsbOverload {
  Total = "Total",
  Port1 = "Port1",
  Port2 = "Port2",
  Port3 = "Port3",
}

enum OutputState {
  On = "On",
  Off = "Off",
  OffFloating = "OffFloating",
  Changing = "Changing",
  InvertedPolarity = "InvertedPolarity",
  OverCurrent = "OverCurrent",
  OverVoltage = "OverVoltage",
  RealtimeViolation = "RealtimeViolation",
}

type Duration = {
  secs: number;
  nanos: number;
};

type UpstreamBundle = {
  compatible: string;
  version: string;
  manifest_hash: string;
  effective_url: string;
  newer_than_installed: boolean;
};

type Channel = {
  name: string;
  display_name: string;
  description: string;
  url: string;
  polling_interval?: Duration;
  enabled: boolean;
  primary: boolean;
  bundle?: UpstreamBundle;
};

type UpdateRequest = {
  manifest_hash: string;
  url: string;
};

interface SlotStatusProps {
  setCmdHint: (hint: React.ReactNode | null) => void;
}

export function SlotStatus(props: SlotStatusProps) {
  const slot_status = useMqttSubscription<RaucSlots>("/v1/tac/update/slots");

  if (slot_status === undefined) {
    return <Spinner />;
  } else {
    let booted_slot = [];

    if (slot_status.rootfs_0.state === "booted") {
      booted_slot.push(slot_status.rootfs_0);
    }

    if (slot_status.rootfs_1.state === "booted") {
      booted_slot.push(slot_status.rootfs_1);
    }

    return (
      <SpaceBetween size="m">
        <Container
          header={
            <Header
              variant="h3"
              description="The root file system contains your applications and settings"
            >
              Root Filesystem Slots
            </Header>
          }
        >
          <Cards
            selectedItems={booted_slot}
            cardDefinition={{
              header: (e) => (typeof e === "string" ? e : e.bootname),
              sections: [
                {
                  id: "status",
                  header: "Status",
                  content: (e) => e.status,
                },
                {
                  id: "boot_status",
                  header: "Boot Status",
                  content: (e) => e.boot_status,
                },
                {
                  id: "build_date",
                  header: "Build Date",
                  content: (e) => e.bundle_build,
                },
                {
                  id: "install_date",
                  header: "Installation Date",
                  content: (e) => e.installed_timestamp,
                },
              ],
            }}
            cardsPerRow={[{ cards: 1 }, { minWidth: 500, cards: 2 }]}
            items={[slot_status.rootfs_0, slot_status.rootfs_1]}
            loadingText="Loading resources"
            selectionType="single"
            trackBy="name"
            onSelectionChange={(ev) => {
              props.setCmdHint(
                <p>
                  # Mark a RAUC slot as active:
                  <br />
                  rauc status mark-active {ev.detail.selectedItems[0].bootname}
                </p>,
              );
            }}
          />
        </Container>

        <Container
          header={
            <Header
              variant="h3"
              description="The bootloader is responsible for loading the Linux kernel"
            >
              Bootloader Slot
            </Header>
          }
        >
          <ColumnLayout columns={3} variant="text-grid">
            <Box>
              <Box variant="awsui-key-label">Status</Box>
              <Box>{slot_status.bootloader_0.status}</Box>
            </Box>
            <Box>
              <Box variant="awsui-key-label">Build Date</Box>
              <Box>{slot_status.bootloader_0.bundle_build}</Box>
            </Box>
            <Box>
              <Box variant="awsui-key-label">Installation Date</Box>
              <Box>{slot_status.bootloader_0.installed_timestamp}</Box>
            </Box>
          </ColumnLayout>
        </Container>
      </SpaceBetween>
    );
  }
}

export function UpdateConfig() {
  return (
    <Container
      header={
        <Header
          variant="h3"
          description="Decide how updates are handled on this TAC"
        >
          Update Configuration
        </Header>
      }
    >
      <ColumnLayout columns={3} variant="text-grid">
        <Box>
          <Box variant="awsui-key-label">Update Polling</Box>
          <MqttToggle topic="/v1/tac/update/enable_polling">
            Periodically check for updates
          </MqttToggle>
        </Box>
        <Box>
          <Box variant="awsui-key-label">Auto Install</Box>
          <MqttToggle topic="/v1/tac/update/enable_auto_install">
            Automatically install and boot updates
          </MqttToggle>
        </Box>
      </ColumnLayout>
    </Container>
  );
}

interface UpdateChannelsProps {
  setCmdHint: (hint: React.ReactNode | null) => void;
}

export function UpdateChannels(props: UpdateChannelsProps) {
  const channels_topic = useMqttSubscription<Array<Channel>>(
    "/v1/tac/update/channels",
  );
  const enable_polling_topic = useMqttSubscription<Array<Channel>>(
    "/v1/tac/update/enable_polling",
  );

  const channels = channels_topic !== undefined ? channels_topic : [];
  const enable_polling =
    enable_polling_topic !== undefined ? enable_polling_topic : false;

  return (
    <Table
      header={
        <Header
          variant="h3"
          description="Enabled update channels are periodically checked for updates"
        >
          Update Channels
        </Header>
      }
      footer={
        <Form
          actions={
            <MqttButton
              iconName="refresh"
              topic="/v1/tac/update/channels/reload"
              send={true}
            >
              Reload
            </MqttButton>
          }
        />
      }
      columnDefinitions={[
        {
          id: "name",
          header: "Name",
          cell: (e) => e.display_name,
        },
        {
          id: "enabled",
          header: "Enabled",
          cell: (e) => (
            <Checkbox
              checked={e.enabled}
              disabled={!enable_polling}
              onChange={() => {
                let action = e.enabled ? "Disable" : "Enable";
                let cmd = e.enabled ? "rauc-disable-cert" : "rauc-enable-cert";

                props.setCmdHint(
                  <p>
                    # {action} the {e.display_name} update channel:
                    <br />
                    {cmd} {e.name}.cert.pem
                  </p>,
                );
              }}
            />
          ),
        },
        {
          id: "description",
          header: "Description",
          maxWidth: "50em",
          cell: (e) => (
            <SpaceBetween size="xs">
              {e.description.split("\n").map((p) => (
                <span>{p}</span>
              ))}
            </SpaceBetween>
          ),
        },
        {
          id: "interval",
          header: "Polling Interval",
          cell: (e) => {
            if (!enable_polling) {
              return "Disabled";
            }

            if (!e.polling_interval) {
              return "Never";
            }

            let seconds = e.polling_interval.secs;
            let minutes = seconds / 60;
            let hours = minutes / 60;
            let days = hours / 24;

            if (Math.floor(days) === days) {
              return days === 1 ? "Daily" : `Every ${days} Days`;
            }

            if (Math.floor(hours) === hours) {
              return hours === 1 ? "Hourly" : `Every ${hours} Hours`;
            }

            if (Math.floor(days) === days) {
              return minutes === 1
                ? "Once a minute"
                : `Every ${minutes} Minutes`;
            }

            return `Every ${seconds} Seconds`;
          },
        },
        {
          id: "upgrade",
          header: "Upgrade",
          cell: (e) => {
            if (!e.enabled) {
              return "Not enabled";
            }

            if (!e.primary) {
              return "Not primary";
            }

            if (!e.bundle) {
              if (enable_polling) {
                return <Spinner />;
              } else {
                return "Polling disabled";
              }
            }

            if (!e.bundle.newer_than_installed) {
              return "Up to date";
            }

            const request: UpdateRequest = {
              manifest_hash: e.bundle.manifest_hash,
              url: e.bundle.effective_url,
            };

            return (
              <MqttButton
                iconName="download"
                topic="/v1/tac/update/install"
                send={request}
              >
                Upgrade
              </MqttButton>
            );
          },
        },
      ]}
      items={channels}
      sortingDisabled
      trackBy="name"
    />
  );
}

export function ProgressNotification() {
  const operation = useMqttSubscription<string>("/v1/tac/update/operation");
  const progress = useMqttSubscription<RaucProgress>("/v1/tac/update/progress");
  const last_error = useMqttSubscription<string>("/v1/tac/update/last_error");

  const [installStep, setInstallStep] = useState(RaucInstallStep.Idle);
  const prev_operation = useRef<string | undefined>(undefined);

  useEffect(() => {
    if (prev_operation.current === "idle" && operation === "installing") {
      setInstallStep(RaucInstallStep.Installing);
    }

    if (prev_operation.current === "installing" && operation === "idle") {
      setInstallStep(RaucInstallStep.Done);
    }

    prev_operation.current = operation;
  }, [operation]);

  let inner = null;

  if (installStep === RaucInstallStep.Installing) {
    let valid = progress !== undefined;
    let value = progress === undefined ? 0 : progress.percentage;
    let message = progress === undefined ? "" : progress.message;

    inner = (
      <ProgressBar
        status={valid ? "in-progress" : "error"}
        value={value}
        description="Installation may take several minutes"
        additionalInfo={message}
      />
    );
  }

  if (installStep === RaucInstallStep.Done) {
    if (last_error !== undefined && last_error !== "") {
      inner = (
        <ProgressBar
          status={"error"}
          value={100}
          description="Failure"
          additionalInfo="Bundle installation failed"
        />
      );
    }
  }

  return (
    <Alert
      statusIconAriaLabel="Info"
      header="Installing Operating System Update"
      visible={inner !== null}
    >
      {inner}
    </Alert>
  );
}

export function RebootNotification() {
  const should_reboot = useMqttSubscription<boolean>(
    "/v1/tac/update/should_reboot",
  );

  return (
    <Alert
      statusIconAriaLabel="Info"
      visible={should_reboot === true}
      action={
        <MqttButton iconName="refresh" topic="/v1/tac/reboot" send={true}>
          Reboot
        </MqttButton>
      }
      header="Reboot into other slot"
    >
      There is a newer operating system bundle installed in the other boot slot.
      Reboot now to use it.
    </Alert>
  );
}

interface UpdateContainerProps {
  setCmdHint: (hint: React.ReactNode | null) => void;
}

export function UpdateContainer(props: UpdateContainerProps) {
  return (
    <Container
      header={
        <Header
          variant="h2"
          description="Check your redundant update status and slots"
        >
          RAUC
        </Header>
      }
    >
      <SpaceBetween size="m">
        <UpdateConfig />
        <UpdateChannels setCmdHint={props.setCmdHint} />
        <SlotStatus setCmdHint={props.setCmdHint} />
      </SpaceBetween>
    </Container>
  );
}

export function UpdateNotification() {
  const channels = useMqttSubscription<Array<Channel>>(
    "/v1/tac/update/channels",
  );

  let updates = [];

  if (channels !== undefined) {
    for (let ch of channels) {
      if (ch.enabled && ch.bundle && ch.bundle.newer_than_installed) {
        const request: UpdateRequest = {
          manifest_hash: ch.bundle.manifest_hash,
          url: ch.bundle.effective_url,
        };

        updates.push({
          name: ch.name,
          display_name: ch.display_name,
          request: request,
        });
      }
    }
  }

  const install_buttons = updates.map((u) => (
    <MqttButton
      key={u.name}
      iconName="download"
      topic="/v1/tac/update/install"
      send={u.request}
    >
      Install new {u.display_name} bundle
    </MqttButton>
  ));

  let text =
    "There is a new operating system update available for installation";

  if (updates.length > 1) {
    text =
      "There are new operating system updates available available for installation";
  }

  return (
    <Alert
      statusIconAriaLabel="Info"
      visible={updates.length > 0}
      action={<SpaceBetween size="xs">{install_buttons}</SpaceBetween>}
      header="Update your LXA TAC"
    >
      {text}
    </Alert>
  );
}

export function LocatorNotification() {
  const locator = useMqttSubscription<boolean>("/v1/tac/display/locator");

  return (
    <Alert
      statusIconAriaLabel="Info"
      visible={locator === true}
      action={
        <MqttButton topic="/v1/tac/display/locator" send={false}>
          Found it!
        </MqttButton>
      }
      header="Find this TAC"
    >
      Someone is looking for this TAC.
    </Alert>
  );
}

export function IOBusFaultNotification() {
  const overload = useMqttSubscription<boolean>("/v1/iobus/feedback/fault");

  return (
    <Alert
      statusIconAriaLabel="Warning"
      type="warning"
      visible={overload === true}
      header="The IOBus power supply is overloaded"
    >
      The power supply on the IOBus connector is either shorted or overloaded by
      too many devices on the bus.
    </Alert>
  );
}

export function OverTemperatureNotification() {
  const warning = useMqttSubscription<string>("/v1/tac/temperatures/warning");

  return (
    <Alert
      statusIconAriaLabel="Warning"
      type="warning"
      visible={warning !== undefined && warning !== "Okay"}
      header="Your LXA TAC is overheating"
    >
      The LXA TAC's temperature is{" "}
      {warning === "SocCritical" ? "critical" : "high"}. Provide better airflow
      and check for overloads!
    </Alert>
  );
}

export function UsbOverloadNotification() {
  const overload = useMqttSubscription<UsbOverload | null>(
    "/v1/usb/host/overload",
  );

  let header = "One of the USB host ports is overloaded";
  let detail = "";

  switch (overload) {
    case UsbOverload.Total:
      header = "The USB host ports are overloaded";
      detail = "devices";
      break;
    case UsbOverload.Port1:
      detail = "the device from port 1";
      break;
    case UsbOverload.Port2:
      detail = "the device from port 2";
      break;
    case UsbOverload.Port3:
      detail = "the device from port 3";
      break;
  }

  return (
    <Alert
      statusIconAriaLabel="Warning"
      type="warning"
      visible={overload !== undefined && overload !== null}
      header={header}
    >
      Disconnect {detail} or use a powered hub to resolve this issue.
    </Alert>
  );
}

export function PowerFailNotification() {
  const state = useMqttSubscription<OutputState>("/v1/dut/powered");

  let reason = null;

  switch (state) {
    case OutputState.InvertedPolarity:
      reason = "an inverted polarity event";
      break;
    case OutputState.OverCurrent:
      reason = "an overcurrent event";
      break;
    case OutputState.OverVoltage:
      reason = "an overvoltage event";
      break;
    case OutputState.RealtimeViolation:
      reason = "a realtime violation";
      break;
  }

  return (
    <Alert
      statusIconAriaLabel="Info"
      visible={reason !== null}
      action={
        <SpaceBetween size="xs">
          <MqttButton iconName="refresh" topic="/v1/dut/powered" send={"On"}>
            Turn DUT back on
          </MqttButton>
          <MqttButton
            iconName="status-stopped"
            topic="/v1/dut/powered"
            send={"Off"}
          >
            Keep DUT powered off
          </MqttButton>
        </SpaceBetween>
      }
      header="DUT powered off"
    >
      The DUT was powered off due to {reason}.
    </Alert>
  );
}

interface CmdHintNotificationProps {
  cmdHint: React.ReactNode | null;
  setCmdHint: (hint: React.ReactNode | null) => void;
}

export function CmdHintNotification(props: CmdHintNotificationProps) {
  return (
    <Alert
      dismissible
      statusIconAriaLabel="Info"
      visible={props.cmdHint !== null}
      header="Complete an action on the command line"
      onDismiss={() => props.setCmdHint(null)}
    >
      The selected action can not be performed in the web interface. To complete
      it use the command line interface instead:
      <Box variant="code">{props.cmdHint}</Box>
    </Alert>
  );
}
