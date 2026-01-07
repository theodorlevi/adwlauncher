use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum OpenType {
    Graphical,
    Terminal,
    Window,
}

impl Default for OpenType {
    fn default() -> Self {
        Self::Graphical
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub open_type: OpenType,
    pub exec: String,
    pub icon: String,
    pub name: String,
}
