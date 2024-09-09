import fs from "node:fs";

import { parse_manifest } from "./Legal";

const manifest_ref = [
  {
    package_name: "tacd",
    version: "0.1.0+gitAUTOINC+803b2084b2",
    recipe_name: "tacd",
    license: "GPL-2.0-or-later",
  },
  {
    package_name: "tacd-webinterface",
    version: "0.1.0+gitAUTOINC+803b2084b2",
    recipe_name: "tacd-webinterface",
    license: "GPL-2.0-or-later",
  },
];

it("parses the manifest", () => {
  const manifest_raw = fs.readFileSync(
    "../demo_files/usr/share/common-licenses/license.manifest",
    "utf-8",
  );

  const manifest = parse_manifest(manifest_raw);

  expect(manifest).toEqual(manifest_ref);
});
