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

import Cards from "@cloudscape-design/components/cards";
import Header from "@cloudscape-design/components/header";
import Link from "@cloudscape-design/components/link";
import SpaceBetween from "@cloudscape-design/components/space-between";

export default function LandingPage() {
  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="Control the LXA TAC">
        LXA TAC
      </Header>
      <Cards
        ariaLabels={{
          itemSelectionLabel: (e, t) => `select ${t.name}`,
          selectionGroupLabel: "Item selection",
        }}
        cardDefinition={{
          header: (item) => (
            <Link fontSize="heading-m" href={item.href}>
              {item.name}
            </Link>
          ),
          sections: [
            {
              id: "description",
              header: "Description",
              content: (item) => item.description,
            },
          ],
        }}
        cardsPerRow={[{ cards: 1 }, { minWidth: 500, cards: 2 }]}
        items={[
          {
            name: "Dashboard / DUT",
            href: "/#/dashboard/dut",
            description: "Control the Device under Test",
          },
          {
            name: "Dashboard / TAC",
            href: "/#/dashboard/tac",
            description: "Control various LXA TAC parameters",
          },
          {
            name: "Dashboard / Journal",
            href: "/#/dashboard/journal",
            description: "Watch the most recent systemd journal entries",
          },
          {
            name: "Settings / Labgrid",
            href: "/#/settings/labgrid",
            description: "Modify the labgrid exporter config",
          },
          {
            name: "Documentation / REST API",
            href: "/#/docs/api",
            description: "Find API definitions to automate you LXA TAC",
          },
          {
            name: "Documentation / Legal Information",
            href: "/#/docs/legal",
            description: "See the software components and their licenses",
          },
        ]}
      />
    </SpaceBetween>
  );
}
