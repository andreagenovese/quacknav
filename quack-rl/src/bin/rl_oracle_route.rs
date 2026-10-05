//! rl_oracle_route: a route on the oracle's map (`QK_ORACLE_AS_MAPPED`),
//! the true rims on the books — what a twin journey plans, offline.
//!
//!     rl_oracle_route WALLS.toml TRUTH.json x0,y0 x1,y1
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let walls = quack_nav::oracle::read_walls(&a[0])?;
    let holes = quack_nav::oracle::read_holes(&a[1])?;
    let boxes = quack_nav::oracle::read_boxes(&a[1])?;
    let m = quack_nav::oracle::draw_as(&walls, &holes, &boxes, true);
    let cells = m.cells.iter().map(|c| match c {
        1 => quack_nav::map::Cell::Free,
        2 => quack_nav::map::Cell::Wall,
        _ => quack_nav::map::Cell::Unknown,
    }).collect();
    let grid = quack_nav::map::Grid { rows: m.rows, cols: m.cols, x_min: m.x_min, y_min: m.y_min, cell_m: 0.05, cells };
    let p = |s: &str| { let v: Vec<f64> = s.split(',').map(|x| x.parse().unwrap()).collect(); (v[0], v[1]) };
    let (s, g) = (p(&a[2]), p(&a[3]));
    // SAFETY: one thread, before anything reads the environment.
    unsafe { std::env::set_var("QK_ORACLE_BOOK", &a[1]) };
    let books: Vec<quack_nav::frontier::ExtraWall> = quack_nav::oracle::book(0.10).unwrap_or_default().into_iter().map(|(q, _)| (q, 0.12)).collect();
    for (name, w) in [("no books", vec![]), ("books", books)] {
        let r = quack_nav::frontier::path_to(&grid, s.0, s.1, g, &w, quack_nav::frontier::inflate_m(), &[]);
        println!("{name}: {}", r.map(|r| format!("{} points", r.len())).unwrap_or_else(|| "NO WAY".into()));
    }
    // The cells round the goal.
    for dy in [0.3, 0.15, 0.0, -0.15, -0.3] {
        let row: String = [-0.3, -0.15, 0.0, 0.15, 0.3].iter().map(|dx| match grid.at(g.0 + dx, g.1 + dy) { Some(quack_nav::map::Cell::Free) => '.', Some(quack_nav::map::Cell::Wall) => '#', _ => '?' }).collect();
        println!("{row}");
    }
    Ok(())
}
