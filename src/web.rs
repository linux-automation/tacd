use std::net::TcpListener;

use systemd::daemon::{listen_fds, tcp_listener};
use tide::Server;

mod static_files;

pub async fn serve(mut server: Server<()>) -> Result<(), std::io::Error> {
    // Use sockets provided by systemd (if any)
    let mut listeners: Vec<TcpListener> = listen_fds(true)
        .map(|fds| fds.iter().filter_map(|fd| tcp_listener(fd).ok()).collect())
        .unwrap_or(Vec::new());

    // Open [::]:80 outselves if systemd did not provide anything.
    // This, somewhat confusingly also listens on 0.0.0.0.
    if listeners.is_empty() {
        listeners.push(TcpListener::bind("[::]:80").expect(
            "Could not bind web API to [::]:80, is there already another service running?",
        ));
    }

    static_files::register(&mut server);

    server.listen(listeners).await
}
