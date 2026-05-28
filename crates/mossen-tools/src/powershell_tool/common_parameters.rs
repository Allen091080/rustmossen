//! # common_parameters — PowerShell Common Parameters
//!
//! Translates `tools/PowerShellTool/commonParameters.ts`.
//! PowerShell Common Parameters (available on all cmdlets via [CmdletBinding()]).

use std::collections::HashSet;
use std::sync::LazyLock;

/// Common switch parameters (no value needed).
pub const COMMON_SWITCHES: &[&str] = &["-verbose", "-debug"];

/// Common parameters that take a value.
pub const COMMON_VALUE_PARAMS: &[&str] = &[
    "-erroraction",
    "-warningaction",
    "-informationaction",
    "-progressaction",
    "-errorvariable",
    "-warningvariable",
    "-informationvariable",
    "-outvariable",
    "-outbuffer",
    "-pipelinevariable",
];

/// Combined set of all common parameters (stored lowercase with leading dash).
pub static COMMON_PARAMETERS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for s in COMMON_SWITCHES {
        set.insert(*s);
    }
    for p in COMMON_VALUE_PARAMS {
        set.insert(*p);
    }
    set
});
