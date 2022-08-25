use std::fs::write;
use std::net::TcpListener;

use tide::{Request, Response, Server};

#[cfg(any(test, feature = "stub_out_root"))]
mod sd {
    use std::io::Result;
    use std::net::TcpListener;

    pub const FALLBACK_PORT: &str = "[::]:8080";

    pub fn listen_fds(_: bool) -> Result<[(); 0]> {
        Ok([])
    }

    pub fn tcp_listener<E>(_: E) -> Result<TcpListener> {
        unimplemented!()
    }
}

#[cfg(not(any(test, feature = "stub_out_root")))]
mod sd {
    pub use systemd::daemon::*;

    pub const FALLBACK_PORT: &str = "[::]:80";
}

use sd::{listen_fds, tcp_listener, FALLBACK_PORT};

mod static_files;

pub struct WebInterface {
    listeners: Vec<TcpListener>,
    pub server: Server<()>,
}

impl WebInterface {
    pub fn new() -> Self {
        let mut this = Self {
            listeners: Vec::new(),
            server: tide::new(),
        };

        // Use sockets provided by systemd (if any) to make socket activation
        // work
        if let Ok(fds) = listen_fds(true) {
            this.listeners
                .extend(fds.iter().filter_map(|fd| tcp_listener(fd).ok()));
        }

        // Open [::]:80 / [::]:8080 outselves if systemd did not provide anything.
        // This, somewhat confusingly also listens on 0.0.0.0.
        if this.listeners.is_empty() {
            this.listeners.push(TcpListener::bind(FALLBACK_PORT).expect(
                "Could not bind web API to port, is there already another service running?",
            ));
        }

        // Serve the React based web interface that is (currently) included in
        // the tacd binary.
        static_files::register(&mut this.server);

        this
    }

    // Serve a file from disk for reading and writing
    pub fn expose_file_rw(&mut self, fs_path: &str, web_path: &str) {
        self.server.at(web_path).serve_file(fs_path).unwrap();

        let fs_path = fs_path.to_string();

        self.server.at(web_path).put(move |mut req: Request<()>| {
            let fs_path = fs_path.clone();

            async move {
                let content = req.body_bytes().await?;
                write(&fs_path, &content)?;

                Ok(Response::new(204))
            }
        });
    }

    pub async fn serve(self) -> Result<(), std::io::Error> {
        self.server.listen(self.listeners).await
    }
}
