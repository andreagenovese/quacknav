//! The environment's knobs, by their name: `QK_<NAME>`.
//!
//! One place that says what the prefix is. The knobs named after quacksat
//! when the navigation lived there (`QUACKSAT_*`) were renamed on
//! 2026-09-30 and the old names are not read.

/// `QK_<name>`.
pub fn qk(name: &str) -> Option<String> {
    std::env::var(format!("QK_{name}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_knob_is_read_by_its_qk_name_only() {
        // Names no other test touches.
        unsafe {
            std::env::set_var("QK_ENV_TEST_A", "1");
            std::env::set_var("QUACKSAT_ENV_TEST_B", "1");
        }
        assert_eq!(qk("ENV_TEST_A").as_deref(), Some("1"));
        assert_eq!(qk("ENV_TEST_B"), None, "the old prefix is gone");
    }
}
