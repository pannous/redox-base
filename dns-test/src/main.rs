use std::net::ToSocketAddrs;

fn main() {
    eprintln!("DNS test starting");

    eprintln!("Resolving pannous.com:80");
    match "pannous.com:80".to_socket_addrs() {
        Ok(addrs) => {
            eprintln!("Resolved:");
            for addr in addrs {
                eprintln!("  {}", addr);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    eprintln!("Done");
}
