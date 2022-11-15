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

use std::collections::hash_map::DefaultHasher;
use std::env::var_os;
use std::fs::{create_dir, read_dir, read_to_string, remove_dir_all, write, File};
use std::hash::Hasher;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::process::Command;
use std::str::FromStr;

use chrono::prelude::Utc;
use flate2::{read::GzEncoder, Compression};

const ENTRY_TEMPLATE: &str = r#"
    server.at("MOUNT_POINT").get(|req: Request<()>| async move {
        response(
            &req,
            &"\"FILE_ETAG\"",
            &"CONTENT_TYPE",
            &include_bytes!("FILE_PATH")[..]
        )
    });
"#;

fn gzip(dst_dir: &Path, entry: &Path) -> (PathBuf, String) {
    let file_name_src = entry.file_name().unwrap();
    let file_name_dst = {
        let mut name = file_name_src.to_os_string();
        name.push(".gz");
        PathBuf::from(name)
    };
    let path_dst = dst_dir.join(&file_name_dst);

    let mut compressed = Vec::new();
    GzEncoder::new(File::open(entry).unwrap(), Compression::best())
        .read_to_end(&mut compressed)
        .unwrap();
    write(&path_dst, &compressed).unwrap();

    let hash_str = {
        let mut hash = DefaultHasher::new();
        hash.write(&compressed);
        format!("{:016x}", hash.finish())
    };

    (file_name_dst, hash_str)
}

fn walk_dir_and_compress(
    dst: &Path,
    base: &Path,
    src: &Path,
    files: &mut Vec<(PathBuf, PathBuf, String)>,
) {
    for entry in read_dir(src).unwrap() {
        let entry = &entry.unwrap().path();
        if entry.is_dir() {
            walk_dir_and_compress(dst, base, entry, files)
        } else {
            let (file_name_dst, hash_str) = gzip(dst, entry);

            files.push((
                entry.strip_prefix(base).unwrap().to_path_buf(),
                file_name_dst,
                hash_str,
            ))
        }
    }
}

/// Generate a rust file to include that includes all web interface
/// files in the binary and serves them using the correct mime type
fn generate_web_includes() {
    let web_files = {
        let cargo_dir = var_os("CARGO_MANIFEST_DIR").unwrap();
        let cargo_dir = Path::new(&cargo_dir);

        let web_out_dir = {
            let out_dir = var_os("OUT_DIR").unwrap();
            let web_out_dir = Path::new(&out_dir).join("web");
            let _ = remove_dir_all(&web_out_dir);
            let _ = create_dir(&web_out_dir);
            web_out_dir
        };

        let web_in_dir = match var_os("TACD_WEB_DIR") {
            Some(dir) => Path::new(&dir).to_path_buf(),
            None => {
                println!("cargo:rerun-if-changed=web/build");
                cargo_dir.join("web").join("build")
            }
        };

        let mut files = Vec::new();
        walk_dir_and_compress(&web_out_dir, &web_in_dir, &web_in_dir, &mut files);

        if files.is_empty() {
            eprintln!("Could not find any web interface files.");
            eprintln!("Run npm install . && npm run build");
            eprintln!("In the web directory, unpack a web interface archive there");
            eprintln!("or set TACD_WEB_DIR");
            exit(1);
        }

        // Convert the openapi.yaml to .json, then to .json.gz and then
        // add it to the files to serve
        println!("cargo:rerun-if-changed=openapi.yaml");
        let openapi = read_to_string(cargo_dir.join("openapi.yaml")).unwrap();
        let openapi: serde_json::Value = serde_yaml::from_str(&openapi).unwrap();
        let openapi_json = web_out_dir.join("openapi.json");
        write(&openapi_json, serde_json::to_vec(&openapi).unwrap()).unwrap();
        let (openapi_gz, openapi_hash) = gzip(&web_out_dir, &openapi_json);
        files.push(("/v1/openapi.json".into(), openapi_gz, openapi_hash));

        files
    };

    let mut dst_file = {
        let out_dir = var_os("OUT_DIR").unwrap();
        let path = Path::new(&out_dir).join("static_files.rs.in");
        File::create(path).unwrap()
    };

    dst_file.write(b"{").unwrap();

    for (mount_point, file, hash_str) in web_files {
        let content_type = match mount_point.extension().map(|e| e.to_str()).flatten() {
            Some("css") => "text/css",
            Some("html") => "text/html",
            Some("ico") => "image/vnd.microsoft.icon",
            Some("js") => "text/javascript",
            Some("json") => "application/json",
            Some("map") => "text/plain",
            Some("png") => "image/png",
            Some("svg") => "image/svg+xml",
            Some("txt") => "text/plain",
            _ => panic!("Unkown mime type for {:?}", file),
        };

        let mount_point = mount_point.to_str().unwrap().replace("index.html", "/");
        let file_name = file.to_str().unwrap();

        dst_file
            .write(
                String::from_str(ENTRY_TEMPLATE)
                    .unwrap()
                    .replace("MOUNT_POINT", &mount_point)
                    .replace("FILE_PATH", &format!("web/{file_name}"))
                    .replace("CONTENT_TYPE", content_type)
                    .replace("FILE_ETAG", &hash_str)
                    .as_bytes(),
            )
            .unwrap();
    }

    dst_file.write(b"}").unwrap();
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

fn main() {
    generate_version_string();
    generate_web_includes();
}
