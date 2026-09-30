//! The environment's knobs, by their name: `QK_<NAME>`, or the name they had
//! when the navigation lived in quacksat, `QUACKSAT_<NAME>`, still read so
//! a script or a unit file written then keeps working (renamed 2026-09-30).

/// `QK_<name>`, else `QUACKSAT_<name>`.
pub fn knob(name: &str) -> Option<String> {
    std::env::var(format!("QK_{name}")).or_else(|_| std::env::var(format!("QUACKSAT_{name}"))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_name_wins_and_the_old_one_still_reads() {
        // Names no other test touches.
        unsafe {
            std::env::set_var("QUACKSAT_ENV_TEST_A", "old");
            std::env::set_var("QUACKSAT_ENV_TEST_B", "old");
            std::env::set_var("QK_ENV_TEST_B", "new");
        }
        assert_eq!(knob("ENV_TEST_A").as_deref(), Some("old"));
        assert_eq!(knob("ENV_TEST_B").as_deref(), Some("new"));
        assert_eq!(knob("ENV_TEST_NONE"), None);
    }
}
