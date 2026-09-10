//! ruagent-core: domain model shared by every crate.
//!
//! Pure types, zero I/O: [`Task`](task::Task), [`Run`](run::Run), the
//! normalized [`RunEvent`](event::RunEvent) stream, and
//! [`RoutingDecision`](routing::RoutingDecision). See
//! `docs/plans/2026-09-11-ruagent-design.md` §4.3 and §5.

pub mod agent;
pub mod event;
pub mod id;
pub mod routing;
pub mod run;
pub mod task;
pub mod usage;

pub use agent::{AgentCard, HarnessKind, ReasoningEffort};
pub use event::{
    ContentBlock, PermissionOption, PermissionResolution, PlanEntry, PlanEntryStatus, RunEvent,
};
pub use id::{AgentId, RunId, TaskId};
pub use routing::{RouteSource, RoutingDecision};
pub use run::{Run, RunParams, RunStatus, StopReason};
pub use task::{EdgeKind, Task, TaskCreator, TaskEdge, TaskStatus};
pub use usage::{ModelPrice, UsageTotals};
