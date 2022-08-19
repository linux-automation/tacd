use std::net::TcpListener;

use systemd::daemon::{listen_fds, tcp_listener};
use tide::Server;

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

        // Use sockets provided by systemd (if any)
        if let Ok(fds) = listen_fds(true) {
            this.listeners
                .extend(fds.iter().filter_map(|fd| tcp_listener(fd).ok()));
        }

        // Open [::]:80 outselves if systemd did not provide anything.
        // This, somewhat confusingly also listens on 0.0.0.0.
        if this.listeners.is_empty() {
            this.listeners.push(TcpListener::bind("[::]:80").expect(
                "Could not bind web API to [::]:80, is there already another service running?",
            ));
        }

        static_files::register(&mut this.server);

        this
    }

    pub async fn serve(self) -> Result<(), std::io::Error> {
        self.server.listen(self.listeners).await
    }
}
