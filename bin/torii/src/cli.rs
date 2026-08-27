//! CLI for Torii.
//!
//! Use a `Cli` struct to parse the CLI arguments
//! and to have flexibility in the future to add more commands
//! that may not start Torii directly.
use clap::Parser;
use torii_cli::ToriiArgs;

#[derive(Parser)]
#[command(name = "torii", author, version = env!("TORII_VERSION_SPEC"), about, long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub args: ToriiArgs,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn version_uses_the_underware_build_specification() {
        let version = Cli::command()
            .get_version()
            .expect("torii has a version")
            .to_owned();

        assert!(version.contains("-uw (base torii "));
        assert!(version.contains(&format!("base torii v{}", env!("CARGO_PKG_VERSION"))));
        assert!(!version.contains("base torii unknown"));
    }
}
