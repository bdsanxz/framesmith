//! A render: the job, the five stages it goes through, and what is left
//! behind when it stops early. Specified in `docs/spec/render-stages.md`.

mod debug;
mod job;
mod outcome;
mod run;
mod site;
mod staging;

pub use debug::{Debug, Order};
pub use job::{Job, JobFormat};
pub use outcome::{Event, Failure, Reporter, Stage, Stopped, Summary};
pub use run::{run, Setup};
pub use site::Site;
pub use staging::Staging;
