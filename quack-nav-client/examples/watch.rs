//! watch: say hello to quack-navd and print its events as they come.
//!
//!     cargo run -p quack-nav-client --example watch -- /run/quack-nav/nav.sock [topic…]
fn main() -> Result<(), quack_nav_client::Error> {
    let mut args = std::env::args().skip(1);
    let socket = args.next().unwrap_or_else(|| "/run/quack-nav/nav.sock".into());
    let topics: Vec<String> = args.collect();
    let mut nav = quack_nav_client::Client::connect(&socket)?;
    let hello = nav.hello("watch")?;
    println!("quack-navd {} speaks v{} (this client v{}); events: {}", hello.daemon_version, hello.api_version, quack_nav_client::API_VERSION, hello.events.join(", "));
    let topics: Vec<&str> = topics.iter().map(String::as_str).collect();
    for event in quack_nav_client::subscribe(&socket, &topics)? {
        let e = event?;
        println!("{:>8.1}  #{:<4} {:<13} {}", e.t, e.seq, e.topic, e.data);
    }
    Ok(())
}
