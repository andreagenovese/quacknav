//! rl_show: one scenario as JSON (the world, the start, the goal, the
//! bias) — for a look at what a bench's journey met.
//!
//!     rl_show SEED [LEVEL] [FAMILY]
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let seed: u64 = a.first().and_then(|v| v.parse().ok()).unwrap_or(1);
    let level: u32 = a.get(1).and_then(|v| v.parse().ok()).unwrap_or(3);
    let s = quack_rl::scenarios::generate(seed, level, a.get(2).map(String::as_str));
    println!("{}", serde_json::to_string_pretty(&s).unwrap());
}
