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

use std::env::var_os;
use std::fs::{read_to_string, write, File};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use chrono::prelude::Utc;
use png::{BitDepth, ColorType, Decoder};

fn generate_openapi_include() {
    let cargo_dir = {
        let dir = var_os("CARGO_MANIFEST_DIR").unwrap();
        Path::new(&dir).to_path_buf()
    };

    let out_dir = {
        let dir = var_os("OUT_DIR").unwrap();
        Path::new(&dir).to_path_buf()
    };

    println!("cargo:rerun-if-changed=openapi.yaml");
    let openapi_json = {
        let yaml = read_to_string(cargo_dir.join("openapi.yaml")).unwrap();
        let json: serde_json::Value = serde_yaml::from_str(&yaml).unwrap();
        serde_json::to_vec(&json).unwrap()
    };

    let openapi_file = out_dir.join("openapi.json");
    write(openapi_file, openapi_json).unwrap();
}

/// Generates a version string
/// `version: 0.1.0 b9ff258-dirty @ 2019-11-05 14:13:49`
fn generate_version_string() {
    let dir = var_os("CARGO_MANIFEST_DIR").unwrap();

    let git_hash = Command::new("git")
        .arg("describe")
        .arg("--always")
        .arg("--dirty=-dirty")
        .current_dir(&dir)
        .output()
        .expect("Could not exec 'git describe'");

    assert!(
        git_hash.status.success(),
        "Could no get git commit hash. Maybe no git repo or first commit?"
    );

    let git_hash_str = String::from_utf8_lossy(&git_hash.stdout)
        .trim_end()
        .to_string();

    let rustc_version = Command::new("rustc")
        .arg("-V")
        .current_dir(&dir)
        .output()
        .expect("Could not exec 'rustc -V'");

    assert!(rustc_version.status.success(), "rustc -V failed? how?");

    let rustc_version_str = String::from_utf8_lossy(&rustc_version.stdout)
        .trim_end()
        .to_string();

    println!(
        "cargo:rustc-env=VERSION_STRING={} {} ({} @ {}) with {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        git_hash_str,
        Utc::now().format("%Y-%m-%d %T"),
        rustc_version_str
    )
}

/// Store the build date and time to have a lower bound on HTTP Last-Modified
/// for files with faked timestamps.
fn generate_build_date() {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);
}

fn decode_png(path: &Path) -> Vec<(u8, u8, u8)> {
    let mut reader = Decoder::new(File::open(path).unwrap()).read_info().unwrap();

    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();

    let width = info.width as usize;
    let height = info.height as usize;

    let bytes_per_pixel = match (info.bit_depth, info.color_type) {
        (BitDepth::Eight, ColorType::Rgb) => 3,
        (BitDepth::Eight, ColorType::Rgba) => 4,
        _ => unimplemented!(),
    };

    let mut pixels = vec![(0, 0, 0); width * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;

            pixels[idx] = (
                buf[idx * bytes_per_pixel],
                buf[idx * bytes_per_pixel + 1],
                buf[idx * bytes_per_pixel + 2],
            );
        }
    }

    pixels
}

fn generate_background() {
    let cargo_dir = {
        let dir = var_os("CARGO_MANIFEST_DIR").unwrap();
        Path::new(&dir).to_path_buf()
    };

    let out_dir = {
        let dir = var_os("OUT_DIR").unwrap();
        Path::new(&dir).to_path_buf()
    };

    let mut output = {
        let output_path = out_dir.join("background.rs");
        File::create(output_path).unwrap()
    };

    println!("cargo:rerun-if-changed=assets/background.png");

    let pixels = decode_png(&cargo_dir.join("assets/background.png"));

    writeln!(&mut output, "&[").unwrap();

    for (r, g, b) in pixels {
        writeln!(&mut output, "    ({}, {}, {}),", r, g, b).unwrap();
    }

    writeln!(&mut output, "]").unwrap();
}

fn main() {
    generate_openapi_include();
    generate_version_string();
    generate_build_date();
    generate_background();
}
