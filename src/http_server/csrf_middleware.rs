// This file is part of tacd, the LXA TAC system daemon
// Copyright (C) 2026 Pengutronix e.K.
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
// with this library; if not, see <https://www.gnu.org/licenses/>.

use tide::{Middleware, Next, Request};

pub struct CsrfMiddleware {}

fn get_exactly_one_header<'a, R>(req: &'a Request<R>, name: &str) -> Option<&'a str> {
    let mut values = req.header(name)?.iter();
    let value = values.next()?;

    if values.next().is_some() {
        // There is a second header.
        // We can not decide which one is valid, so decide against both.
        return None;
    }

    Some(value.as_str())
}

fn has_header<R>(req: &Request<R>, name: &str) -> bool {
    req.header(name).is_some()
}

impl CsrfMiddleware {
    pub fn new() -> Self {
        Self {}
    }
}

#[tide::utils::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for CsrfMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        // Browsers tend to send `Origin`, `Referer` or `Sec-Fetch-Site` headers,
        // while other API users do not. Use that to apply different rules for them.
        // An alternative would be to check the user agent, but let's not do that.
        let has_origin = has_header(&req, "Origin");
        let has_referer = has_header(&req, "Referer");
        let has_sec_fetch = has_header(&req, "Sec-Fetch-Site");
        let is_browser = has_origin || has_referer || has_sec_fetch;

        // Check if the browser sent CSRF protection headers and treat the request
        // differently if they did.
        let sec_fetch_site_header = get_exactly_one_header(&req, "Sec-Fetch-Site");
        let is_same_origin = match sec_fetch_site_header {
            Some("same-origin") => {
                // Request comes from a `fetch()`, `XMLHttpRequest` or some some other
                // source triggered from javascript or something else triggered by a site.
                true
            },
            Some("none") => {
                // This resource was requested directly by the browser.
                // For example by typing it into the URL bar
                true
            }
            Some(_) | None => {
                // Either a cross origin request or no header was sent.
                false
            }
        };

        if is_browser && !is_same_origin {
            // Block requests from browsers that do not carry the correct CSRF headers.
            Err(tide::Error::from_str(
                tide::StatusCode::Forbidden,
                "Cross-Origin requests from browsers are not allowed",
            ))
        } else {
            // Accept requests that do not come from a browser (i.e. users of the API).
            // Accept requests from browsers that _do_ carry the correct CSRF headers.
            Ok(next.run(req).await)
        }
    }
}
