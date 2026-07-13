use serde::{Deserialize, Serialize};

/// Link graph tracking cross-note references (Wiki-links / markdown links).
/// Reserved for future development phases.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct LinkGraph {}
