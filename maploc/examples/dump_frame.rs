//! A saved session as the same JSON a live `map.frame` carries.
//!
//!     cargo run -p maploc --example dump_frame -- <session> > frame.json
//!
//! So that a map on disk can be measured by the same tools that measure a
//! map in flight — quacksat's `mapquality.py` scores either.

use maploc::pipeline::{Slam, SlamConfig};
use maploc::session::SessionState;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_frame <session>");
    let session = SessionState::load(std::path::Path::new(&path))
        .expect("read the session")
        .expect("the session is empty");
    let slam = Slam::from_session(SlamConfig::default(), session);
    let grid = slam.render().expect("the session has no map");
    let cfg = *grid.cfg();
    let (w, h) = (grid.width(), grid.height());
    let log = grid.log_raw();
    // The wire's one byte a cell: 0 unknown, 1 free, 2 wall.
    let cells: Vec<u8> = log
        .iter()
        .map(|lo| match *lo {
            l if l >= 200 => 2,
            l if l <= -200 => 1,
            _ => 0,
        })
        .collect();
    let tracked = slam.tracked();
    println!(
        "{{\"frame\":{{\"seq\":0,\"x\":{},\"y\":{},\"yaw\":{},\"tracking\":true,\"x_min\":{},\"y_min\":{},\
         \"cell_m\":{},\"rows\":{h},\"cols\":{w},\"cells\":\"{}\",\"n_submaps\":{},\"n_loops\":0,\
         \"windows\":0,\"still\":true,\"seated\":false}},\"trail\":[]}}",
        tracked.0,
        tracked.1,
        tracked.2,
        cfg.x_range.0,
        cfg.y_range.0,
        cfg.cell,
        b64(&cells),
        slam.n_submaps(),
    );
}

/// Standard base64, no padding rules to argue about: three bytes to four
/// characters, `=` to fill.
fn b64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}
