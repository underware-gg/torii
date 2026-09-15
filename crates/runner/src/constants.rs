use std::time::Duration;

use starknet::macros::felt;
use starknet_crypto::Felt;

pub(crate) const LOG_TARGET: &str = "torii:runner";

#[allow(unused)]
pub(crate) const UDC_ADDRESS: Felt =
    felt!("0x041a78e741e5af2fec34b695679bc6891742439f7afb8484ecd7766661ad02bf");

/// How often accumulated histogram samples are folded into the exported summaries.
pub(crate) const METRICS_UPKEEP_INTERVAL: Duration = Duration::from_secs(5);
