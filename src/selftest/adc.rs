use std::time::Duration;

use anyhow::{Error, Result};
use async_std::task::sleep;
use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::adc::Adc;
use crate::broker::BrokerBuilder;
use crate::measurement::Measurement;
use crate::system::HardwareGeneration;
use crate::watched_tasks::WatchedTasksBuilder;

#[derive(ValueEnum, Copy, Clone, Debug)]
pub enum ADCChannels {
    UsbHostCurr,
    UsbHost1Curr,
    UsbHost2Curr,
    UsbHost3Curr,
    Out0Volt,
    Out1Volt,
    IoBusVolt,
    IoBusCurr,
}

#[derive(Args, Debug)]
pub struct AdcArgs {
    /// ADC channels to sample
    #[arg(short, long, required = true)]
    pub inputs: Vec<ADCChannels>,

    /// Number of samples to collect
    #[arg(short, long, default_value_t = 100)]
    pub samples: u32,

    /// Time between sample in ms
    #[arg(short, long, default_value_t = 100)]
    pub t_delta: u64,

    /// Outputs every sample as they get collected.
    #[arg(short, default_value_t = false)]
    pub verbose: bool,

    /// Add samples to output
    #[arg(short, default_value_t = false)]
    pub output_samples: bool,
}

/// The data we collect for an ADC channel
struct AdcData {
    channel_name: String,
    samples: Vec<Measurement>,
}

#[derive(Debug, Serialize)]
struct AdcStatistic {
    pub min: i32,
    pub max: i32,
    pub mean: i32,
    pub std_dev: f32,
    pub count: usize,
    pub t_delta: u64,
}

#[derive(Debug, Serialize)]
struct AdcInfo<'a> {
    channel_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    samples: Option<&'a Vec<Measurement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    statistic: Option<AdcStatistic>,
}

impl AdcData {
    fn statistics(&self, t_delta: u64) -> AdcStatistic {
        let count = self.samples.len();
        let sum: i32 = self.samples.iter().map(|x| x.raw.unwrap()).sum();
        let mean = sum / count as i32;

        let variance_sum = self
            .samples
            .iter()
            .map(|x| (x.raw.unwrap() - mean).pow(2))
            .sum::<i32>();

        let variance = variance_sum as f32 / count as f32;

        let std_dev = variance.sqrt();

        let min = self.samples.iter().map(|x| x.raw.unwrap()).min().unwrap();
        let max = self.samples.iter().map(|x| x.raw.unwrap()).max().unwrap();

        AdcStatistic {
            min,
            max,
            mean,
            std_dev,
            count,
            t_delta,
        }
    }
}

pub async fn collect_adc_samples(
    mut bb: BrokerBuilder,
    mut wtb: WatchedTasksBuilder,
    hardware_generation: HardwareGeneration,
    args: AdcArgs,
) -> Result<()> {
    let adc = Adc::new(&mut bb, &mut wtb, hardware_generation).await?;

    if args.verbose {
        println!("ADC mode");
        println!("  Samples: {}", args.samples);
        println!("  Time delta: {}", args.t_delta);
        println!("  Inputs: {:?}", args.inputs);
    }

    let num_samples = args.samples;

    // Setup to collect ADC samples
    let mut samples: Vec<AdcData> = Vec::new();
    for adc_ch in args.inputs.iter() {
        samples.push(AdcData {
            channel_name: adc_ch.to_possible_value().unwrap().get_name().to_string(),
            samples: Vec::new(),
        });
    }

    let mut start = None;

    // Collect samples from all selected channels interleaved
    for _i in 0..num_samples {
        for (i, adc_ch) in args.inputs.iter().enumerate() {
            let adc_fn = match adc_ch {
                ADCChannels::UsbHostCurr => &adc.usb_host_curr.fast,
                ADCChannels::UsbHost1Curr => &adc.usb_host1_curr.fast,
                ADCChannels::UsbHost2Curr => &adc.usb_host2_curr.fast,
                ADCChannels::UsbHost3Curr => &adc.usb_host3_curr.fast,
                ADCChannels::Out0Volt => &adc.out0_volt.fast,
                ADCChannels::Out1Volt => &adc.out1_volt.fast,
                ADCChannels::IoBusVolt => &adc.iobus_volt.fast,
                ADCChannels::IoBusCurr => &adc.iobus_curr.fast,
            };
            let meas = adc_fn.get().map_err(|_| Error::msg("Adc Error"))?;

            if args.verbose {
                let ts = meas.ts.as_instant();
                let ms_since_start = ts.duration_since(*start.get_or_insert(ts)).as_millis();

                println!(
                    "  {:15} | {:10}ms | {:15}A | {:10}",
                    adc_ch.to_possible_value().unwrap().get_name(),
                    ms_since_start,
                    meas.value,
                    meas.raw.unwrap_or(0)
                );
            }
            samples[i].samples.push(meas);
        }
        sleep(Duration::from_millis(args.t_delta)).await;
    }

    // Build JSON output
    let mut infos: Vec<AdcInfo> = Vec::new();
    for i in samples.iter() {
        let stat = i.statistics(args.t_delta);

        let info = AdcInfo {
            channel_name: i.channel_name.to_string(),
            samples: if args.output_samples {
                Some(&i.samples)
            } else {
                None
            },
            statistic: Some(stat),
        };

        infos.push(info);
    }
    let json = serde_json::to_string_pretty(&infos).unwrap();
    println!("{}", json);
    Ok(())
}
