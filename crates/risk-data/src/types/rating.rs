//! The risk-axis and rule-effect vocabulary shared by every rule type — see [`ProfilePatch`] and
//! [`Effect`].
//!
//! Doubles as the runtime shape [`crate::entry::CommandEntry`] and its siblings are built from:
//! the same `TrustImpact`/`Reversibility`/`BlastRadius`/`Disclosure`/`Persistence`/`Privilege`/
//! `Auditability`/`Exposure`/`Verification`/`ProfilePatch` definitions back both the
//! TOML-deserialization schema (`crate::types::Command`'s `baseline`/flags/rules) and the
//! `phf`-codegen'd lookup table [`crate::generate_command_table`] renders, so there's one
//! definition of "what a risk axis is" rather than two kept in sync by convention.

use serde::Deserialize;

/// A patch to a `Command`'s (or [`crate::entry::CommandEntry`]'s) risk axes.
///
/// One independent setting per axis, all optional, so a rule can patch only the axes it
/// actually has an opinion about.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfilePatch {
    /// Does this extend what the script trusts (arbitrary/uncontrolled code execution)?
    pub trust: Option<TrustImpact>,
    /// Can this be undone afterward?
    pub reversibility: Option<Reversibility>,
    /// How much of the system can one invocation affect?
    pub blast_radius: Option<BlastRadius>,
    /// Does this reveal information the script shouldn't?
    pub disclosure: Option<Disclosure>,
    /// Does this outlive the current invocation/session?
    pub persistence: Option<Persistence>,
    /// Does this change what identity/privilege level the script operates as?
    pub privilege: Option<Privilege>,
    /// Does this destroy or disable evidence of what happened?
    pub auditability: Option<Auditability>,
    /// Does this open a new path for something outside to reach in?
    pub exposure: Option<Exposure>,
    /// Does this skip a check whose entire job is catching a problem before it causes harm?
    pub verification: Option<Verification>,
}

impl ProfilePatch {
    /// A patch that changes nothing — equivalent to [`Default::default`], but `const` so
    /// generated `phf`-table source (built from a `static` initializer, which can't call
    /// `Default::default` — that's a trait method, not a `const fn`) can spell it directly.
    pub const EMPTY: Self = Self {
        trust: None,
        reversibility: None,
        blast_radius: None,
        disclosure: None,
        persistence: None,
        privilege: None,
        auditability: None,
        exposure: None,
        verification: None,
    };

    /// Whether every axis is unset — a flag/combo/operand rule declared only so something else
    /// can reference it (e.g. a combo rule's `requires`), carrying no risk by itself. Callers
    /// folding patches into a running profile skip recording a cause for an empty patch.
    #[must_use = "checking emptiness has no effect unless the caller uses the result"]
    pub const fn is_empty(self) -> bool {
        self.trust.is_none()
            && self.reversibility.is_none()
            && self.blast_radius.is_none()
            && self.disclosure.is_none()
            && self.persistence.is_none()
            && self.privilege.is_none()
            && self.auditability.is_none()
            && self.exposure.is_none()
            && self.verification.is_none()
    }
}

/// Whether a command/flag/rule extends what the script trusts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustImpact {
    /// Acts only with the trust the script already had.
    None,
    /// Hands execution off to another explicitly-named command (`caffeinate`, `env`, `arch`)
    /// without itself interpreting an arbitrary string as code.
    DelegatesExecution,
    /// Executes arbitrary or uncontrolled code (`eval`, `source`, dynamically loaded builtins,
    /// `docker run --privileged`, ...).
    ArbitraryExecution,
}

impl TrustImpact {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::ArbitraryExecution, _) | (_, Self::ArbitraryExecution) => {
                Self::ArbitraryExecution
            }
            (Self::DelegatesExecution, _) | (_, Self::DelegatesExecution) => {
                Self::DelegatesExecution
            }
            (Self::None, Self::None) => Self::None,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::None, _) | (_, Self::None) => Self::None,
            (Self::DelegatesExecution, Self::DelegatesExecution | Self::ArbitraryExecution)
            | (Self::ArbitraryExecution, Self::DelegatesExecution) => Self::DelegatesExecution,
            (Self::ArbitraryExecution, Self::ArbitraryExecution) => Self::ArbitraryExecution,
        }
    }
}

/// Whether a command/flag/rule's effect can be undone afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    /// Mistakes can be fixed (a file can be rewritten, a process restarted).
    Reversible,
    /// Undoing takes real effort — a backup restore, the reflog, a trash/undelete — but the
    /// state isn't gone for good.
    Recoverable,
    /// There's no undo (`SIGKILL`, `rm -rf`, `history -c`, ...).
    Irreversible,
}

impl Reversibility {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Irreversible, _) | (_, Self::Irreversible) => Self::Irreversible,
            (Self::Recoverable, _) | (_, Self::Recoverable) => Self::Recoverable,
            (Self::Reversible, Self::Reversible) => Self::Reversible,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Reversible, _) | (_, Self::Reversible) => Self::Reversible,
            (Self::Recoverable, Self::Recoverable | Self::Irreversible)
            | (Self::Irreversible, Self::Recoverable) => Self::Recoverable,
            (Self::Irreversible, Self::Irreversible) => Self::Irreversible,
        }
    }
}

/// How much of the system one invocation can affect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlastRadius {
    /// Confined to a specific, named target.
    Narrow,
    /// Affects a named group broader than one target (a directory subtree, every member of a
    /// security group) without reaching the entire system.
    Moderate,
    /// Can affect far more than what's named (every process the caller owns, an entire
    /// filesystem, ...).
    Broad,
}

impl BlastRadius {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Broad, _) | (_, Self::Broad) => Self::Broad,
            (Self::Moderate, _) | (_, Self::Moderate) => Self::Moderate,
            (Self::Narrow, Self::Narrow) => Self::Narrow,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Narrow, _) | (_, Self::Narrow) => Self::Narrow,
            (Self::Moderate, Self::Moderate | Self::Broad) | (Self::Broad, Self::Moderate) => {
                Self::Moderate
            }
            (Self::Broad, Self::Broad) => Self::Broad,
        }
    }
}

/// Whether a command/flag/rule reveals information the script shouldn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disclosure {
    /// Reveals nothing beyond what the script already had access to.
    None,
    /// Reveals data that wasn't otherwise exposed (file contents, configuration, ...).
    DisclosesData,
    /// Reveals credentials or secrets (API keys, tokens, passwords, ...) — typically a path to
    /// further compromise, not just an information leak.
    DisclosesCredentials,
}

impl Disclosure {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::DisclosesCredentials, _) | (_, Self::DisclosesCredentials) => {
                Self::DisclosesCredentials
            }
            (Self::DisclosesData, _) | (_, Self::DisclosesData) => Self::DisclosesData,
            (Self::None, Self::None) => Self::None,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::None, _) | (_, Self::None) => Self::None,
            (Self::DisclosesData, Self::DisclosesData | Self::DisclosesCredentials)
            | (Self::DisclosesCredentials, Self::DisclosesData) => Self::DisclosesData,
            (Self::DisclosesCredentials, Self::DisclosesCredentials) => Self::DisclosesCredentials,
        }
    }
}

/// Whether a command/flag/rule's effect outlives the current invocation/session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    /// Scoped to the current invocation/session — naturally undone by a reboot or a new shell.
    Ephemeral,
    /// Outlives the immediate invocation but not the machine's lifetime (survives until logout
    /// or reboot, an in-memory daemon, a temporary cron job).
    SessionScoped,
    /// Outlives the current invocation/session (a config file, a registered daemon, a
    /// permanently-set kernel parameter, ...) until something explicitly reverts it.
    Persistent,
}

impl Persistence {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Persistent, _) | (_, Self::Persistent) => Self::Persistent,
            (Self::SessionScoped, _) | (_, Self::SessionScoped) => Self::SessionScoped,
            (Self::Ephemeral, Self::Ephemeral) => Self::Ephemeral,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Ephemeral, _) | (_, Self::Ephemeral) => Self::Ephemeral,
            (Self::SessionScoped, Self::SessionScoped | Self::Persistent)
            | (Self::Persistent, Self::SessionScoped) => Self::SessionScoped,
            (Self::Persistent, Self::Persistent) => Self::Persistent,
        }
    }
}

/// Whether a command/flag/rule changes what identity/privilege level the script operates as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privilege {
    /// Operates as whatever identity the script already had.
    None,
    /// Switches to another specific, non-root identity (an assumed IAM role, a different
    /// regular user) without gaining superuser/admin rights.
    Delegated,
    /// Switches to a higher-privileged identity (`sudo`, `su`, ...).
    Elevated,
}

impl Privilege {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Elevated, _) | (_, Self::Elevated) => Self::Elevated,
            (Self::Delegated, _) | (_, Self::Delegated) => Self::Delegated,
            (Self::None, Self::None) => Self::None,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::None, _) | (_, Self::None) => Self::None,
            (Self::Delegated, Self::Delegated | Self::Elevated)
            | (Self::Elevated, Self::Delegated) => Self::Delegated,
            (Self::Elevated, Self::Elevated) => Self::Elevated,
        }
    }
}

/// Whether a command/flag/rule destroys or disables evidence of what happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Auditability {
    /// Whatever record-keeping exists is left intact.
    Intact,
    /// Weakens or narrows what's recorded (lower verbosity, redaction, disabling one log
    /// stream) without destroying the record outright.
    Reduced,
    /// Destroys or disables a record of what happened (`history -c`, disabling cloud audit
    /// logs, ...).
    Tampered,
}

impl Auditability {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Tampered, _) | (_, Self::Tampered) => Self::Tampered,
            (Self::Reduced, _) | (_, Self::Reduced) => Self::Reduced,
            (Self::Intact, Self::Intact) => Self::Intact,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Intact, _) | (_, Self::Intact) => Self::Intact,
            (Self::Reduced, Self::Reduced | Self::Tampered) | (Self::Tampered, Self::Reduced) => {
                Self::Reduced
            }
            (Self::Tampered, Self::Tampered) => Self::Tampered,
        }
    }
}

/// Whether a command/flag/rule opens a new path for something outside to reach in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure {
    /// Stays confined to its existing boundary (loopback, an existing namespace, ...).
    Contained,
    /// Reachable from other hosts on the same private network or VPC, but not the public
    /// internet.
    PeerReachable,
    /// Opens a path reachable from outside that boundary (a public bind, an opened firewall
    /// rule, ...).
    NetworkReachable,
}

impl Exposure {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::NetworkReachable, _) | (_, Self::NetworkReachable) => Self::NetworkReachable,
            (Self::PeerReachable, _) | (_, Self::PeerReachable) => Self::PeerReachable,
            (Self::Contained, Self::Contained) => Self::Contained,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Contained, _) | (_, Self::Contained) => Self::Contained,
            (Self::PeerReachable, Self::PeerReachable | Self::NetworkReachable)
            | (Self::NetworkReachable, Self::PeerReachable) => Self::PeerReachable,
            (Self::NetworkReachable, Self::NetworkReachable) => Self::NetworkReachable,
        }
    }
}

/// Whether a command/flag/rule skips a check whose entire job is catching a problem before it
/// causes harm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    /// The check still runs.
    Checked,
    /// The check still runs but in a reduced form — downgraded from hard-fail to a warning, or
    /// validating less than the full check would.
    Weakened,
    /// The check is skipped (`curl -k`, `git commit --no-verify`, ...).
    Bypassed,
}

impl Verification {
    /// Folds `other` in via a per-axis maximum — the worse of the two.
    #[must_use = "escalating has no effect unless the caller uses the result"]
    pub const fn escalate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Bypassed, _) | (_, Self::Bypassed) => Self::Bypassed,
            (Self::Weakened, _) | (_, Self::Weakened) => Self::Weakened,
            (Self::Checked, Self::Checked) => Self::Checked,
        }
    }

    /// Caps `self` at `cap` via a per-axis minimum — the better of the two.
    #[must_use = "mitigating has no effect unless the caller uses the result"]
    pub const fn mitigate(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Checked, _) | (_, Self::Checked) => Self::Checked,
            (Self::Weakened, Self::Weakened | Self::Bypassed)
            | (Self::Bypassed, Self::Weakened) => Self::Weakened,
            (Self::Bypassed, Self::Bypassed) => Self::Bypassed,
        }
    }
}

/// Whether a rule makes a command's rating worse or better than it would otherwise be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Raises the result: folded in via a per-axis maximum with every other escalating rule.
    Escalate,
    /// Caps the result: folded in via a per-axis minimum, applied after every escalation, so a
    /// mitigating flag always wins regardless of declaration order.
    Mitigate,
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    mod trust_impact {
        use super::*;

        #[rstest]
        #[case::keeps_none_when_both_none(TrustImpact::None, TrustImpact::None, TrustImpact::None)]
        #[case::escalates_to_delegates_when_either_side_delegates(
            TrustImpact::DelegatesExecution,
            TrustImpact::None,
            TrustImpact::DelegatesExecution
        )]
        #[case::escalates_to_arbitrary_execution_when_either_side_is(
            TrustImpact::ArbitraryExecution,
            TrustImpact::DelegatesExecution,
            TrustImpact::ArbitraryExecution
        )]
        fn escalate(
            #[case] this: TrustImpact,
            #[case] other: TrustImpact,
            #[case] expected: TrustImpact,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_none_when_either_side_is_none(
            TrustImpact::ArbitraryExecution,
            TrustImpact::None,
            TrustImpact::None
        )]
        #[case::caps_arbitrary_execution_down_to_delegates(
            TrustImpact::ArbitraryExecution,
            TrustImpact::DelegatesExecution,
            TrustImpact::DelegatesExecution
        )]
        #[case::keeps_arbitrary_execution_when_both_sides_are(
            TrustImpact::ArbitraryExecution,
            TrustImpact::ArbitraryExecution,
            TrustImpact::ArbitraryExecution
        )]
        fn mitigate(
            #[case] this: TrustImpact,
            #[case] cap: TrustImpact,
            #[case] expected: TrustImpact,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod reversibility {
        use super::*;

        #[rstest]
        #[case::keeps_reversible_when_both_sides_are(
            Reversibility::Reversible,
            Reversibility::Reversible,
            Reversibility::Reversible
        )]
        #[case::escalates_to_recoverable_when_either_side_is(
            Reversibility::Recoverable,
            Reversibility::Reversible,
            Reversibility::Recoverable
        )]
        #[case::escalates_to_irreversible_when_either_side_is(
            Reversibility::Irreversible,
            Reversibility::Recoverable,
            Reversibility::Irreversible
        )]
        fn escalate(
            #[case] this: Reversibility,
            #[case] other: Reversibility,
            #[case] expected: Reversibility,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_reversible_when_either_side_is(
            Reversibility::Irreversible,
            Reversibility::Reversible,
            Reversibility::Reversible
        )]
        #[case::caps_irreversible_down_to_recoverable(
            Reversibility::Irreversible,
            Reversibility::Recoverable,
            Reversibility::Recoverable
        )]
        #[case::keeps_irreversible_when_both_sides_are(
            Reversibility::Irreversible,
            Reversibility::Irreversible,
            Reversibility::Irreversible
        )]
        fn mitigate(
            #[case] this: Reversibility,
            #[case] cap: Reversibility,
            #[case] expected: Reversibility,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod blast_radius {
        use super::*;

        #[rstest]
        #[case::keeps_narrow_when_both_sides_are(
            BlastRadius::Narrow,
            BlastRadius::Narrow,
            BlastRadius::Narrow
        )]
        #[case::escalates_to_moderate_when_either_side_is(
            BlastRadius::Moderate,
            BlastRadius::Narrow,
            BlastRadius::Moderate
        )]
        #[case::escalates_to_broad_when_either_side_is(
            BlastRadius::Broad,
            BlastRadius::Moderate,
            BlastRadius::Broad
        )]
        fn escalate(
            #[case] this: BlastRadius,
            #[case] other: BlastRadius,
            #[case] expected: BlastRadius,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_narrow_when_either_side_is(
            BlastRadius::Broad,
            BlastRadius::Narrow,
            BlastRadius::Narrow
        )]
        #[case::caps_broad_down_to_moderate(
            BlastRadius::Broad,
            BlastRadius::Moderate,
            BlastRadius::Moderate
        )]
        #[case::keeps_broad_when_both_sides_are(
            BlastRadius::Broad,
            BlastRadius::Broad,
            BlastRadius::Broad
        )]
        fn mitigate(
            #[case] this: BlastRadius,
            #[case] cap: BlastRadius,
            #[case] expected: BlastRadius,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod disclosure {
        use super::*;

        #[rstest]
        #[case::keeps_none_when_both_none(Disclosure::None, Disclosure::None, Disclosure::None)]
        #[case::escalates_to_data_when_either_side_discloses_data(
            Disclosure::DisclosesData,
            Disclosure::None,
            Disclosure::DisclosesData
        )]
        #[case::escalates_to_credentials_when_either_side_discloses_credentials(
            Disclosure::DisclosesCredentials,
            Disclosure::DisclosesData,
            Disclosure::DisclosesCredentials
        )]
        fn escalate(
            #[case] this: Disclosure,
            #[case] other: Disclosure,
            #[case] expected: Disclosure,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_none_when_either_side_is_none(
            Disclosure::DisclosesCredentials,
            Disclosure::None,
            Disclosure::None
        )]
        #[case::caps_credentials_down_to_data(
            Disclosure::DisclosesCredentials,
            Disclosure::DisclosesData,
            Disclosure::DisclosesData
        )]
        #[case::keeps_credentials_when_both_sides_are(
            Disclosure::DisclosesCredentials,
            Disclosure::DisclosesCredentials,
            Disclosure::DisclosesCredentials
        )]
        fn mitigate(
            #[case] this: Disclosure,
            #[case] cap: Disclosure,
            #[case] expected: Disclosure,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod persistence {
        use super::*;

        #[rstest]
        #[case::keeps_ephemeral_when_both_sides_are(
            Persistence::Ephemeral,
            Persistence::Ephemeral,
            Persistence::Ephemeral
        )]
        #[case::escalates_to_session_scoped_when_either_side_is(
            Persistence::SessionScoped,
            Persistence::Ephemeral,
            Persistence::SessionScoped
        )]
        #[case::escalates_to_persistent_when_either_side_is(
            Persistence::Persistent,
            Persistence::SessionScoped,
            Persistence::Persistent
        )]
        fn escalate(
            #[case] this: Persistence,
            #[case] other: Persistence,
            #[case] expected: Persistence,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_ephemeral_when_either_side_is(
            Persistence::Persistent,
            Persistence::Ephemeral,
            Persistence::Ephemeral
        )]
        #[case::caps_persistent_down_to_session_scoped(
            Persistence::Persistent,
            Persistence::SessionScoped,
            Persistence::SessionScoped
        )]
        #[case::keeps_persistent_when_both_sides_are(
            Persistence::Persistent,
            Persistence::Persistent,
            Persistence::Persistent
        )]
        fn mitigate(
            #[case] this: Persistence,
            #[case] cap: Persistence,
            #[case] expected: Persistence,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod privilege {
        use super::*;

        #[rstest]
        #[case::keeps_none_when_both_sides_are(Privilege::None, Privilege::None, Privilege::None)]
        #[case::escalates_to_delegated_when_either_side_is(
            Privilege::Delegated,
            Privilege::None,
            Privilege::Delegated
        )]
        #[case::escalates_to_elevated_when_either_side_is(
            Privilege::Elevated,
            Privilege::Delegated,
            Privilege::Elevated
        )]
        fn escalate(
            #[case] this: Privilege,
            #[case] other: Privilege,
            #[case] expected: Privilege,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_none_when_either_side_is(
            Privilege::Elevated,
            Privilege::None,
            Privilege::None
        )]
        #[case::caps_elevated_down_to_delegated(
            Privilege::Elevated,
            Privilege::Delegated,
            Privilege::Delegated
        )]
        #[case::keeps_elevated_when_both_sides_are(
            Privilege::Elevated,
            Privilege::Elevated,
            Privilege::Elevated
        )]
        fn mitigate(#[case] this: Privilege, #[case] cap: Privilege, #[case] expected: Privilege) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod auditability {
        use super::*;

        #[rstest]
        #[case::keeps_intact_when_both_sides_are(
            Auditability::Intact,
            Auditability::Intact,
            Auditability::Intact
        )]
        #[case::escalates_to_reduced_when_either_side_is(
            Auditability::Reduced,
            Auditability::Intact,
            Auditability::Reduced
        )]
        #[case::escalates_to_tampered_when_either_side_is(
            Auditability::Tampered,
            Auditability::Reduced,
            Auditability::Tampered
        )]
        fn escalate(
            #[case] this: Auditability,
            #[case] other: Auditability,
            #[case] expected: Auditability,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_intact_when_either_side_is(
            Auditability::Tampered,
            Auditability::Intact,
            Auditability::Intact
        )]
        #[case::caps_tampered_down_to_reduced(
            Auditability::Tampered,
            Auditability::Reduced,
            Auditability::Reduced
        )]
        #[case::keeps_tampered_when_both_sides_are(
            Auditability::Tampered,
            Auditability::Tampered,
            Auditability::Tampered
        )]
        fn mitigate(
            #[case] this: Auditability,
            #[case] cap: Auditability,
            #[case] expected: Auditability,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod exposure {
        use super::*;

        #[rstest]
        #[case::keeps_contained_when_both_sides_are(
            Exposure::Contained,
            Exposure::Contained,
            Exposure::Contained
        )]
        #[case::escalates_to_peer_reachable_when_either_side_is(
            Exposure::PeerReachable,
            Exposure::Contained,
            Exposure::PeerReachable
        )]
        #[case::escalates_to_network_reachable_when_either_side_is(
            Exposure::NetworkReachable,
            Exposure::PeerReachable,
            Exposure::NetworkReachable
        )]
        fn escalate(#[case] this: Exposure, #[case] other: Exposure, #[case] expected: Exposure) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_contained_when_either_side_is(
            Exposure::NetworkReachable,
            Exposure::Contained,
            Exposure::Contained
        )]
        #[case::caps_network_reachable_down_to_peer_reachable(
            Exposure::NetworkReachable,
            Exposure::PeerReachable,
            Exposure::PeerReachable
        )]
        #[case::keeps_network_reachable_when_both_sides_are(
            Exposure::NetworkReachable,
            Exposure::NetworkReachable,
            Exposure::NetworkReachable
        )]
        fn mitigate(#[case] this: Exposure, #[case] cap: Exposure, #[case] expected: Exposure) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }

    mod verification {
        use super::*;

        #[rstest]
        #[case::keeps_checked_when_both_sides_are(
            Verification::Checked,
            Verification::Checked,
            Verification::Checked
        )]
        #[case::escalates_to_weakened_when_either_side_is(
            Verification::Weakened,
            Verification::Checked,
            Verification::Weakened
        )]
        #[case::escalates_to_bypassed_when_either_side_is(
            Verification::Bypassed,
            Verification::Weakened,
            Verification::Bypassed
        )]
        fn escalate(
            #[case] this: Verification,
            #[case] other: Verification,
            #[case] expected: Verification,
        ) {
            assert_eq!(this.escalate(other), expected);
        }

        #[rstest]
        #[case::caps_to_checked_when_either_side_is(
            Verification::Bypassed,
            Verification::Checked,
            Verification::Checked
        )]
        #[case::caps_bypassed_down_to_weakened(
            Verification::Bypassed,
            Verification::Weakened,
            Verification::Weakened
        )]
        #[case::keeps_bypassed_when_both_sides_are(
            Verification::Bypassed,
            Verification::Bypassed,
            Verification::Bypassed
        )]
        fn mitigate(
            #[case] this: Verification,
            #[case] cap: Verification,
            #[case] expected: Verification,
        ) {
            assert_eq!(this.mitigate(cap), expected);
        }
    }
}
