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

use std::time::SystemTime;

use crate::auth::TokenStore;

use tide::{Middleware, Next, Request};

pub struct TokenMiddleware {
    token_store: TokenStore,
}

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

impl TokenMiddleware {
    pub fn new(token_store: TokenStore) -> Self {
        Self { token_store }
    }
}

#[tide::utils::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for TokenMiddleware {
    async fn handle(&self, mut req: Request<State>, next: Next<'_, State>) -> tide::Result {
        let now = SystemTime::now();

        let auth_param = req.param("token").ok();
        let auth_header = get_exactly_one_header(&req, "Authorization");

        // Do not allow ambiguity. Respect either the header, or the query parameter.
        let token_req = auth_param
            .xor(auth_header)
            .and_then(|auth| auth.strip_prefix("Bearer "))
            .and_then(|bearer| self.token_store.decrypt(bearer).ok())
            .and_then(|token| (now < token.expires).then_some(token));

        // Generate a fallback token in case none was sent by the client or it expired.
        // Browsers send an `Origin` or `Referer` header while other API users do not.
        // Use that to apply different rules for them.
        let has_origin = has_header(&req, "Origin");
        let has_referer = has_header(&req, "Referer");
        let is_browser = has_origin || has_referer;

        // Check if the browser sent CSRF protection headers and treat the request
        // differently if they did.
        let sec_fetch_site_header = get_exactly_one_header(&req, "Sec-Fetch-Site");
        let is_same_origin = sec_fetch_site_header == Some("same-origin");

        let token_fallback = self.token_store.fallback_token(is_browser, is_same_origin);

        let token = token_req.unwrap_or(token_fallback);

        req.set_ext(token);

        Ok(next.run(req).await)
    }
}
