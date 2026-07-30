use super::args::HiddenRunSupervisorArgs;
use crate::process::{hidden_supervisor_probe_dispatch, run_hidden_supervisor};
use crate::{PulseError, Result};
use std::path::Path;

pub(crate) fn handle_hidden_supervisor(
    repo_root: &Path,
    args: HiddenRunSupervisorArgs,
) -> Result<()> {
    if args.probe {
        let probe = hidden_supervisor_probe_dispatch(&args.control)?;
        let output = serde_json::to_string_pretty(&probe).map_err(PulseError::from)?;
        println!("{output}");
        return Ok(());
    }
    run_hidden_supervisor(repo_root, &args.control)
}
