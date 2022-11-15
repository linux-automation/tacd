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

interface MqttBoxProps<T> {
  topic: string;
  variant?: BoxProps.Variant;
  initial?: T;
  format: (msg: T) => string;
}

export function MqttBox<T>(props: MqttBoxProps<T>) {
  const payload = useMqttSubscription<T>(props.topic, props.initial);

  if (payload === undefined) {
    return (
      <Box variant={props.variant}>
        <Spinner />
      </Box>
    );
  } else {
    return <Box variant={props.variant}>{props.format(payload)}</Box>;
  }
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
    <Toggle
      onChange={(ev) => setPayload(props.from_bool(ev.detail.checked))}
      checked={payload === undefined ? false : props.to_bool(payload)}
      disabled={!settled}
    >
      {props.children}
    </Toggle>
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
  const [_settled, _payload, setPayload] = useMqttState<T>(props.topic);

  return (
    <Button
      formAction="none"
      iconName={props.iconName}
      onClick={() => setPayload(props.send)}
    >
      {props.children}
    </Button>
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
    <ProgressBar
      status={valid ? "in-progress" : "error"}
      value={value}
      description={description}
      additionalInfo={props.additionalInfo}
      label={label}
    />
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
  );
}
