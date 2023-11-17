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

import { useState } from "react";

import Box from "@cloudscape-design/components/box";
import Container from "@cloudscape-design/components/container";
import Icon from "@cloudscape-design/components/icon";
import Header from "@cloudscape-design/components/header";
import Link from "@cloudscape-design/components/link";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Spinner from "@cloudscape-design/components/spinner";
import Wizard from "@cloudscape-design/components/wizard";

import { LabgridService, LabgridConfig } from "./SettingsLabgrid";
import { ConfigEditor } from "./ConfigEditor";
import { useMqttState } from "./mqtt";

const SSH_AUTH_KEYS_EXAMPLE =
  "# Paste one (or multiple) of your ssh public keys here.\n" +
  "# Use 'cat ~/.ssh/id_*.pub' to get a list of your ssh public\n" +
  "# keys or ssh-keygen if you don't have any yet.\n" +
  "# They will look something like this:\n" +
  "# ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBlPtT5dnGcZn0Z6FyD6VGqt3Jx0s+BHhMahxR0KlJ8G tux@igloo\n";

export default function Setup() {
  const [setupModeSettled, setupMode, setSetupMode] =
    useMqttState<boolean>("/v1/tac/setup_mode");
  const [activeStepIndex, setActiveStepIndex] = useState(0);

  if (setupMode === undefined || !setupModeSettled) {
    return (
      <div id="setup_wizard_outer">
        <div id="setup_wizard_inner">
          <div id="setup_wizard_spinner">
            <Spinner size="large" />
          </div>
        </div>
      </div>
    );
  }

  if (!setupMode) {
    window.location.replace("/#/");
  }

  return (
    <div id="setup_wizard_outer">
      <div id="setup_wizard_inner">
        <Container
          header={
            <Header variant="h1" description="Get started with your TAC">
              LXA TAC Setup Wizard
            </Header>
          }
        >
          <Wizard
            i18nStrings={{
              stepNumberLabel: (stepNumber) => `Step ${stepNumber}`,
              collapsedStepsLabel: (stepNumber, stepsCount: number) =>
                `Step ${stepNumber} of ${stepsCount}`,
              skipToButtonLabel: (step, stepNumber) => `Skip to ${step.title}`,
              navigationAriaLabel: "Steps",
              cancelButton: "Cancel",
              previousButton: "Back",
              nextButton: "Next",
              submitButton: "Done",
              optional: "optional",
            }}
            onSubmit={() => setSetupMode(false)}
            onNavigate={({ detail }) =>
              setActiveStepIndex(detail.requestedStepIndex)
            }
            activeStepIndex={activeStepIndex}
            allowSkipTo
            steps={[
              {
                title: "Welcome",
                description: "Welcome to your TAC and its setup mode",
                content: (
                  <Container>
                    <Box variant="p">Hey there,</Box>
                    <Box variant="p">
                      thank you for buying this TAC. We hope you'll like it!
                    </Box>
                    <Box variant="p">
                      Before you can get started using your TAC we need to set
                      up a few things so they match your preferences. Some of
                      these preferences can only be set via the web interface in
                      this special setup mode, because they affect the security
                      and inner workings of your TAC. To configure them once the
                      setup mode is done you either need ssh access to your TAC
                      or physical access to re-enable the setup mode via the
                      buttons on the TAC.
                    </Box>
                    <Box variant="p">
                      Ready to get started? Then maybe have a quick look at the{" "}
                      <Link
                        external
                        href="https://www.linux-automation.com/lxatac-M02/index.html"
                      >
                        online manual
                      </Link>{" "}
                      first and then click "Next" to continue the setup.
                    </Box>
                    <br />
                    <SpaceBetween direction="horizontal" size="s">
                      <Icon url="/logo.svg" size="large" />
                      <Box variant="p">
                        Greetings,
                        <br />
                        the Linux Automation GmbH team
                      </Box>
                    </SpaceBetween>
                  </Container>
                ),
              },
              {
                title: "Add SSH keys",
                description:
                  "Deploy SSH keys onto your LXA TAC so you can log into it",
                content: (
                  <Container>
                    <SpaceBetween size="s">
                      <Box variant="p">
                        For many actions on the LXA TAC you need access to it
                        via ssh. Permissions to ssh into the TAC are managed via
                        a list of ssh public keys, that allow logging in as the
                        root user.
                        <br />
                        Paste a list of ssh public keys into the text box below,
                        to allow them to access the TAC, and click "Save".
                        Afterwards you should be able to log into your TAC like
                        this:
                        <Box
                          variant="code"
                          display="block"
                          padding="s"
                          fontSize="body-m"
                        >
                          $ ssh root@{window.location.hostname}
                        </Box>
                        Make sure to check if logging in works before leaving
                        the setup mode.
                      </Box>
                      <ConfigEditor
                        path="/v1/tac/ssh/authorized_keys"
                        language="text"
                        defaultContent={SSH_AUTH_KEYS_EXAMPLE}
                      />
                    </SpaceBetween>
                  </Container>
                ),
              },
              {
                title: "Configure Labgrid",
                description: "Configure your labgrid Exporter",
                isOptional: true,
                content: (
                  <Container>
                    <SpaceBetween size="s">
                      <Box variant="p">
                        The LXA TAC comes with a mostly pre-configured labgrid
                        exporter, that exports a lot of resources that are built
                        into the TAC or can be connected to it via USB.
                        <br />
                        You may however want to configure the labgrid
                        coordinator IP address/hostname on the "Environment" tab
                        or export additional resources in the "User Config" tab.
                        <br />
                        Once you have made the required changes click "Save" and
                        test your configuration by clicking "Next".
                      </Box>
                      <LabgridConfig />
                    </SpaceBetween>
                  </Container>
                ),
              },
              {
                title: "Test Labgrid",
                description:
                  "Make sure your labgrid Exporter Service looks healthy",
                isOptional: true,
                content: (
                  <Container>
                    <SpaceBetween size="m">
                      <Box variant="p">
                        In this step you can check if the labgrid exporter
                        starts as expected and the correct resources are
                        exported. Use the "Start", "Stop" and "Restart" buttons
                        to control the labgrid exporter systemd service and
                        observe the systemd journal output in the text window
                        above them.
                        <br />
                        Go back to the exporter configuration step to make
                        changes and click "Next" once you are satisfied.
                      </Box>
                      <LabgridService />
                    </SpaceBetween>
                  </Container>
                ),
              },
              {
                title: "Complete Setup",
                description: "Make sure everything is working alright",
                content: (
                  <Container>
                    <Box variant="p">
                      You are about to complete the setup wizard.
                    </Box>
                    <Box variant="p">
                      You will not be able to re-enter the setup wizard via the
                      web interface. You can however re-enable the setup wizard
                      using the buttons and the screen on the device. If you do
                      not have physical access to the TAC you should make sure
                      that you can log in now using:
                      <Box
                        variant="code"
                        display="block"
                        padding="s"
                        fontSize="body-m"
                      >
                        $ ssh root@{window.location.hostname}
                      </Box>
                      before pressing "Done".
                    </Box>
                  </Container>
                ),
              },
            ]}
          />
        </Container>
      </div>
    </div>
  );
}
