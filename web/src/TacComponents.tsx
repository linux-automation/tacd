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

import { useEffect, useState, useRef } from "react";

import Box from "@cloudscape-design/components/box";
import Button from "@cloudscape-design/components/button";
import Cards from "@cloudscape-design/components/cards";
import ColumnLayout from "@cloudscape-design/components/column-layout";
import Container from "@cloudscape-design/components/container";
import Form from "@cloudscape-design/components/form";
import FormField from "@cloudscape-design/components/form-field";
import Header from "@cloudscape-design/components/header";
import Input from "@cloudscape-design/components/input";
import ProgressBar from "@cloudscape-design/components/progress-bar";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Spinner from "@cloudscape-design/components/spinner";
import StatusIndicator from "@cloudscape-design/components/status-indicator";

import { MqttButton } from "./MqttComponents";
import { useMqttSubscription, useMqttState } from "./mqtt";

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

export function RaucSlotStatus() {
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

        <Container
          header={
            <Header
              variant="h3"
              description="The root filesystem contains your applications and settings"
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
          />
        </Container>
      </SpaceBetween>
    );
  }
}

export function RaucInstall() {
  // eslint-disable-next-line
  const [_install_settled, _install_payload, triggerInstall] =
    useMqttState<string>("/v1/tac/update/install");

  const operation = useMqttSubscription<string>("/v1/tac/update/operation");
  const progress = useMqttSubscription<RaucProgress>("/v1/tac/update/progress");
  const last_error = useMqttSubscription<string>("/v1/tac/update/last_error");

  const [installUrl, setInstallUrl] = useState("");
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

  if (installStep === RaucInstallStep.Idle) {
    inner = (
      <form
        onSubmit={(e) => {
          e.preventDefault();
          triggerInstall(installUrl);
          setInstallUrl("");
        }}
      >
        <Form actions={<Button variant="primary">Install</Button>}>
          <FormField
            stretch
            constraintText="Specify a http:// or https:// URL to install a bundle from"
            label="Bundle URL"
          >
            <Input
              onChange={({ detail }) => setInstallUrl(detail.value)}
              value={installUrl}
              placeholder="https://some-host.example/bundle.raucb"
            />
          </FormField>
        </Form>
      </form>
    );
  }

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
    if (last_error === undefined || last_error === "") {
      inner = (
        <Form
          actions={
            <MqttButton iconName="refresh" topic="/v1/tac/reboot" send={true}>
              Reboot
            </MqttButton>
          }
        >
          <StatusIndicator>Success</StatusIndicator>
          <Box>Bundle installation finished sucessfully</Box>
        </Form>
      );
    } else {
      inner = (
        <Form
          actions={
            <Button
              formAction="none"
              onClick={(_) => setInstallStep(RaucInstallStep.Idle)}
            >
              Ok
            </Button>
          }
        >
          <StatusIndicator type="error">Failure</StatusIndicator>
          <Box>Bundle installation failed: {last_error}</Box>
        </Form>
      );
    }
  }

  return (
    <Container
      header={
        <Header
          variant="h3"
          description="Select a bundle to flash and watch the update process"
        >
          Update
        </Header>
      }
    >
      {inner}
    </Container>
  );
}

export function RaucContainer() {
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
        <RaucInstall />
        <RaucSlotStatus />
      </SpaceBetween>
    </Container>
  );
}
