import SwaggerUI from "swagger-ui-react";
import "swagger-ui-react/swagger-ui.css";

import { useEffect, useState, useMemo } from "react";

import Button from "@cloudscape-design/components/button";
import Form from "@cloudscape-design/components/form";
import Header from "@cloudscape-design/components/header";
import SpaceBetween from "@cloudscape-design/components/space-between";
import Spinner from "@cloudscape-design/components/spinner";

export const OPENAPI_URL = "/v1/openapi.json";

interface SwaggerViewProps {
  filter?: string;
}

export function SwaggerView(props: SwaggerViewProps) {
  const [openapi, setOpenapi] = useState<any>();

  const content = useMemo(
    () => (openapi === undefined ? <Spinner /> : <SwaggerUI spec={openapi} />),
    [openapi]
  );

  useEffect(() => {
    fetch(OPENAPI_URL)
      .then((response) => response.json())
      .then((obj) => {
        if (props.filter !== undefined) {
          let paths: { [n: string]: any } = {};
          let tags_to_keep = new Set<string>();

          // OpenAPI paths can contain parameters and look like this:
          //   "/v1/output/{out_n}/feedback/voltage"
          // Our props.filter does however look like this:
          //   "/v1/output/out_0/feedback/voltage"
          // See if our filter matches a parameterized path by constructing
          // a RegExp and matching our filter with it
          for (let path in obj.paths) {
            // "/v1/output/{out_n}/feedback/voltage" ->
            //   [ "", "v1", "output", "{out_n}", "feedback", "voltage" ]
            let frags = path.split("/");

            // [ "", "v1", "output", "{out_n}", "feedback", "voltage" ] ->
            //   [ "", "v1", "output", "[^/]+", "feedback", "voltage" ]
            let frags_no_var = frags.map((el) =>
              el[0] === "{" && el[el.length - 1] === "}" ? "[^/]+" : el
            );

            // [ "", "v1", "output", "[^/]+", "feedback", "voltage" ] ->
            //   "\\/v1\\/output\\/[^/]+\\/feedback\\/voltage"
            let path_no_var = frags_no_var.join("\\/");

            let path_regex = new RegExp("^" + path_no_var + "$");

            if (path_regex.test(props.filter)) {
              paths[path] = obj.paths[path];

              // Extra tag descriptions clutter the view when filtering.
              // Maintain a set of tags that are actually used.
              for (let method of ["get", "post", "put"]) {
                if (
                  paths[path][method] !== undefined &&
                  paths[path][method]["tags"] !== undefined
                ) {
                  for (let tag of paths[path][method]["tags"]) {
                    tags_to_keep.add(tag);
                  }
                }
              }
            }
          }

          obj.paths = paths;

          // Filter out all the tags that are not actually used
          obj.tags = obj.tags.filter((t: { [n: string]: any }) =>
            tags_to_keep.has(t.name)
          );
        }

        setOpenapi(obj);
      });
    // eslint-disable-next-line
  }, []);

  return content;
}

export default function ApiDocs() {
  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="APIs to automate tasks with your TAC">
        LXA TAC / REST API Documentation
      </Header>

      <SwaggerView />

      <Form
        actions={
          <Button
            iconName="download"
            href={OPENAPI_URL}
            variant="link"
            formAction="none"
          >
            Download OpenAPI specification
          </Button>
        }
      />
    </SpaceBetween>
  );
}
