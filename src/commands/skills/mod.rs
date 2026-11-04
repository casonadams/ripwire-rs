//! Agent skill installation: `--add-skills` and the `skills` subcommand.
use super::{Command, SharedOption, run_named_subcommand};
use crate::cli::Cli;
use clap::{Args, Command as ClapCommand};
use std::io;

/// Top-level skills installation flags.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct SkillsFlags {
    /// Install ripwire-rs agent skills to ~/.agents/skills (or custom path)
    #[arg(
        long = "add-skills",
        visible_alias = "install-skills",
        value_name = "DIR"
    )]
    pub add_skills: Option<Option<String>>,
}

/// `skills` subcommand: install the bundled agent skills.
#[derive(Args, Debug, Default, PartialEq)]
pub struct SkillsArgs {
    /// Target directory (default: ~/.agents/skills)
    #[arg(value_name = "DIR")]
    pub path: Option<String>,
}

pub struct Skills;
pub static SKILLS: Skills = Skills;

impl Command for Skills {
    fn name(&self) -> &'static str {
        "skills"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn augment(&self, cmd: ClapCommand) -> ClapCommand {
        SkillsFlags::augment_args(cmd)
    }

    fn subcommands(&self) -> Vec<ClapCommand> {
        vec![
            SkillsArgs::augment_args(ClapCommand::new("skills"))
                .about("Manage and install ripwire-rs agent skills"),
        ]
    }

    fn run(&self, cli: &Cli, _root: &str) -> Option<io::Result<()>> {
        let flags = cli.plugins.decode::<SkillsFlags>()?;
        let target = flags.add_skills.as_ref()?;
        Some(execute_skills_install(target.as_deref()))
    }

    fn run_subcommand(
        &self,
        _cli: &Cli,
        name: &str,
        matches: &clap::ArgMatches,
        _root: &str,
    ) -> Option<io::Result<()>> {
        run_named_subcommand::<SkillsArgs>(name, "skills", matches, |args| {
            execute_skills_install(args.path.as_deref())
        })
    }
}

pub fn execute_skills_install(target: Option<&str>) -> io::Result<()> {
    let report = crate::skills::install_skills(target)?;
    println!(
        "Installed {} ripwire-rs agent skills to {}",
        report.installed_count,
        report.destination.display()
    );
    println!(
        "Active manifest written to {}",
        report.destination.join(".ripwire-rs-manifest-v1").display()
    );
    for skill in &report.skills {
        println!("  - {skill}");
    }
    Ok(())
}

