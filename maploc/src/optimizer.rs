//! Sparse pose-graph Gauss-Newton optimizer.
//!
//! For our scales (≤ 50 nodes), a dense `H` of size 3N × 3N is fine and
//! avoids pulling in a sparse-linalg dependency. Each iteration:
//!
//!   1. Linearize each edge around the current poses → contributes
//!      `J_i^T Ω J_i, J_i^T Ω J_j, J_j^T Ω J_j` blocks to `H` and
//!      `J_i^T Ω r, J_j^T Ω r` blocks to `b`.
//!   2. Pin fixed nodes by zeroing their rows/columns and putting 1 on
//!      the diagonal so the system stays well-conditioned.
//!   3. Solve `H Δ = -b` with Gaussian elimination + partial pivoting.
//!   4. Apply increments, wrap yaw, repeat.

use crate::pose_graph::{PoseGraph, between, wrap_pi};

#[derive(Debug, Clone, Copy)]
pub struct OptimizerConfig {
    pub max_iters: u32,
    /// Huber threshold, in units of the edge's own sigma: an edge whose
    /// residual exceeds it stops being believed in proportion to how
    /// wrong it is, instead of pulling ever harder.
    ///
    /// Least squares gives every edge a quadratic pull, so one loop
    /// closure that matched the wrong corridor bends the whole graph and
    /// keeps bending it — which is what draws a corridor twice in a map,
    /// and why the map's quality was so sensitive to how confident
    /// closures were told to be (a fifth of walls out of place at one
    /// setting, a twentieth at a neighbouring one). Capping an outlier's
    /// influence treats the cause rather than trading one symptom for
    /// another. Zero disables it and restores plain least squares.
    ///
    /// **Off, and measured off.** Replaying seven recorded sessions across
    /// two simulated flats, at both the old edge confidence and the new
    /// one: with 40 cm edges no residual ever reaches 1.5 sigmas, so the
    /// kernel is inert and the maps come out identical; with 5 cm edges it
    /// bites and the result is a lottery — one recording improves by 17
    /// points of misplaced wall, another loses 21, and the mean is no
    /// better. Kept because it costs nothing and anyone tightening the
    /// edges again will want it, but it is not evidence-backed at any
    /// delta we tried.
    pub huber_delta: f32,
    /// Convergence: stop when |Δ| drops below all three thresholds.
    pub eps_xy: f32,
    pub eps_yaw: f32,
    /// Levenberg damping added to H's diagonal each iteration.
    pub lambda: f32,
    /// Loop edges whose residual exceeds this many sigmas after the graph
    /// has settled are disowned, one at a time, and the graph re-solved.
    ///
    /// Least squares cannot disown anything: one closure that matched the
    /// wrong corridor bends the whole graph and goes on bending it, which
    /// is what draws a corridor twice. Damping it (a Huber kernel, tried
    /// and measured) only softens the pull. Removing it is the standard
    /// answer, and the only one that can undo the damage.
    ///
    /// Zero disables the whole mechanism.
    ///
    /// **Measured, and it ships off.** Over the twin's recordings it never
    /// fires at three sigmas, because with 40 cm edges and corrections
    /// capped at 30 cm no residual is ever that large in sigma units — the
    /// same reason a Huber kernel was inert. Tightened to 0.2 sigma, about
    /// 8 cm, it helps a little and consistently: the worst run of a house
    /// goes from 21.6 % of wall beyond 10 cm to 18.7 %, the best from
    /// 1.2 % to 0.9 %. Real, but nowhere near the difference between a
    /// good run and a bad one, which is what needed explaining.
    pub reject_sigmas: f32,
    /// At most this many disowned per solve, so one bad freeze cannot
    /// strip a graph that is merely being optimised from a poor start.
    pub reject_max: u32,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            max_iters: 50,
            huber_delta: 0.0,
            eps_xy: 1e-4,
            eps_yaw: 1e-4,
            lambda: 1e-6,
            // Off until measured.
            reject_sigmas: 0.0,
            reject_max: 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OptimizeResult {
    pub iterations: u32,
    pub converged: bool,
    /// Sum of squared edge residuals at the final iterate.
    pub final_cost: f64,
}

pub fn optimize(graph: &mut PoseGraph, cfg: &OptimizerConfig) -> OptimizeResult {
    let mut result = solve_graph(graph, cfg);
    if cfg.reject_sigmas <= 0.0 {
        return result;
    }
    // A loop edge joins submaps that are not neighbours; an odometry edge
    // joins the one before to the one after, and is never disowned — it is
    // the only thing holding the graph together.
    for _ in 0..cfg.reject_max {
        let mut worst: Option<(usize, f32)> = None;
        for (i, edge) in graph.edges().iter().enumerate() {
            if edge.to.abs_diff(edge.from) <= 1 || edge.information[0][0] == 0.0 {
                continue;
            }
            let a = graph.nodes()[edge.from].pose;
            let b = graph.nodes()[edge.to].pose;
            let pred = between(a, b);
            let r = (
                pred.0 - edge.measurement.0,
                pred.1 - edge.measurement.1,
                wrap_pi(pred.2 - edge.measurement.2),
            );
            let info = edge.information;
            let chi2 = r.0 * (info[0][0] * r.0 + info[0][1] * r.1 + info[0][2] * r.2)
                + r.1 * (info[1][0] * r.0 + info[1][1] * r.1 + info[1][2] * r.2)
                + r.2 * (info[2][0] * r.0 + info[2][1] * r.1 + info[2][2] * r.2);
            if worst.is_none_or(|(_, w)| chi2 > w) {
                worst = Some((i, chi2));
            }
        }
        let Some((i, chi2)) = worst else { break };
        if chi2.max(0.0).sqrt() <= cfg.reject_sigmas {
            break;
        }
        graph.edges_mut()[i].information = [[0.0; 3]; 3];
        result = solve_graph(graph, cfg);
    }
    result
}

/// One Gauss-Newton solve, every edge taken at face value.
fn solve_graph(graph: &mut PoseGraph, cfg: &OptimizerConfig) -> OptimizeResult {
    let n = graph.nodes().len();
    if n == 0 || graph.edges().is_empty() {
        return OptimizeResult {
            iterations: 0,
            converged: true,
            final_cost: 0.0,
        };
    }
    let dim = 3 * n;
    let mut h = vec![vec![0.0_f64; dim]; dim];
    let mut bv = vec![0.0_f64; dim];
    let mut iter = 0u32;
    let mut converged = false;
    let mut last_cost = 0.0_f64;

    for it in 0..cfg.max_iters {
        iter = it + 1;
        // Reset H, b.
        for r in 0..dim {
            for c in 0..dim {
                h[r][c] = 0.0;
            }
        }
        for r in 0..dim {
            bv[r] = 0.0;
        }

        let mut cost = 0.0_f64;
        for edge in graph.edges() {
            let a = graph.nodes()[edge.from].pose;
            let b = graph.nodes()[edge.to].pose;
            let pred = between(a, b);
            // Residual = predicted − measured (with yaw wrap).
            let r = (
                pred.0 - edge.measurement.0,
                pred.1 - edge.measurement.1,
                wrap_pi(pred.2 - edge.measurement.2),
            );
            // Cost = r^T Ω r.
            let mut info = edge.information;
            let or0 = info[0][0] * r.0 + info[0][1] * r.1 + info[0][2] * r.2;
            let or1 = info[1][0] * r.0 + info[1][1] * r.1 + info[1][2] * r.2;
            let or2 = info[2][0] * r.0 + info[2][1] * r.1 + info[2][2] * r.2;
            let chi2 = r.0 * or0 + r.1 * or1 + r.2 * or2;
            cost += chi2 as f64;

            // Huber: past `delta` sigmas an edge's pull grows linearly
            // rather than quadratically, which in this formulation is one
            // scale factor on its information matrix.
            if cfg.huber_delta > 0.0 {
                let e = chi2.max(0.0).sqrt();
                if e > cfg.huber_delta {
                    let w = cfg.huber_delta / e;
                    for row in info.iter_mut() {
                        for v in row.iter_mut() {
                            *v *= w;
                        }
                    }
                }
            }

            // Jacobians: J_a (∂pred/∂a), J_b (∂pred/∂b).
            // pred = T_a^-1 * T_b, so partials follow the standard form
            // documented in `pose_graph.rs`. yaw_a is the relevant cos/sin.
            let (ca, sa) = (a.2.cos(), a.2.sin());
            let j_a: [[f32; 3]; 3] = [[-ca, -sa, pred.1], [sa, -ca, -pred.0], [0.0, 0.0, -1.0]];
            let j_b: [[f32; 3]; 3] = [[ca, sa, 0.0], [-sa, ca, 0.0], [0.0, 0.0, 1.0]];

            let from_off = 3 * edge.from;
            let to_off = 3 * edge.to;

            // H_aa += J_a^T Ω J_a    H_bb += J_b^T Ω J_b
            // H_ab += J_a^T Ω J_b    H_ba += J_b^T Ω J_a (transpose)
            // b_a  += J_a^T Ω r     b_b  += J_b^T Ω r
            // Recomputed after the reweighting: b and H must speak of the
            // same edge.
            let (or0, or1, or2) = (
                info[0][0] * r.0 + info[0][1] * r.1 + info[0][2] * r.2,
                info[1][0] * r.0 + info[1][1] * r.1 + info[1][2] * r.2,
                info[2][0] * r.0 + info[2][1] * r.1 + info[2][2] * r.2,
            );
            let oma = mat3_mul(&info, &j_a);
            let omb = mat3_mul(&info, &j_b);
            let haa = mat3_t_mul(&j_a, &oma);
            let hbb = mat3_t_mul(&j_b, &omb);
            let hab = mat3_t_mul(&j_a, &omb);
            let ba = mat3_t_vec(&j_a, &[or0, or1, or2]);
            let bb = mat3_t_vec(&j_b, &[or0, or1, or2]);

            for i in 0..3 {
                for j in 0..3 {
                    h[from_off + i][from_off + j] += haa[i][j] as f64;
                    h[to_off + i][to_off + j] += hbb[i][j] as f64;
                    h[from_off + i][to_off + j] += hab[i][j] as f64;
                    h[to_off + i][from_off + j] += hab[j][i] as f64; // transpose
                }
                bv[from_off + i] += ba[i] as f64;
                bv[to_off + i] += bb[i] as f64;
            }
        }
        last_cost = cost;

        // Levenberg damping.
        for d in 0..dim {
            h[d][d] += cfg.lambda as f64;
        }

        // Pin fixed nodes: zero rows/cols, set diagonal = 1, b = 0.
        for (idx, node) in graph.nodes().iter().enumerate() {
            if node.fixed {
                let off = 3 * idx;
                for k in 0..3 {
                    let row = off + k;
                    for c in 0..dim {
                        h[row][c] = 0.0;
                    }
                    for r in 0..dim {
                        h[r][row] = 0.0;
                    }
                    h[row][row] = 1.0;
                    bv[row] = 0.0;
                }
            }
        }

        // Solve H Δ = -b.
        let neg_b: Vec<f64> = bv.iter().map(|x| -x).collect();
        let dx = match solve(h.clone(), neg_b) {
            Some(v) => v,
            None => break, // singular system
        };

        // Apply increments.
        let mut max_dxy = 0.0_f32;
        let mut max_dyaw = 0.0_f32;
        for (idx, node) in graph.nodes_mut().iter_mut().enumerate() {
            if node.fixed {
                continue;
            }
            let off = 3 * idx;
            let dxx = dx[off] as f32;
            let dyy = dx[off + 1] as f32;
            let dyaw = dx[off + 2] as f32;
            node.pose.0 += dxx;
            node.pose.1 += dyy;
            node.pose.2 = wrap_pi(node.pose.2 + dyaw);
            let dxy = (dxx * dxx + dyy * dyy).sqrt();
            if dxy > max_dxy {
                max_dxy = dxy;
            }
            if dyaw.abs() > max_dyaw {
                max_dyaw = dyaw.abs();
            }
        }

        if max_dxy < cfg.eps_xy && max_dyaw < cfg.eps_yaw {
            converged = true;
            break;
        }
    }

    OptimizeResult {
        iterations: iter,
        converged,
        final_cost: last_cost,
    }
}

// ── Tiny linear-algebra helpers (3x3 * 3x3, 3x3 * vec3) ────────────

fn mat3_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut o = [[0.0_f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += a[i][k] * b[k][j];
            }
            o[i][j] = s;
        }
    }
    o
}

fn mat3_t_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    // a^T * b
    let mut o = [[0.0_f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += a[k][i] * b[k][j];
            }
            o[i][j] = s;
        }
    }
    o
}

fn mat3_t_vec(a: &[[f32; 3]; 3], v: &[f32; 3]) -> [f32; 3] {
    // a^T * v
    let mut o = [0.0_f32; 3];
    for i in 0..3 {
        let mut s = 0.0;
        for k in 0..3 {
            s += a[k][i] * v[k];
        }
        o[i] = s;
    }
    o
}

/// Gaussian elimination with partial pivoting. Returns `None` on
/// singular `A`. Both inputs are consumed.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for i in 0..n {
        // Pivot.
        let mut max_r = i;
        let mut max_v = a[i][i].abs();
        for r in (i + 1)..n {
            if a[r][i].abs() > max_v {
                max_r = r;
                max_v = a[r][i].abs();
            }
        }
        if max_v < 1e-12 {
            return None;
        }
        if max_r != i {
            a.swap(i, max_r);
            b.swap(i, max_r);
        }
        // Eliminate.
        for r in (i + 1)..n {
            let f = a[r][i] / a[i][i];
            if f == 0.0 {
                continue;
            }
            for c in i..n {
                a[r][c] -= f * a[i][c];
            }
            b[r] -= f * b[i];
        }
    }
    // Back-substitute.
    let mut x = vec![0.0_f64; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for c in (i + 1)..n {
            s -= a[i][c] * x[c];
        }
        x[i] = s / a[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pose_graph::{PoseEdge, information_from_sigmas};

    #[test]
    fn straight_chain_with_consistent_edges_is_already_optimal() {
        // Three nodes in a line, edges encode the actual deltas.
        let mut g = PoseGraph::new();
        g.add_node((0.0, 0.0, 0.0), 0);
        g.add_node((1.0, 0.0, 0.0), 1);
        g.add_node((2.0, 0.0, 0.0), 2);
        g.add_edge(PoseEdge {
            from: 0,
            to: 1,
            measurement: (1.0, 0.0, 0.0),
            information: information_from_sigmas(0.05, 0.02),
        });
        g.add_edge(PoseEdge {
            from: 1,
            to: 2,
            measurement: (1.0, 0.0, 0.0),
            information: information_from_sigmas(0.05, 0.02),
        });
        let r = optimize(&mut g, &OptimizerConfig::default());
        assert!(r.converged);
        // Poses should still be where we put them.
        for (i, expected) in [(0.0_f32), 1.0, 2.0].iter().enumerate() {
            assert!((g.nodes()[i].pose.0 - expected).abs() < 1e-3);
        }
    }

    #[test]
    fn loop_closure_pulls_the_third_pose_back() {
        // Three nodes around a triangle. Odom edges suggest 0→1→2 forms
        // a path away from origin. Loop edge 2→0 says "you're back at
        // start, dx=dy=0 in 2's frame after rotating π". Optimizer
        // should pull pose 2 toward (0,0).
        let mut g = PoseGraph::new();
        g.add_node((0.0, 0.0, 0.0), 0);
        g.add_node((1.0, 0.0, 0.0), 1);
        // Initial guess for node 2: somewhere off (drifted).
        g.add_node((2.0, 0.5, std::f32::consts::PI), 2);
        // Odom edges (loose-ish info).
        g.add_edge(PoseEdge {
            from: 0,
            to: 1,
            measurement: (1.0, 0.0, 0.0),
            information: information_from_sigmas(0.10, 0.05),
        });
        g.add_edge(PoseEdge {
            from: 1,
            to: 2,
            measurement: (1.0, 0.0, std::f32::consts::PI),
            information: information_from_sigmas(0.10, 0.05),
        });
        // Loop edge: from 2 (facing -x), getting back to 0 means moving
        // forward 2 m and rotating back to +x → relative pose
        // (2.0, 0.0, -PI) in node 2's frame.
        g.add_edge(PoseEdge {
            from: 2,
            to: 0,
            measurement: (2.0, 0.0, -std::f32::consts::PI),
            information: information_from_sigmas(0.05, 0.02),
        });

        // Before optimization, residuals are non-zero.
        let r0 = optimize(&mut g, &OptimizerConfig::default());
        assert!(r0.converged, "didn't converge");
        // After optimization, all edges should be (nearly) satisfied.
        // Node 2 should have moved toward the consistent loop position.
        let p2 = g.nodes()[2].pose;
        // The consistent position for pose 2 given the chain is
        // (2.0, 0.0, π) — i.e., x=2, y=0, yaw=π.
        assert!((p2.0 - 2.0).abs() < 0.05, "p2.x = {}", p2.0);
        assert!((p2.1 - 0.0).abs() < 0.05, "p2.y = {}", p2.1);
    }
}
