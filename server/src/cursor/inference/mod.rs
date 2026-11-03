//! Adapts Cursor's client-owned Agent loop to configured model providers.
mod project;
mod service;

pub(crate) use project::project;
pub(crate) use service::{decode_append, InferenceRoute, InferenceService};
