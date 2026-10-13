pub mod cdp;
pub mod discovery;
pub mod management;
pub mod probe;
pub mod recycle;
pub const BOOTSTRAP: &str = include_str!("../../../injected/bootstrap.js");
pub const PROBE: &str = include_str!("../../../injected/probe.js");
