//! One owner at a time (`nav.claim`, `nav.release`; protocol v3).
//!
//! Without a lease anyone may drive, as before. A client that claims the
//! duck gets a short lease, tied to its connection: while it holds it, the
//! tools that move the duck are refused to every other connection, with the
//! holder's name. Reads are never refused, and a stop never is — safety
//! does not wait for ownership; a stop from anyone ends the lease. The
//! lease ends at its ttl, at `nav.release`, and when the holder's
//! connection closes.
//!
//! The lease is the navigation's, not the motors': `robotd` stays the only
//! owner of the joints, its deadman the backstop. The design is
//! `docs/design/service.md` §4.

use std::time::{Duration, Instant};

use serde_json::Value;

/// The longest a lease may be asked for at once.
pub const MAX_TTL_S: f64 = 300.0;

/// The tools that move the duck, and so need the lease when one is held.
pub const MOVING: [&str; 7] = ["robot.go_to", "robot.map_explore", "robot.map_step", "robot.move", "robot.act", "nav.jobs", "nav.act"];

/// A call that only stops: never refused.
pub fn is_stop(tool: &str, args: &Value) -> bool {
    matches!(tool, "robot.go_to" | "robot.map_explore") && args.get("stop").and_then(Value::as_bool) == Some(true)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lease {
    pub token: String,
    pub holder: String,
    /// The connection that holds it.
    pub conn: u64,
    pub until: Instant,
}

/// What a claim, a release or a check came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The call may go ahead.
    Free,
    /// Refused: another connection holds the lease.
    Held { holder: String, left_s: f64 },
}

/// The daemon's lease, if any.
#[derive(Debug, Default)]
pub struct Owner {
    lease: Option<Lease>,
    next: u64,
}

impl Owner {
    fn expire(&mut self, now: Instant) -> Option<Lease> {
        if self.lease.as_ref().is_some_and(|l| now >= l.until) { self.lease.take() } else { None }
    }

    /// The lease held now, an expired one let go first. The second value is
    /// the lease that just expired, for the caller to say so.
    pub fn current(&mut self, now: Instant) -> (Option<&Lease>, Option<Lease>) {
        let expired = self.expire(now);
        (self.lease.as_ref(), expired)
    }

    /// `nav.claim` from connection `conn`: a new lease when none is held
    /// (or the old one expired), a renewal when `token` is the holder's,
    /// refused when another connection holds it.
    pub fn claim(&mut self, conn: u64, holder: &str, ttl_s: f64, token: Option<&str>, now: Instant) -> Result<Lease, Verdict> {
        self.expire(now);
        let ttl = Duration::from_secs_f64(ttl_s.clamp(1.0, MAX_TTL_S));
        match &mut self.lease {
            Some(l) if l.conn == conn || token == Some(l.token.as_str()) => {
                l.until = now + ttl;
                l.conn = conn;
                if !holder.is_empty() {
                    l.holder = holder.to_string();
                }
                Ok(l.clone())
            }
            Some(l) => Err(Verdict::Held { holder: l.holder.clone(), left_s: l.until.saturating_duration_since(now).as_secs_f64() }),
            None => {
                self.next += 1;
                // Not a secret (the socket is the owner's), only hard to
                // guess by mistake.
                use std::hash::{BuildHasher, Hasher};
                let mut h = std::collections::hash_map::RandomState::new().build_hasher();
                h.write_u64(conn);
                h.write_u64(self.next);
                let token = format!("lease-{}-{:016x}", self.next, h.finish());
                let l = Lease { token, holder: if holder.is_empty() { "a client".into() } else { holder.to_string() }, conn, until: now + ttl };
                self.lease = Some(l.clone());
                Ok(l)
            }
        }
    }

    /// `nav.release`: the holder's token (or its connection) ends it.
    /// Whether a lease ended.
    pub fn release(&mut self, conn: u64, token: Option<&str>) -> Option<Lease> {
        if self.lease.as_ref().is_some_and(|l| l.conn == conn || token == Some(l.token.as_str())) { self.lease.take() } else { None }
    }

    /// The holder's connection closed: its lease ends with it.
    pub fn gone(&mut self, conn: u64) -> Option<Lease> {
        if self.lease.as_ref().is_some_and(|l| l.conn == conn) { self.lease.take() } else { None }
    }

    /// May connection `conn` call `tool` with `args` now? A stop always may,
    /// and ends a lease held by someone else.
    pub fn check(&mut self, conn: u64, tool: &str, args: &Value, now: Instant) -> (Verdict, Option<Lease>) {
        let expired = self.expire(now);
        let Some(l) = &self.lease else { return (Verdict::Free, expired) };
        if is_stop(tool, args) {
            let ended = if l.conn != conn { self.lease.take() } else { None };
            return (Verdict::Free, ended.or(expired));
        }
        if l.conn == conn || !MOVING.contains(&tool) {
            return (Verdict::Free, expired);
        }
        (Verdict::Held { holder: l.holder.clone(), left_s: l.until.saturating_duration_since(now).as_secs_f64() }, expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn without_a_lease_anyone_drives_and_with_one_only_its_holder() {
        let mut o = Owner::default();
        let t = Instant::now();
        assert_eq!(o.check(1, "robot.go_to", &json!({"place": "kitchen"}), t).0, Verdict::Free);
        let l = o.claim(1, "quacksat", 30.0, None, t).unwrap();
        assert_eq!(l.holder, "quacksat");
        // The holder drives; another connection is refused by name; reads pass.
        assert_eq!(o.check(1, "robot.go_to", &json!({}), t).0, Verdict::Free);
        match o.check(2, "robot.move", &json!({"vx": 0.3}), t).0 {
            Verdict::Held { holder, left_s } => assert!(holder == "quacksat" && left_s > 29.0),
            v => panic!("{v:?}"),
        }
        assert_eq!(o.check(2, "robot.map_status", &json!({}), t).0, Verdict::Free);
        // Another claim is refused; the holder renews with its token from a new connection.
        assert!(matches!(o.claim(2, "control page", 30.0, None, t), Err(Verdict::Held { .. })));
        let renewed = o.claim(3, "", 60.0, Some(&l.token), t).unwrap();
        assert_eq!((renewed.conn, renewed.holder.as_str()), (3, "quacksat"));
        assert!(matches!(o.check(1, "robot.go_to", &json!({}), t).0, Verdict::Held { .. }), "the lease moved with the token");
    }

    #[test]
    fn a_stop_is_never_refused_and_ends_the_lease() {
        let mut o = Owner::default();
        let t = Instant::now();
        o.claim(1, "the agent", 60.0, None, t).unwrap();
        let (v, ended) = o.check(2, "robot.go_to", &json!({"stop": true}), t);
        assert_eq!(v, Verdict::Free);
        assert_eq!(ended.unwrap().holder, "the agent");
        assert_eq!(o.check(2, "robot.go_to", &json!({}), t).0, Verdict::Free, "the duck is free again");
    }

    #[test]
    fn a_lease_ends_at_its_ttl_at_its_release_and_with_its_connection() {
        let mut o = Owner::default();
        let t = Instant::now();
        o.claim(1, "a", 5.0, None, t).unwrap();
        let (v, expired) = o.check(2, "robot.go_to", &json!({}), t + Duration::from_secs(6));
        assert_eq!(v, Verdict::Free);
        assert_eq!(expired.unwrap().holder, "a");
        let l = o.claim(1, "a", 5.0, None, t).unwrap();
        assert_eq!(o.release(2, None), None, "not the holder's to release");
        assert_eq!(o.release(2, Some(&l.token)).unwrap().holder, "a");
        o.claim(4, "b", 5.0, None, t).unwrap();
        assert_eq!(o.gone(4).unwrap().holder, "b");
        assert_eq!(o.current(t).0, None);
        // The ttl is bounded.
        let l = o.claim(5, "c", 10_000.0, None, t).unwrap();
        assert!(l.until <= t + Duration::from_secs_f64(MAX_TTL_S));
    }
}
