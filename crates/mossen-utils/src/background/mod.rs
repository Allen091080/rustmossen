//! Background utilities — translated from utils/background/remote/

use serde::{Deserialize, Serialize};
use std::time::Duration;
use anyhow::Result;

pub mod preconditions;
pub mod remote_session;

pub use preconditions::*;
pub use remote_session::*;
