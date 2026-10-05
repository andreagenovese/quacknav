//! The teacher: it sees the truth — where the body is, every thing on the
//! floor, every mover — and walks the expert's field down, waiting for what
//! crosses the lane. It is what the pilot imitates first (behaviour
//! cloning, with DAgger: the pilot's own mistakes, labelled by it), before
//! it learns on its own reward.

use quack_nav::rlnav::{Action, STEP_S, STEP_VYAW_UNIT};

use crate::field::Field;
use crate::world::World;
use crate::wrap;

/// The yaw asked per radian of heading error (the stick's own gain).
const YAW_GAIN: f64 = 1.0 / (0.65 * STEP_S);
const TURN_FIRST_RAD: f64 = 0.6;

/// The teacher's move from the truth.
pub fn expert(world: &World, field: &Field, (x, y, yaw): (f64, f64, f64), stalls: u32) -> Action {
    // Something moving in the lane, near: let it pass.
    for m in &world.movers {
        let (dx, dy) = (m.x - x, m.y - y);
        let (ex, ey) = (yaw.cos() * dx + yaw.sin() * dy, -yaw.sin() * dx + yaw.cos() * dy);
        if ex > 0.0 && ex < 0.45 + m.r && ey.abs() < 0.16 + m.r {
            return if stalls >= 4 { Action::Back } else { Action::Wait };
        }
    }
    // Stuck: back off, then turn, by turns (a back-off the shield refuses
    // stands, and the turn gets the nose off what holds it).
    if stalls >= 2 {
        return if stalls % 4 < 2 { Action::Back } else if (stalls / 4) % 2 == 0 { Action::TurnLeft } else { Action::TurnRight };
    }
    let Some(look) = field.descend(x, y, 0.3) else { return Action::Wait };
    if (look.0 - x).hypot(look.1 - y) < 0.03 {
        return Action::Step(0);
    }
    let err = wrap((look.1 - y).atan2(look.0 - x) - yaw);
    if err.abs() > TURN_FIRST_RAD {
        return if err > 0.0 { Action::TurnLeft } else { Action::TurnRight };
    }
    let k = (YAW_GAIN * err / STEP_VYAW_UNIT).round().clamp(-2.0, 2.0) as i8;
    Action::Step(k)
}

/// The teacher as a brain on the journey's legs: the bench's upper bound,
/// and the stick's teacher in the trainer.
pub struct ExpertBrain {
    pub body: crate::body::SimBody,
    pub field: std::sync::Arc<Field>,
    state: std::sync::Mutex<(Option<(f64, f64)>, u32)>,
}

impl ExpertBrain {
    pub fn new(body: crate::body::SimBody, field: std::sync::Arc<Field>) -> Self {
        Self { body, field, state: std::sync::Mutex::new((None, 0)) }
    }

    /// The teacher's move now, counting its own stalls on the truth.
    pub fn label(&self) -> Action {
        let sim = self.body.lock();
        let here = (sim.x, sim.y);
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.1 = if st.0.is_some_and(|p| crate::dist(p, here) < 0.01) { st.1 + 1 } else { 0 };
        st.0 = Some(here);
        expert(&sim.world, &self.field, sim.truth(), st.1)
    }
}

impl quack_nav::rlnav::Brain for ExpertBrain {
    fn act(&self, _obs: &[f32]) -> Action {
        self.label()
    }
}
