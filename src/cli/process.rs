use super::args::HiddenRunSupervisorArgs;
use crate::process::hidden_supervisor_probe_dispatch;
use crate::{PulseError, Result};

pub(crate) fn handle_hidden_supervisor(args: HiddenRunSupervisorArgs) -> Result<()> {
    if !args.probe {
        return Err(PulseError::validation(
            "run_supervisor_spawn_failed",
            "hidden run supervisor is wired for Slice 3 I0 feasibility probe only",
        ));
    }
    let probe = hidden_supervisor_probe_dispatch(&args.control)?;
    let output = serde_json::to_string_pretty(&probe).map_err(PulseError::from)?;
    println!("{output}");
    Ok(())
}
