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

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use async_std::channel::bounded;
use async_std::prelude::*;
use async_std::sync::{Arc, Weak};
use async_std::task;
use serde::{Deserialize, Serialize};

use crate::adc::AdcChannel;
use crate::broker::{BrokerBuilder, Topic};
use crate::digital_io::{find_line, LineHandle, LineRequestFlags};
use crate::led::{BlinkPattern, BlinkPatternBuilder};
use crate::watched_tasks::WatchedTasksBuilder;

#[cfg(any(test, feature = "demo_mode"))]
mod prio {
    use anyhow::Result;

    pub fn realtime_priority() -> Result<()> {
        Ok(())
    }
}

#[cfg(not(any(test, feature = "demo_mode")))]
mod prio {
    use std::convert::TryFrom;

    use anyhow::{anyhow, Result};
    use thread_priority::*;

    pub fn realtime_priority() -> Result<()> {
        set_thread_priority_and_policy(
            thread_native_id(),
            ThreadPriority::Crossplatform(ThreadPriorityValue::try_from(10).unwrap()),
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
        )
        .map_err(|e| anyhow!("Failed to set up realtime priority {e:?}"))
    }
}

use prio::realtime_priority;

const MAX_AGE: Duration = Duration::from_millis(300);
const THREAD_INTERVAL: Duration = Duration::from_millis(100);
const TASK_INTERVAL: Duration = Duration::from_millis(200);
const MAX_CURRENT: f32 = 5.0;
const MAX_VOLTAGE: f32 = 48.0;
const MIN_VOLTAGE: f32 = -1.0;

const PWR_LINE_ASSERTED: u8 = 0;
const DISCHARGE_LINE_ASSERTED: u8 = 0;

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum OutputRequest {
    Idle,
    On,
    Off,
    OffFloating,
}

impl From<u8> for OutputRequest {
    fn from(val: u8) -> Self {
        if val == (OutputRequest::Idle as u8) {
            return OutputRequest::Idle;
        }

        if val == (OutputRequest::On as u8) {
            return OutputRequest::On;
        }

        if val == (OutputRequest::Off as u8) {
            return OutputRequest::Off;
        }

        if val == (OutputRequest::OffFloating as u8) {
            return OutputRequest::OffFloating;
        }

        panic!()
    }
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize, Debug)]
pub enum OutputState {
    On,
    Off,
    OffFloating,
    Changing,
    InvertedPolarity,
    OverCurrent,
    OverVoltage,
    RealtimeViolation,
}

impl From<u8> for OutputState {
    fn from(val: u8) -> Self {
        if val == (OutputState::On as u8) {
            return OutputState::On;
        }

        if val == (OutputState::Off as u8) {
            return OutputState::Off;
        }

        if val == (OutputState::OffFloating as u8) {
            return OutputState::OffFloating;
        }

        if val == (OutputState::Changing as u8) {
            return OutputState::Changing;
        }

        if val == (OutputState::InvertedPolarity as u8) {
            return OutputState::InvertedPolarity;
        }

        if val == (OutputState::OverCurrent as u8) {
            return OutputState::OverCurrent;
        }

        if val == (OutputState::OverVoltage as u8) {
            return OutputState::OverVoltage;
        }

        if val == (OutputState::RealtimeViolation as u8) {
            return OutputState::RealtimeViolation;
        }

        panic!()
    }
}

pub struct TickReader {
    src: Weak<AtomicU32>,
    val: u32,
}

impl TickReader {
    pub fn new(src: &Arc<AtomicU32>) -> Self {
        Self {
            src: Arc::downgrade(src),
            val: src.load(Ordering::Relaxed),
        }
    }

    /// Check if the corresponding power thread is still doing fine
    ///
    /// This function checks if at least some progress was made in the
    /// power thread between the last call to is_stale() and the current
    /// call.
    /// Ensuring that is_stale() is not called too frequently is up to the
    /// user.
    pub fn is_stale(&mut self) -> bool {
        if let Some(tick) = self.src.upgrade() {
            let prev = self.val;
            self.val = tick.load(Ordering::Relaxed);

            prev == self.val
        } else {
            true
        }
    }
}

pub struct DutPwrThread {
    pub request: Arc<Topic<OutputRequest>>,
    pub state: Arc<Topic<OutputState>>,
    tick: Arc<AtomicU32>,
}

struct MedianFilter<const N: usize> {
    history: [f32; N],
    index: usize,
    ready: bool,
}

impl<const N: usize> MedianFilter<N> {
    pub fn new() -> Self {
        Self {
            history: [f32::NAN; N],
            index: 0,
            ready: false,
        }
    }

    /// Return the median of the N last values added or None if less than N
    /// values were stepped in yet.
    ///
    /// Returns the mean of the two center most entries if N is even.
    pub fn step(&mut self, val: f32) -> Option<f32> {
        self.history[self.index] = val;
        self.index = (self.index + 1) % N;
        self.ready |= self.index == 0;

        if self.ready {
            let sorted = {
                let mut sorted = [0.0; N];
                sorted.clone_from_slice(&self.history);
                sorted.sort_unstable_by(f32::total_cmp);
                sorted
            };

            if N % 2 == 0 {
                Some((sorted[N / 2 - 1] + sorted[N / 2]) / 2.0)
            } else {
                Some(sorted[N / 2])
            }
        } else {
            None
        }
    }
}

/// Turn the output off and set an appropriate reason
fn turn_off_with_reason(
    reason: OutputState,
    pwr_line: &LineHandle,
    discharge_line: &LineHandle,
    fail_state: &AtomicU8,
) {
    pwr_line.set_value(1 - PWR_LINE_ASSERTED).unwrap();
    discharge_line.set_value(DISCHARGE_LINE_ASSERTED).unwrap();
    fail_state.store(reason as u8, Ordering::Relaxed);
}

/// Labgrid has a fixed assumption of how a REST based power port should work.
/// It should consume "1" and "0" as PUT request bodies and return "1" or not
/// "1" as GET response bodies.
/// Provide a compat interface that provides this behaviour while keeping the
/// main interface used by e.g. the web UI pretty.
fn setup_labgrid_compat(
    bb: &mut BrokerBuilder,
    wtb: &mut WatchedTasksBuilder,
    request: Arc<Topic<OutputRequest>>,
    state: Arc<Topic<OutputState>>,
) {
    let compat_request = bb.topic_wo::<u8>("/v1/dut/powered/compat", None);
    let compat_response = bb.topic_ro::<u8>("/v1/dut/powered/compat", None);

    let (mut state_stream, _) = state.subscribe_unbounded();
    let (mut compat_request_stream, _) = compat_request.subscribe_unbounded();

    wtb.spawn_task("power-compat-from-labgrid", async move {
        while let Some(req) = compat_request_stream.next().await {
            match req {
                0 => request.set(OutputRequest::Off),
                1 => request.set(OutputRequest::On),
                _ => {}
            }
        }

        Ok(())
    });

    wtb.spawn_task("power-compat-to-labgrid", async move {
        while let Some(state) = state_stream.next().await {
            match state {
                OutputState::On => compat_response.set(1),
                OutputState::Changing => {}
                _ => compat_response.set(0),
            }
        }

        Ok(())
    });
}

impl DutPwrThread {
    pub async fn new(
        bb: &mut BrokerBuilder,
        wtb: &mut WatchedTasksBuilder,
        pwr_volt: AdcChannel,
        pwr_curr: AdcChannel,
        pwr_led: Arc<Topic<BlinkPattern>>,
    ) -> Result<Self> {
        let pwr_line = find_line("DUT_PWR_EN")
            .ok_or_else(|| anyhow!("Could not find GPIO line DUT_PWR_EN"))?;
        let discharge_line = find_line("DUT_PWR_DISCH")
            .ok_or_else(|| anyhow!("Could not find GPIO line DUT_PWR_DISCH"))?;

        // Early TACs have a i2c port expander on the power board.
        // This port expander is output only and can not be configured as
        // open drain.
        // The outputs on later TACs should however be driven open drain
        // for EMI reasons.
        let flags = match pwr_line.chip().label() {
            "pca9570" => LineRequestFlags::OUTPUT,
            _ => LineRequestFlags::OUTPUT | LineRequestFlags::OPEN_DRAIN,
        };

        let pwr_line = pwr_line.request(flags, 1 - PWR_LINE_ASSERTED, "tacd")?;
        let discharge_line = discharge_line.request(flags, DISCHARGE_LINE_ASSERTED, "tacd")?;

        // The realtime priority must be set up inside the tread, but
        // the operation may fail, in which case we want new() to fail
        // as well.
        // Use a queue to notify the calling thread if the priority setup
        // succeeded.
        let (thread_res_tx, mut thread_res_rx) = bounded(1);

        // Spawn a high priority thread that handles the power status
        // in a realtimey fashion.
        thread::Builder::new()
            .name("tacd power".into())
            .spawn(move || {
                let mut last_ts: Option<Instant> = None;

                // There may be transients in the measured voltage/current, e.g. due to EMI or
                // inrush currents.
                // Nothing will break if they are sufficiently short, so the DUT can stay powered.
                // Filter out transients by taking the last four values, throwing away the largest
                // and smallest and averaging the two remaining ones.
                let mut volt_filter = MedianFilter::<4>::new();
                let mut curr_filter = MedianFilter::<4>::new();

                let (tick_weak, request, state) = match realtime_priority() {
                    Ok(_) => {
                        let tick = Arc::new(AtomicU32::new(0));
                        let tick_weak = Arc::downgrade(&tick);

                        let request = Arc::new(AtomicU8::new(OutputRequest::Idle as u8));
                        let state = Arc::new(AtomicU8::new(OutputState::Off as u8));

                        thread_res_tx
                            .try_send(Ok((tick, request.clone(), state.clone())))
                            .unwrap();

                        (tick_weak, request, state)
                    }
                    Err(e) => {
                        thread_res_tx.try_send(Err(e)).unwrap();
                        panic!()
                    }
                };

                // Run as long as there is a strong reference to `tick`.
                // As tick is a private member of the struct this is equivalent
                // to running as long as the DutPwrThread was not dropped.
                while let Some(tick) = tick_weak.upgrade() {
                    thread::sleep(THREAD_INTERVAL);

                    // Get new voltage and current readings while making sure
                    // that they are not stale
                    let (volt, curr) = loop {
                        let feedback = pwr_volt
                            .fast
                            .try_get_multiple([&pwr_volt.fast, &pwr_curr.fast]);

                        // We do not care too much about _why_ we could not get
                        // a new value from the ADC.
                        // If we get a new valid value before the timeout we
                        // are fine.
                        // If not we are not.
                        if let Ok(m) = feedback {
                            last_ts = Some(m[0].ts.as_instant());
                        }

                        let too_old = last_ts
                            .map(|ts| Instant::now().duration_since(ts) > MAX_AGE)
                            .unwrap_or(false);

                        if too_old {
                            turn_off_with_reason(
                                OutputState::RealtimeViolation,
                                &pwr_line,
                                &discharge_line,
                                &state,
                            );
                        } else {
                            // We have a fresh ADC value. Signal "everything is well"
                            // to the watchdog task.
                            tick.fetch_add(1, Ordering::Relaxed);
                        }

                        if let Ok(m) = feedback {
                            break (m[0].value, m[1].value);
                        }
                    };

                    // The median filter needs some values in it's backlog before it
                    // starts outputting values.
                    let (volt, curr) = match (volt_filter.step(volt), curr_filter.step(curr)) {
                        (Some(volt), Some(curr)) => (volt, curr),
                        _ => continue,
                    };

                    // Take the next pending OutputRequest (if any) even if it
                    // may not be used due to a pending error condition, as it
                    // could be quite surprising for the output to turn on
                    // immediately when a fault is cleared after quite some time
                    // of the output being off.
                    let req = request
                        .swap(OutputRequest::Idle as u8, Ordering::Relaxed)
                        .into();

                    // Don't even look at the requests if there is an ongoing
                    // overvoltage condition. Instead turn the output off and
                    // go back to measuring.
                    if volt > MAX_VOLTAGE {
                        turn_off_with_reason(
                            OutputState::OverVoltage,
                            &pwr_line,
                            &discharge_line,
                            &state,
                        );

                        continue;
                    }

                    // Don't even look at the requests if there is an ongoin
                    // polarity inversion. Turn off, go back to start, do not
                    // collect $200.
                    if volt < MIN_VOLTAGE {
                        turn_off_with_reason(
                            OutputState::InvertedPolarity,
                            &pwr_line,
                            &discharge_line,
                            &state,
                        );

                        continue;
                    }

                    // Don't even look at the requests if there is an ongoin
                    // overcurrent condition.
                    if curr > MAX_CURRENT {
                        turn_off_with_reason(
                            OutputState::OverCurrent,
                            &pwr_line,
                            &discharge_line,
                            &state,
                        );

                        continue;
                    }

                    // There is no ongoing fault condition, so we could e.g. turn
                    // the output on if requested.
                    match req {
                        OutputRequest::Idle => {}
                        OutputRequest::On => {
                            discharge_line
                                .set_value(1 - DISCHARGE_LINE_ASSERTED)
                                .unwrap();
                            pwr_line.set_value(PWR_LINE_ASSERTED).unwrap();
                            state.store(OutputState::On as u8, Ordering::Relaxed);
                        }
                        OutputRequest::Off => {
                            discharge_line.set_value(DISCHARGE_LINE_ASSERTED).unwrap();
                            pwr_line.set_value(1 - PWR_LINE_ASSERTED).unwrap();
                            state.store(OutputState::Off as u8, Ordering::Relaxed);
                        }
                        OutputRequest::OffFloating => {
                            discharge_line
                                .set_value(1 - DISCHARGE_LINE_ASSERTED)
                                .unwrap();
                            pwr_line.set_value(1 - PWR_LINE_ASSERTED).unwrap();
                            state.store(OutputState::OffFloating as u8, Ordering::Relaxed);
                        }
                    }
                }

                // Make sure to enter fail safe mode before leaving the thread
                turn_off_with_reason(OutputState::Off, &pwr_line, &discharge_line, &state);
            })?;

        let (tick, request, state) = thread_res_rx.next().await.unwrap()?;

        // The request and state topic use the same external path, this way one
        // can e.g. publish "On" to the topic and be sure that the output is
        // actually on once a corresponding publish is received from the broker,
        // as it has done the full round trip through the realtime power thread
        // and is not just a copy of the received command.
        let request_topic = bb.topic_wo::<OutputRequest>("/v1/dut/powered", None);
        let state_topic = bb.topic_ro::<OutputState>("/v1/dut/powered", None);

        setup_labgrid_compat(bb, wtb, request_topic.clone(), state_topic.clone());

        // Requests come from the broker framework and are placed into an atomic
        // request variable read by the thread.
        let state_topic_task = state_topic.clone();
        let (mut request_stream, _) = request_topic.clone().subscribe_unbounded();
        wtb.spawn_task("power-from-broker", async move {
            while let Some(req) = request_stream.next().await {
                state_topic_task.set(OutputState::Changing);
                request.store(req as u8, Ordering::Relaxed);
            }

            Ok(())
        });

        // State information comes from the thread in the form of an atomic
        // variable and is forwarded to the broker framework.
        let state_topic_task = state_topic.clone();
        wtb.spawn_task("power-to-broker", async move {
            loop {
                task::sleep(TASK_INTERVAL).await;

                let curr_state = state.load(Ordering::Relaxed).into();
                state_topic_task.set_if_changed(curr_state);
            }
        });

        // Forward the state information to the DUT Power LED
        let (mut state_stream, _) = state_topic.clone().subscribe_unbounded();
        wtb.spawn_task("power-to-led", async move {
            let pattern_on = BlinkPattern::solid(1.0);
            let pattern_off = BlinkPattern::solid(0.0);
            let pattern_error = {
                let mut pb = BlinkPatternBuilder::new(1.0);

                // Three angry blinks ...
                for _ in 0..3 {
                    pb = pb
                        .step_to(1.0)
                        .stay_for(Duration::from_millis(50))
                        .step_to(0.0)
                        .stay_for(Duration::from_millis(50));
                }

                // ... followed by a pause and repetition
                pb.stay_for(Duration::from_millis(400)).forever()
            };

            while let Some(state) = state_stream.next().await {
                match state {
                    OutputState::On => pwr_led.set(pattern_on.clone()),
                    OutputState::Off | OutputState::OffFloating => pwr_led.set(pattern_off.clone()),
                    OutputState::Changing => {}
                    _ => pwr_led.set(pattern_error.clone()),
                }
            }

            Ok(())
        });

        Ok(Self {
            request: request_topic,
            state: state_topic,
            tick,
        })
    }

    pub fn tick(&self) -> TickReader {
        TickReader::new(&self.tick)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_std::task::{block_on, sleep};

    use crate::adc::Adc;
    use crate::broker::{BrokerBuilder, Topic};
    use crate::digital_io::find_line;
    use crate::watched_tasks::WatchedTasksBuilder;

    use super::{
        DutPwrThread, OutputRequest, OutputState, DISCHARGE_LINE_ASSERTED, MAX_CURRENT,
        MAX_VOLTAGE, MIN_VOLTAGE, PWR_LINE_ASSERTED,
    };

    #[test]
    fn failsafe() {
        let mut wtb = WatchedTasksBuilder::new();
        let pwr_line = find_line("DUT_PWR_EN").unwrap();
        let discharge_line = find_line("DUT_PWR_DISCH").unwrap();

        let (adc, dut_pwr, led) = {
            let mut bb = BrokerBuilder::new();
            let adc = block_on(Adc::new(&mut bb, &mut wtb)).unwrap();
            let led = Topic::anonymous(None);

            let dut_pwr = block_on(DutPwrThread::new(
                &mut bb,
                &mut wtb,
                adc.pwr_volt.clone(),
                adc.pwr_curr.clone(),
                led.clone(),
            ))
            .unwrap();

            (adc, dut_pwr, led)
        };

        println!("Test with acceptable voltage/current");

        // Set acceptable voltage / current
        adc.pwr_volt.fast.set(MAX_VOLTAGE * 0.99);
        adc.pwr_curr.fast.set(MAX_CURRENT * 0.99);

        block_on(sleep(Duration::from_millis(500)));

        // Make sure that the DUT power is off by default
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::Off);
        assert!(block_on(led.get()).is_off());

        println!("Turn Off Floating");
        dut_pwr.request.set(OutputRequest::OffFloating);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::OffFloating);
        assert!(block_on(led.get()).is_off());

        println!("Turn on");
        dut_pwr.request.set(OutputRequest::On);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger transient inverted polarity (Output should stay on)");
        adc.pwr_volt.fast.transient(MIN_VOLTAGE * 1.01);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger inverted polarity");
        adc.pwr_volt.fast.set(MIN_VOLTAGE * 1.01);
        block_on(sleep(Duration::from_millis(500)));
        adc.pwr_volt.fast.set(MIN_VOLTAGE * 0.99);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::InvertedPolarity);
        assert!(block_on(led.get()).is_blinking());

        println!("Turn on again");
        dut_pwr.request.set(OutputRequest::On);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger transient overcurrent (Output should stay on)");
        adc.pwr_curr.fast.transient(MAX_CURRENT * 1.01);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger overcurrent");
        adc.pwr_curr.fast.set(MAX_CURRENT * 1.01);
        block_on(sleep(Duration::from_millis(500)));
        adc.pwr_curr.fast.set(MAX_CURRENT * 0.99);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::OverCurrent);
        assert!(block_on(led.get()).is_blinking());

        println!("Turn on again");
        dut_pwr.request.set(OutputRequest::On);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger transient overvoltage (Output should stay on)");
        adc.pwr_volt.fast.transient(MAX_VOLTAGE * 1.01);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger overvoltage");
        adc.pwr_volt.fast.set(MAX_VOLTAGE * 1.01);
        block_on(sleep(Duration::from_millis(500)));
        adc.pwr_volt.fast.set(MAX_VOLTAGE * 0.99);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::OverVoltage);
        assert!(block_on(led.get()).is_blinking());

        println!("Turn on again");
        dut_pwr.request.set(OutputRequest::On);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Trigger realtime violation");
        adc.pwr_volt.fast.stall(true);
        block_on(sleep(Duration::from_millis(500)));
        adc.pwr_volt.fast.stall(false);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), DISCHARGE_LINE_ASSERTED);
        assert_eq!(
            block_on(dut_pwr.state.get()),
            OutputState::RealtimeViolation
        );
        assert!(block_on(led.get()).is_blinking());

        println!("Turn on again");
        dut_pwr.request.set(OutputRequest::On);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), 1 - DISCHARGE_LINE_ASSERTED);
        assert_eq!(block_on(dut_pwr.state.get()), OutputState::On);
        assert!(block_on(led.get()).is_on());

        println!("Drop DutPwrThread");
        std::mem::drop(dut_pwr);
        block_on(sleep(Duration::from_millis(500)));
        assert_eq!(pwr_line.stub_get(), 1 - PWR_LINE_ASSERTED);
        assert_eq!(discharge_line.stub_get(), DISCHARGE_LINE_ASSERTED);
    }
}
