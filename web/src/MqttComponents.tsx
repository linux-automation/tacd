import React, { useEffect, useState } from "react";

import Box from "@cloudscape-design/components/box";
import { BoxProps } from "@cloudscape-design/components/box";
import ProgressBar from "@cloudscape-design/components/progress-bar";
import Spinner from "@cloudscape-design/components/spinner";
import Toggle from "@cloudscape-design/components/toggle";
import Button from "@cloudscape-design/components/button";
import { IconProps } from "@cloudscape-design/components/icon";
import LineChart from "@cloudscape-design/components/line-chart";
import { MixedLineBarChartProps } from "@cloudscape-design/components/mixed-line-bar-chart";

import { useMqttSubscription, useMqttState } from "./mqtt";

var api_pickers = new Set<(state: boolean) => void>();

export function ApiPickerButton() {
  const [active, setActive] = useState(false);

  useEffect(() => {
    api_pickers.add(setActive);

    return () => {
      api_pickers.delete(setActive);
    };
  }, []);

  return (
    <Button
      onClick={(_) => api_pickers.forEach((v) => v(!active))}
      formAction="none"
      iconName="search"
    >
      API docs
    </Button>
  );
}

interface ApiPickerProps {
  topic: string;
  children: React.ReactNode;
}

export function ApiPicker(props: ApiPickerProps) {
  const [active, setActive] = useState(false);

  useEffect(() => {
    api_pickers.add(setActive);

    return () => {
      api_pickers.delete(setActive);
    };
  }, []);

  function click(ev: React.MouseEvent<HTMLElement>) {
    if (active) {
      let tab = window.open(`/v1/api_doc/#${props.topic}`, "_blank");
      if (tab !== null) {
        tab.focus();
      }

      api_pickers.forEach((v) => v(false));

      ev.preventDefault();
      ev.stopPropagation();
    }
  }

  let outer_class = active ? "api_picker_outer_active" : "api_picker_outer";
  let inner_class = active ? "api_picker_inner_active" : "api_picker_inner";

  return (
    <div className={outer_class} onClick={click}>
      <div className={inner_class}>{props.children}</div>
    </div>
  );
}

interface MqttBoxProps<T> {
  topic: string;
  variant?: BoxProps.Variant;
  initial?: T;
  format: (msg: T) => string;
}

export function MqttBox<T>(props: MqttBoxProps<T>) {
  const payload = useMqttSubscription<T>(props.topic, props.initial);

  let box = null;

  if (payload === undefined) {
    box = (
      <Box variant={props.variant}>
        <Spinner />
      </Box>
    );
  } else {
    box = <Box variant={props.variant}>{props.format(payload)}</Box>;
  }

  return ApiPicker({ topic: props.topic, children: box });
}

interface MqttToggleConvProps<T> {
  topic: string;
  children: React.ReactNode;
  to_bool: (msg: T) => boolean;
  from_bool: (val: boolean) => T;
}

export function MqttToggleConv<T>(props: MqttToggleConvProps<T>) {
  const [settled, payload, setPayload] = useMqttState<T>(props.topic);

  return (
    <ApiPicker topic={props.topic}>
      <Toggle
        onChange={(ev) => setPayload(props.from_bool(ev.detail.checked))}
        checked={payload === undefined ? false : props.to_bool(payload)}
        disabled={!settled}
      >
        {props.children}
      </Toggle>
    </ApiPicker>
  );
}

interface MqttToggleProps {
  topic: string;
  children: React.ReactNode;
}

export function MqttToggle(props: MqttToggleProps) {
  return MqttToggleConv({
    topic: props.topic,
    children: props.children,
    to_bool: (b: boolean) => b,
    from_bool: (b: boolean) => b,
  });
}

interface MqttButtonProps<T> {
  topic: string;
  iconName?: IconProps.Name;
  children: React.ReactNode;
  send: T;
}

export function MqttButton<T>(props: MqttButtonProps<T>) {
  // eslint-disable-next-line
  const [_settled, _payload, setPayload] = useMqttState<T>(props.topic);

  return (
    <ApiPicker topic={props.topic}>
      <Button
        formAction="none"
        iconName={props.iconName}
        onClick={() => setPayload(props.send)}
      >
        {props.children}
      </Button>
    </ApiPicker>
  );
}

interface MqttBarMeterProps<T> {
  topic: string;
  description: ((obj: T) => string) | string;
  label: ((obj: T) => string) | string;
  to_percent: (obj: T) => number;
  additionalInfo?: string;
}

export function MqttBarMeter<T>(props: MqttBarMeterProps<T>) {
  const payload = useMqttSubscription<T>(props.topic);

  let valid = true;
  let value = 0.0;
  let description =
    typeof props.description === "string" ? props.description : "";
  let label = typeof props.label === "string" ? props.label : "";

  if (payload === undefined) {
    valid = false;
  } else {
    value = props.to_percent(payload);

    if (typeof props.description === "function") {
      description = props.description(payload);
    }

    if (typeof props.label === "function") {
      label = props.label(payload);
    }
  }

  return (
    <ApiPicker topic={props.topic}>
      <ProgressBar
        status={valid ? "in-progress" : "error"}
        value={value}
        description={description}
        additionalInfo={props.additionalInfo}
        label={label}
      />
    </ApiPicker>
  );
}

type Measurement = {
  ts: number;
  value: number;
};

interface MqttChartProps {
  topic: string;
}

export function MqttChart(props: MqttChartProps) {
  const payload = useMqttSubscription<Measurement>(props.topic);
  const [history, setHistory] = useState<Array<{ x: Date; y: number }>>([]);

  useEffect(() => {
    if (payload === undefined) {
      return;
    }

    let elem = {
      x: new Date(payload.ts),
      y: payload.value,
    };

    if (history.length > 0 && elem.x === history[history.length - 1].x) {
      return;
    }

    let new_history = new Array(200);

    let oi = history.length - 1;
    let ni = new_history.length - 1;

    new_history[ni--] = elem;

    while (oi >= 0 && ni >= 0) {
      new_history[ni--] = history[oi--];
    }

    while (ni >= 0) {
      new_history[ni--] = elem;
    }

    setHistory(new_history);
    // eslint-disable-next-line
  }, [payload]);

  let end = history.length >= 1 ? history[history.length - 1]["x"] : new Date();

  let series: MixedLineBarChartProps.ChartSeries<Date> = {
    type: "line",
    title: "eh",
    data: history,
  };

  return (
    <ApiPicker topic={props.topic}>
      <LineChart
        series={[series]}
        visibleSeries={[series]}
        xScaleType="time"
        i18nStrings={{
          xTickFormatter: (e) =>
            ((Number(e) - Number(end)) / 1000).toFixed(1) + "s",
        }}
        height={200}
        hideFilter
        hideLegend
      />
    </ApiPicker>
  );
}
