use std::io::{self, BufRead};
use std::thread;
use vibecraft::network::server::{server_usage, HeadlessServer, ServerConfig, ServerError};

fn main() {
    env_logger::init();

    let config = match ServerConfig::from_args(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(ServerError::HelpRequested) => {
            println!("{}", server_usage());
            return;
        }
        Err(error) => {
            eprintln!("{error}");
            eprintln!("\n{}", server_usage());
            std::process::exit(2);
        }
    };

    let mut server = match HeadlessServer::bind(config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("failed to start headless server: {error}");
            std::process::exit(1);
        }
    };
    let address = match server.local_addr() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("failed to inspect server address: {error}");
            std::process::exit(1);
        }
    };
    log::info!("headless server listening on {address}; type `quit` to save and stop");

    let shutdown = server.shutdown_token();
    let input_shutdown = shutdown.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line)
                    if line.trim().eq_ignore_ascii_case("quit")
                        || line.trim().eq_ignore_ascii_case("stop") =>
                {
                    input_shutdown.request_shutdown();
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    log::warn!("server console input failed: {error}");
                    input_shutdown.request_shutdown();
                    break;
                }
            }
        }
        // EOF is treated as a clean stop so non-interactive launches do not
        // leave a world process running without an owner.
        input_shutdown.request_shutdown();
    });

    if let Err(error) = server.run_until_shutdown(&shutdown) {
        eprintln!("headless server stopped with an error: {error}");
        std::process::exit(1);
    }
}
