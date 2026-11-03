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

use crate::auth::{Capability, Token};

use tide::{Middleware, Next, Request};

#[derive(Clone, Copy)]
pub enum Direction {
    Read,
    Write,
}

pub struct AuthMiddleware {
    required_capability: Capability,
    direction: Direction,
}

impl AuthMiddleware {
    pub fn new(required_capability: Capability, direction: Direction) -> Self {
        Self {
            required_capability,
            direction,
        }
    }
}

#[tide::utils::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for AuthMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        let token: &Token = req.ext::<Token>().unwrap();

        let capabilities = match self.direction {
            Direction::Read => token.capabilities_read,
            Direction::Write => token.capabilities_write,
        };

        if !capabilities.is_set(self.required_capability) {
            return Err(tide::Error::from_str(
                403,
                "Token is missing required capability",
            ));
        }

        Ok(next.run(req).await)
    }
}
