// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2024 Pengutronix e.K.
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

import Container from "@cloudscape-design/components/container";
import Header from "@cloudscape-design/components/header";
import Table from "@cloudscape-design/components/table";
import Link from "@cloudscape-design/components/link";
import SpaceBetween from "@cloudscape-design/components/space-between";

import { useEffect, useState } from "react";

type Package = {
  package_name: string;
  version: string;
  recipe_name: string;
  license: string;
};

export function parse_manifest(text: string) {
  let packages: Package[] = [];

  // The content of `text` looks something like this:
  //
  // PACKAGE NAME: tacd
  // PACKAGE VERSION: 1.0.0
  // RECIPE NAME: tacd
  // LICENSE: GPL-2.0-or-later
  //
  // PACKAGE NAME: tacd-webinterface
  // ...

  for (var group of text.split("\n\n")) {
    let pkg: Package = {
      package_name: "",
      version: "",
      recipe_name: "",
      license: "",
    };

    for (var line of group.split("\n")) {
      if (line.startsWith("PACKAGE NAME: ")) {
        pkg.package_name = line.replace("PACKAGE NAME: ", "");
      }
      if (line.startsWith("PACKAGE VERSION: ")) {
        pkg.version = line.replace("PACKAGE VERSION: ", "");
      }
      if (line.startsWith("RECIPE NAME: ")) {
        pkg.recipe_name = line.replace("RECIPE NAME: ", "");
      }
      if (line.startsWith("LICENSE: ")) {
        pkg.license = line.replace("LICENSE: ", "");
      }
    }

    if (pkg.package_name && pkg.version && pkg.recipe_name && pkg.license) {
      packages.push(pkg);
    }
  }

  return packages;
}

export function package_table(packages?: Package[]) {
  return (
    <Table
      header={
        <Header
          variant="h3"
          description="Software packages used on this LXA TAC"
        >
          Packages
        </Header>
      }
      columnDefinitions={[
        {
          id: "package_name",
          header: "Package Name",
          cell: (p) => p.package_name,
        },
        {
          id: "version",
          header: "Version",
          cell: (p) => p.version,
        },
        {
          id: "recipe_name",
          header: "Recipe Name",
          cell: (p) => p.recipe_name,
        },
        {
          id: "license",
          header: "License",
          cell: (p) => (
            <Link href={"/docs/legal/files/" + p.recipe_name}>{p.license}</Link>
          ),
        },
      ]}
      items={packages || []}
      loading={packages === undefined}
      sortingDisabled
      resizableColumns
      stickyHeader
      trackBy="package_name"
    />
  );
}

function PackageList() {
  const [packages, setPackages] = useState<Package[]>();

  useEffect(() => {
    fetch("/docs/legal/license.manifest").then((response) => {
      if (response.ok) {
        response.text().then((text) => setPackages(parse_manifest(text)));
      }
    });
  }, []);

  return package_table(packages);
}

export default function Legal() {
  return (
    <SpaceBetween size="m">
      <Header
        variant="h1"
        description="Information regarding your rights as an LXA TAC software user"
      >
        LXA TAC / Legal Information
      </Header>

      <Container
        header={
          <Header
            variant="h2"
            description="Where to find the source code that makes up the LXA TAC software"
          >
            Availability of Source Code
          </Header>
        }
      >
        <p>
          The LXA TAC software uses many pieces of free and open source
          software. A list of these pieces of software, along with their version
          number and their respective software license, is shown below.
        </p>

        <p>
          Linux Automation GmbH provides all software components required to
          build your own LXA TAC software bundles in the form of a public Yocto
          Layer:{" "}
          <Link href="https://github.com/linux-automation/meta-lxatac">
            linux-automation/meta-lxatac
          </Link>
          .
        </p>

        <p>
          To comply with the terms of copyleft licenses like the GPL we also
          provide copies of their sources, along with the applied patches on our{" "}
          <Link href="https://downloads.linux-automation.com/lxatac/software/">
            download server
          </Link>
          .
        </p>
      </Container>

      <PackageList />
    </SpaceBetween>
  );
}
