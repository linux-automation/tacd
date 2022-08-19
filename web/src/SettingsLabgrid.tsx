import "ace-builds/src-noconflict/ace"; // Load Ace Editor
import { config } from "ace-builds/src-noconflict/ace";

import Button from "@cloudscape-design/components/button";
import Form from "@cloudscape-design/components/form";
import Header from "@cloudscape-design/components/header";
import Container from "@cloudscape-design/components/container";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Tabs from "@cloudscape-design/components/tabs";
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
// he basePath from which to load modules is not set.
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
};

function ConfigEditor(props: ConfigEditorProps) {
  const [preferences, setPreferences] = useState<
    CodeEditorProps.Preferences | undefined
  >(undefined);

  const [content, setContent] = useState<string | undefined>();
  const [newContent, setNewContent] = useState<string | undefined>();

  function loadContent() {
    fetch(props.path)
      .then((response) => response.text())
      .then((text) => setContent(text));
  }

  useEffect(() => {
    loadContent();
    // eslint-disable-next-line
  }, []);

  function save() {
    if (newContent !== undefined) {
      setContent(undefined);

      fetch(props.path, { method: "PUT", body: newContent }).then(() =>
        loadContent()
      );
    }
  }

  return (
    <Form
      actions={
        <SpaceBetween direction="horizontal" size="xs">
          <Button formAction="none" variant="primary" onClick={save}>
            Save
          </Button>
        </SpaceBetween>
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

export default function SettingsLabgrid() {
  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="Configure the labgrid exporter">
        LXA TAC / Labgrid Settings
      </Header>

      <Container
        header={
          <Header
            variant="h2"
            description="Restart the Labgrid exporter service and view its log"
          >
            Labgrid Exporter Status
          </Header>
        }
      ></Container>

      <Container
        header={
          <Header variant="h2" description="Edit the labgrid exporter config">
            Config Files
          </Header>
        }
      >
        <Tabs
          tabs={[
            {
              label: "User Config",
              id: "user",
              content: (
                <ConfigEditor path="/v1/labgrid/userconfig" language="yaml" />
              ),
            },
            {
              label: "Environment",
              id: "env",
              content: (
                <ConfigEditor path="/v1/labgrid/environment" language="sh" />
              ),
            },
            {
              label: "System Config",
              id: "system",
              content: (
                <ConfigEditor path="v1/labgrid/configuration" language="yaml" />
              ),
            },
          ]}
        />
      </Container>
    </SpaceBetween>
  );
}
