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

use std::cmp::{max, Ordering};
use std::fs::read_dir;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use html_escape::{encode_double_quoted_attribute, encode_text};
use tide::{Body, Redirect, Request, Response, Result};

mod templates;
use templates::{DIR_LISTING, NOT_FOUND};

static BUILD_TIMESTAMP: &str = std::env!("BUILD_TIMESTAMP");

fn clamp_timestamp(ts: SystemTime) -> SystemTime {
    // timestamps in the filesystem are not necessarily reliable,
    // as yocto will fake them to be older than they actually are for
    // image reproducibility reasons.
    // Clamp the timestamps to the build date of the tacd to make sure
    // that files are reloaded on OS update.

    let tacd_build_time = {
        let seconds_since_epoch = BUILD_TIMESTAMP.parse().unwrap();
        let since_epoch = Duration::from_secs(seconds_since_epoch);

        SystemTime::UNIX_EPOCH + since_epoch
    };

    max(tacd_build_time, ts)
}

async fn file(req: &Request<()>, fs_path: &Path) -> Result {
    // Check the files modification date and compare it to the one provided
    // by the client (if any) to determine if we even need to send the file.
    let modification_date = fs_path.metadata()?.modified()?;

    let last_modified = {
        let modified = clamp_timestamp(modification_date);
        let modified: DateTime<Utc> = modified.into();

        modified.to_rfc2822().replace("+0000", "GMT")
    };

    let if_modified_since = req
        .header("If-Modified-Since")
        .map(|imss| imss.last().as_str());

    if Some(last_modified.as_str()) == if_modified_since {
        // The client already has the correct file, but thank you for asking.
        return Ok(Response::builder(304).build());
    }

    // fs_path = "/srv/www/file.html" -> gz_path = "/srv/www/file.html.gz"
    let gz_path: Option<PathBuf> = fs_path.to_str().map(|p| {
        let mut p = p.to_owned();
        p += ".gz";
        p.into()
    });

    // Serve a compressed variant of the file if it is available, the client
    // accepts it and the modification dates are exactly the same.
    let have_gz = gz_path
        .as_ref()
        .and_then(|p| p.metadata().ok())
        .map(|meta| {
            let is_file = meta.is_file();
            let dates_match = meta
                .modified()
                .map(|md| md == modification_date)
                .unwrap_or(false);

            is_file && dates_match
        })
        .unwrap_or(false);

    // There may be multiple Accept-Encoding headers (or none) and each one may
    // contain a list of accepted encodings, which is why this search is a bit
    // convoluted.
    // TL;DR: Check if "gzip" is somewhere in the accepted encodings.
    let accept_gz = req
        .header("Accept-Encoding")
        .map(|aes| {
            aes.iter()
                .flat_map(|ae| ae.as_str().split(','))
                .any(|aee| aee.trim() == "gzip")
        })
        .unwrap_or(false);

    // Make sure the client re-validates quite regularly if its cached
    // resource is still up to date (every 30s).
    let res_builder = Response::builder(200)
        .header("Last-Modified", last_modified)
        .header("Cache-Control", "max-age=30, must-revalidate");

    let body = Body::from_file(fs_path).await?;

    if have_gz && accept_gz {
        let mut gz_body = Body::from_file(gz_path.unwrap()).await?;
        gz_body.set_mime(body.mime().clone());

        let res = res_builder
            .header("Content-Encoding", "gzip")
            .body(gz_body)
            .build();

        Ok(res)
    } else {
        Ok(res_builder.body(body).build())
    }
}

/// If the URL provided by the user did not contain a trailing slash but the
/// object behind the URL is a directory redirect the user to the same URL but
/// with a trailing slash.
/// This makes it so that relative links in the index.html and generated
/// directory listing work as expected. (And it looks a bit nicer imho).
fn redirect_dir(url_path: &str) -> Result {
    let mut url_path = url_path.to_owned();

    if !url_path.ends_with('/') {
        url_path.push('/');
    }

    Ok(Redirect::new(url_path).into())
}

/// Scan a directory and return a list of contained files/directories as a
/// HTML page.
fn dir_listing(fs_path: &Path, is_root: bool) -> Result {
    struct ListEntry {
        name: String,
        is_dir: bool,
        html: String,
    }

    let mut rows = Vec::new();

    for entry in read_dir(fs_path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        let size = metadata.len();
        let is_dir = metadata.is_dir();

        let last_modified = {
            let lm = metadata.modified()?;
            let lm: DateTime<Utc> = lm.into();
            lm.to_rfc2822()
        };

        let name = {
            let mut name = entry.file_name().to_string_lossy().to_string();

            if is_dir {
                name.push('/')
            }

            name
        };

        let html = format!(
            r#"<tr>
              <td><a href="{}">{}</a></td>
              <td>{}</td>
              <td>{}</td>
            </tr>"#,
            encode_double_quoted_attribute(&name),
            encode_text(&name),
            encode_text(&last_modified),
            size
        );

        rows.push(ListEntry { name, is_dir, html })
    }

    // List directories before files and otherwise sort alphabetically
    rows.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) | (false, false) => a.name.cmp(&b.name),
    });

    let table_rows = {
        let mut html = String::new();

        // Add an "up" entry as long as we are not at the root of the exported
        // directory.
        if !is_root {
            html.push_str(
                r#"<tr>
                  <td><a href="..">..</a></td>
                  <td>-</td>
                  <td>-</td>
                "#,
            );
        }

        html.extend(rows.into_iter().map(|r| r.html));

        html
    };

    // Show the path _in the filesystem_ (in contrast to the path from the URL)
    // to the user so they know where to place files.
    let dir_name = fs_path.to_string_lossy();

    // Use fake html tags like <DIR_NAME/> to prevent users from injecting
    // e.g. TABLE_ROWS as part of the DIR_NAME by naming a directory TABLE_ROWS.
    // The HTML encoding performed above will prevent any literal < and > to leak
    // into the generated HTML.
    let html = DIR_LISTING
        .to_owned()
        .replace("<DIR_NAME/>", &encode_text(&dir_name))
        .replace("<TABLE_ROWS/>", &table_rows);

    let body = {
        let mut body = Body::from_string(html);
        body.set_mime("text/html;charset=utf-8");
        body
    };

    let res = Response::builder(200).body(body).build();

    Ok(res)
}

pub async fn serve_dir(base_path: &str, directory_listings: bool, req: Request<()>) -> Result {
    let url_path = req.url().path();
    let has_trailing_slash = url_path.ends_with('/');

    let rel_path = req.param("rel_path").unwrap_or("");

    let (path, is_root) = {
        let rel_path = Path::new(rel_path);
        let base_path = Path::new(base_path);
        let mut path = base_path.to_owned();

        // Prevent path traversal via e.g. http://tac/srv/../../../etc/passwd
        // by removing any non-normal path component.
        path.extend(rel_path.components().filter_map(|cmp| match cmp {
            Component::Normal(n) => Some(n),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => None,
        }));

        let is_root = path == base_path;

        (path, is_root)
    };

    let index_path = path.join("index.html");

    let is_dir = path.is_dir();
    let has_index = is_dir && index_path.is_file();

    let res = {
        if !is_dir {
            file(&req, &path).await
        } else if !has_trailing_slash {
            redirect_dir(url_path)
        } else if directory_listings && !has_index {
            dir_listing(&path, is_root)
        } else {
            file(&req, &index_path).await
        }
    };

    // Return a file not found error if something went wrong, as it is the
    // most likely cause.
    res.or_else(|_| {
        let mut body = Body::from_string(NOT_FOUND.to_owned());
        body.set_mime("text/html;charset=utf-8");

        Ok(Response::builder(404).body(body).build())
    })
}
