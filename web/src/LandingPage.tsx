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
            description: "Modify the Labgrid exporter config",
          },
          {
            name: "Documentation / REST API",
            href: "/#/docs/api",
            description: "Find API definitions to automate you LXA TAC",
          },
        ]}
      />
    </SpaceBetween>
  );
}
