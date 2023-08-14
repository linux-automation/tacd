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

import "ace-builds/src-noconflict/ace"; // Load Ace Editor
import { config } from "ace-builds/src-noconflict/ace";

import Button from "@cloudscape-design/components/button";
import Form from "@cloudscape-design/components/form";
import CodeEditor, {
  CodeEditorProps,
} from "@cloudscape-design/components/code-editor";

import { useEffect, useState } from "react";

// Make sure to only require (and thus pack using webpack) modules that are
// actually used.
// Requiring with file-loader returns a URL (this is different from what
// require usually does, e.g. returning an object).
// This behaviour is borrowed from ace webpack-resolver.js, which does however
// result in each and every ace module landing in our build directory.
// eslint is really not okay with us using require like that but I currently
// do not know how to do it right without modiying webpack.config.js which
// requires opening a whole other can of worms.
const aceModules: { [name: string]: string } = {
  // eslint-disable-next-line
  "ace/mode/yaml_worker": require("file-loader?esModule=false!ace-builds/src-noconflict/worker-yaml.js"),
  // eslint-disable-next-line
  "ace/theme/dawn": require("file-loader?esModule=false!ace-builds/src-noconflict/theme-dawn.js"),
  // eslint-disable-next-line
  "ace/mode/yaml": require("file-loader?esModule=false!ace-builds/src-noconflict/mode-yaml.js"),
  // eslint-disable-next-line
  "ace/mode/sh": require("file-loader?esModule=false!ace-builds/src-noconflict/mode-sh.js"),
  // eslint-disable-next-line
  "ace/ext/language_tools": require("file-loader?esModule=false!ace-builds/src-noconflict/ext-language_tools.js"),
  // eslint-disable-next-line
  "ace/snippets/sh": require("file-loader?esModule=false!ace-builds/src-noconflict/snippets/sh.js"),
};

// Only here to silence the error in the browser console telling us that
// the basePath from which to load modules is not set.
// (We don't care, as we will use our own moduleUrl resolving function)
config.set("basePath", "/");

config.moduleUrl = function (name: string, component?: string) {
  let url = aceModules[name];

  if (url === undefined) {
    console.log("Missing ace module ", name, component);
  }

  return aceModules[name];
};

type ConfigEditorProps = {
  path: string;
  language: CodeEditorProps.Language;
  defaultContent?: string;
};

export function ConfigEditor(props: ConfigEditorProps) {
  const [preferences, setPreferences] = useState<
    CodeEditorProps.Preferences | undefined
  >(undefined);

  const [content, setContent] = useState<string | undefined>();
  const [newContent, setNewContent] = useState<string | undefined>();

  function loadContent() {
    fetch(props.path).then((response) => {
      if (response.ok) {
        response.text().then((text) => setContent(text));
      } else {
        setContent(props.defaultContent || "");
      }
    });
  }

  useEffect(() => {
    loadContent();
    // eslint-disable-next-line
  }, []);

  function save() {
    if (newContent !== undefined) {
      setContent(undefined);

      fetch(props.path, { method: "PUT", body: newContent }).then(() =>
        loadContent(),
      );
    }
  }

  return (
    <Form
      actions={
        <Button formAction="none" variant="primary" onClick={save}>
          Save
        </Button>
      }
    >
      <CodeEditor
        ace={ace}
        language={props.language}
        value={content || ""}
        preferences={preferences}
        onPreferencesChange={(e) => setPreferences(e.detail)}
        onDelayedChange={(e) => setNewContent(e.detail.value)}
        loading={content === undefined}
        i18nStrings={{
          loadingState: "Loading code editor",
          errorState: "There was an error loading the code editor.",
          errorStateRecovery: "Retry",
          editorGroupAriaLabel: "Code editor",
          statusBarGroupAriaLabel: "Status bar",
          cursorPosition: (row, column) => `Ln ${row}, Col ${column}`,
          errorsTab: "Errors",
          warningsTab: "Warnings",
          preferencesButtonAriaLabel: "Preferences",
          paneCloseButtonAriaLabel: "Close",
          preferencesModalHeader: "Preferences",
          preferencesModalCancel: "Cancel",
          preferencesModalConfirm: "Confirm",
          preferencesModalWrapLines: "Wrap lines",
          preferencesModalTheme: "Theme",
          preferencesModalLightThemes: "Light themes",
          preferencesModalDarkThemes: "Dark themes",
        }}
      />
    </Form>
  );
}
