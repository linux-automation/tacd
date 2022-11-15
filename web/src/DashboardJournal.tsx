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

import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import "xterm/css/xterm.css";

import Header from "@cloudscape-design/components/header";
import SpaceBetween from "@cloudscape-design/components/space-between";

import { useEffect, useRef } from "react";

interface JournalViewProps {
  history_len: number;
  rows: number;
  unit?: string;
}

export function JournalView(props: JournalViewProps) {
  const terminal_div = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let div = terminal_div.current;

    let terminal = new Terminal({ rows: props.rows });
    let fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);

    let url = `/v1/tac/journal?history_len=${props.history_len}`;

    if (props.unit !== undefined) {
      url = url + `&unit=${props.unit}`;
    }

    let es = new EventSource(url);

    es.addEventListener("entry", (ev) => {
      let entry = JSON.parse(ev.data);

      let ts = "-";

      if (entry["_SOURCE_REALTIME_TIMESTAMP"] !== undefined) {
        let ts_us = Number(entry["_SOURCE_REALTIME_TIMESTAMP"]);
        let d = new Date(ts_us / 1000);

        let month = [
          "Jan",
          "Feb",
          "Mar",
          "Apr",
          "May",
          "Jun",
          "Jul",
          "Aug",
          "Sep",
          "Oct",
          "Nov",
          "Dec",
        ][d.getMonth()];

        let hour = (d.getHours() + 100).toFixed(0).slice(1);
        let minute = (d.getMinutes() + 100).toFixed(0).slice(1);
        let second = (d.getSeconds() + 100).toFixed(0).slice(1);

        ts = `${month} ${d.getDate()} ${hour}:${minute}:${second}`;
      }

      if (entry["SYSLOG_TIMESTAMP"] !== undefined) {
        ts = entry["SYSLOG_TIMESTAMP"];
      }

      ts = ts.padEnd(15).slice(0, 15);

      let unit =
        entry["UNIT"] ||
        entry["_SYSTEMD_UNIT"] ||
        entry["SYSLOG_IDENTIFIER"] ||
        "-";
      let msg = entry["MESSAGE"] || "-";

      unit = unit.padEnd(16).slice(0, 16);

      terminal.writeln(`${ts} | ${unit} | ${msg}`);
    });

    function on_resize() {
      fitAddon.fit();
    }

    if (div !== null) {
      terminal.open(div);
      on_resize();
    }

    window.addEventListener("onresize", on_resize);

    return () => {
      es.close();
      window.removeEventListener("onresize", on_resize);

      if (div !== null) {
        div.innerText = "";
      }
    };
  }, [props.history_len, props.unit, props.rows]);

  return <div className="terminal_wrap" ref={terminal_div} />;
}

export default function DashboardJournal() {
  return (
    <SpaceBetween size="m">
      <Header variant="h1" description="Watch the Systemd Journal">
        LXA TAC / Systemd Journal
      </Header>

      <JournalView history_len={30} rows={50} />
    </SpaceBetween>
  );
}
