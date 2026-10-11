//! Maintainer switches, read from the environment. Not part of the user
//! interface; listed in `docs/spec/render-stages.md`.

use crate::Stage;
use std::path::PathBuf;

/// The order frames are captured in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Forward,
    /// From the last frame to the first.
    Reverse,
    /// These frames first, as listed, then every other frame from the first to the last.
    First(Vec<u64>),
}

impl Order {
    /// `reverse`, or frame numbers separated by commas. Anything else is forward order.
    pub fn parse(written: &str) -> Order {
        if written == "reverse" {
            return Order::Reverse;
        }
        match written.split(',').map(|n| n.trim().parse()).collect() {
            Ok(first) => Order::First(first),
            Err(_) => Order::Forward,
        }
    }

    /// Every frame of a video of `frames` frames, once, in capture order.
    pub fn of(&self, frames: u64) -> Vec<u64> {
        match self {
            Order::Forward => (0..frames).collect(),
            Order::Reverse => (0..frames).rev().collect(),
            Order::First(first) => {
                let mut order: Vec<u64> = Vec::with_capacity(frames as usize);
                for &frame in first.iter().chain(&(0..frames).collect::<Vec<u64>>()) {
                    if frame < frames && !order.contains(&frame) {
                        order.push(frame);
                    }
                }
                order
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Debug {
    /// Every captured picture is also saved here.
    pub frames_dir: Option<PathBuf>,
    /// The order frames are captured in; they are always encoded in order.
    pub order: Order,
    /// Wait at the start of this stage so a test can interrupt there.
    pub hold: Option<Stage>,
    /// A fault the loader is asked to commit.
    pub fault: Option<String>,
    /// The render job is written here when it is created.
    pub job_file: Option<PathBuf>,
    /// What each seek cost, as the page measured it, is written here after capture.
    pub costs_file: Option<PathBuf>,
}

impl Debug {
    pub fn from_env() -> Debug {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Debug {
            frames_dir: var("FRAMESMITH_DEBUG_FRAMES").map(PathBuf::from),
            order: var("FRAMESMITH_DEBUG_ORDER").map_or(Order::Forward, |written| Order::parse(&written)),
            hold: var("FRAMESMITH_DEBUG_HOLD").and_then(|name| Stage::from_name(&name)),
            fault: var("FRAMESMITH_DEBUG_FAULT"),
            job_file: var("FRAMESMITH_DEBUG_JOB").map(PathBuf::from),
            costs_file: var("FRAMESMITH_DEBUG_COSTS").map(PathBuf::from),
        }
    }
}
