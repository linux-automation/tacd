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
import Header from "@cloudscape-design/components/header";
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
            <Header variant="h1" description="Setup via SSH keys">
              SSH key based Wizard
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
                title: "Add SSH keys",
                description:
                  "Deploy SSH keys on your LXA TAC so you can log into it",
                content: (
                  <Container>
                    <ConfigEditor
                      path="/v1/tac/ssh/authorized_keys"
                      language="text"
                      defaultContent={SSH_AUTH_KEYS_EXAMPLE}
                    />
                  </Container>
                ),
              },
              {
                title: "Configure Labgrid",
                description: "Configure your labgrid Exporter",
                isOptional: true,
                content: (
                  <Container>
                    <LabgridConfig />
                  </Container>
                ),
              },
              {
                title: "Test Labgrid",
                description:
                  "Make sure your labgrid Exporter Service looks healty",
                isOptional: true,
                content: (
                  <Container>
                    <LabgridService />
                  </Container>
                ),
              },
              {
                title: "Complete Setup",
                description: "Make sure everything is working alright",
                content: (
                  <Container>
                    <Box>You are about to complete the setup wizard.</Box>
                    <Box>
                      You will not be able to re-enter the setup wizard to
                      deploy new SSH keys via the web interface. You can however
                      re-enable the setup wizard using the buttons and the
                      screen on the device. If you do not have physical access
                      the TAC you should make sure that you can log in to the
                      device via the SSH keys you have deployed before pressing
                      "Done".
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
