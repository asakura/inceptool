//! Which concrete implementation of a command is the classification target — see [`Platform`].

use serde::Deserialize;

use std::fmt;

/// The concrete binary a command models, when more than one real
/// implementation of the same command name exists with a different flag set (GNU vs. BSD vs.
/// busybox coreutils, ...).
///
/// A command name may be declared more than once, each under a different `Platform` — that's
/// how "multiple implementations" is modeled, rather than one conservative union of every
/// implementation's flags. Every existing data file omits this field and gets [`Self::GnuLinux`]
/// by default, so a command that never bothers to declare more than one variant is completely
/// unaffected by platform selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// GNU userland on Linux — this crate's default, and today's only modeled platform for most
    /// commands.
    #[default]
    GnuLinux,
    /// BSD userland (`FreeBSD`, `OpenBSD`, ...).
    Bsd,
    /// macOS's BSD-derived userland.
    MacOs,
    /// `BusyBox`'s combined-binary userland, as found on minimal/embedded Linux systems.
    Busybox,
}

impl fmt::Display for Platform {
    /// Renders as the same `snake_case` spelling a data file's own `platform = "..."` uses, so an
    /// error message naming a `Platform` matches the text a dataset author would search for.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::GnuLinux => "gnu_linux",
            Self::Bsd => "bsd",
            Self::MacOs => "mac_os",
            Self::Busybox => "busybox",
        };

        write!(f, "{name}")
    }
}
