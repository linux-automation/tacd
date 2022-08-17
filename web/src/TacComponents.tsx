import Box from "@cloudscape-design/components/box";
import Cards from "@cloudscape-design/components/cards";
import ColumnLayout from "@cloudscape-design/components/column-layout";
import Container from "@cloudscape-design/components/container";
import Header from "@cloudscape-design/components/header";
import ProgressBar from "@cloudscape-design/components/progress-bar";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Spinner from "@cloudscape-design/components/spinner";

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
  const operation = useMqttSubscription<string>("/v1/tac/update/operation");
  const progress = useMqttSubscription<RaucProgress>("/v1/tac/update/progress");

  let inner = null;

  if (operation === "installing") {
    let valid = progress !== undefined;
    let value = progress === undefined ? 0 : progress.percentage;
    let message = progress === undefined ? "" : progress.message;

    inner = (
      <ProgressBar
        status={valid ? "in-progress" : "error"}
        value={value}
        description="Installation may take some minutes"
        additionalInfo={message}
      />
    );
  } else {
    inner = <Box>Todo: file upload/URL entry</Box>;
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
