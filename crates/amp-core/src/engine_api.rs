//! Versioned process boundary for the optional analysis add-on. No DSP implementation.
use crate::Preparation;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub const PROTOCOL: u32 = 1;
pub const ENGINE_ID: &str = "amp-prep-heuristic-1";
pub const JOB_TYPE: &str = "preparation";
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub protocol: u32,
    pub root: PathBuf,
    pub relative: PathBuf,
    pub draft: Option<Preparation>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Prepared {
        protocol: u32,
        preparation: Box<Preparation>,
    },
    Skipped {
        protocol: u32,
    },
    Error {
        protocol: u32,
        message: String,
    },
}
