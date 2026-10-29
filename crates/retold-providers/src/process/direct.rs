//! Direct launch for hosts without the Unix lifetime-connection supervisor.
use super::Group;
use std::io;
use std::path::Path;
use tokio::process::{Child, Command};

pub(super) struct Launch {
    pub command: Command,
}

pub(super) struct Spawned {
    pub child: Child,
    pub group: Group,
    pub lease: Option<()>,
}

impl Launch {
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the common launch interface is fallible on Unix"
    )]
    pub fn new(program: &Path, args: &[String]) -> io::Result<Self> {
        let mut command = Command::new(program);
        command.args(args).kill_on_drop(true);
        Ok(Self { command })
    }

    pub async fn spawn(mut self) -> io::Result<Spawned> {
        let child = self.command.spawn()?;
        let group = Group::new(child.id());
        Ok(Spawned {
            child,
            group,
            lease: Option::default(),
        })
    }
}
