//! Who may do what (0.0.6 permissions). The only code that can produce an
//! [`Approved`].
//!
//! ```compile_fail,E0451
//! // Outside this module an approval cannot be made up; its field is private.
//! fn forge(v: sam_core::action::Validated) -> sam_core::policy::Approved {
//!     sam_core::policy::Approved { validated: v }
//! }
//! ```
//!
//! ```compile_fail,E0451
//! // Nor can a validation be skipped.
//! fn forge(i: sam_core::intent::Intent, s: sam_core::action::ActionSpec) -> sam_core::action::Validated {
//!     sam_core::action::Validated { intent: i, spec: s }
//! }
//! ```
//!
//! The model proposes. The runtime decides: a model-sourced intent
//! ([`Source::Model`](crate::intent::Source)) travels this exact path, and
//! no output shape can skip it.

use crate::action::{ActionSpec, Confirmation, Permission, Validated};
use crate::intent::Intent;

/// Permission levels, independent of the capability tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// L0: search and preview only.
    ReadOnly,
    /// L1: SAM proposes; the user's pick is the approval.
    Suggest,
    /// L2: SAM may propose on its own; everything it proposes needs a yes.
    Confirm,
    /// L3: SAM's own `Safe` proposals run; `Confirm` ones still ask.
    Trusted,
    /// L4: everything runs.
    Autonomous,
}

impl Level {
    pub const ALL: [Level; 5] = [
        Level::ReadOnly,
        Level::Suggest,
        Level::Confirm,
        Level::Trusted,
        Level::Autonomous,
    ];
}

/// The level SAM Core runs at.
pub const CORE_LEVEL: Level = Level::Suggest;

/// Whether a person picked this, or it arose by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// As-you-type, or a model's own unpicked proposal.
    Automatic,
    /// Enter or a click on this result.
    UserSelected,
    /// A transcript of what was said (§16). It never approves a side
    /// effect by itself (F32).
    Spoken,
}

/// What the table says about one combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Approve,
    Confirm,
    Deny,
}

/// The 0.0.6 table, as a pure function.
///
/// `ambiguous` is true when the proposal needs a human reading: model-
/// sourced, low confidence, or destructive-looking arguments. A fully
/// visible typed gesture (a picked row, a `>command` the user typed) is
/// unambiguous.
pub fn rule(
    level: Level,
    origin: Origin,
    permission: Permission,
    confirmation: Confirmation,
    ambiguous: bool,
) -> Rule {
    use Level as L;
    use Origin::*;
    use Permission as P;
    // Looks never change anything: approved everywhere, however they arose.
    if permission == P::Read {
        return Rule::Approve;
    }
    match (level, origin) {
        (L::ReadOnly, _) => Rule::Deny,
        (L::Autonomous, _) => Rule::Approve,
        // What was heard is a guess about what was said, so it never
        // approves a side effect by itself (F32). The yes comes from the
        // panel's countdown, or from an Enter.
        (_, Spoken) => Rule::Confirm,
        (L::Suggest, Automatic) => Rule::Deny,
        (L::Confirm, Automatic) => Rule::Confirm,
        _ => {
            // Destroying data or acting on the machine itself always asks
            // below L4, however the intent arose.
            if permission == P::Destructive || permission == P::Privileged {
                return Rule::Confirm;
            }
            match confirmation {
                Confirmation::Always => Rule::Confirm,
                Confirmation::WhenAmbiguous if ambiguous => Rule::Confirm,
                _ => Rule::Approve,
            }
        }
    }
}

/// An intent that may run now. Consumed by the executor; it cannot be copied.
#[derive(Debug)]
pub struct Approved {
    validated: Validated,
}

impl Approved {
    pub fn intent(&self) -> &Intent {
        self.validated.intent()
    }

    pub fn spec(&self) -> &ActionSpec {
        self.validated.spec()
    }

    pub fn action(&self) -> &str {
        self.validated.spec().id
    }

    pub fn text(&self, arg: &str) -> Option<&str> {
        self.intent().text(arg)
    }

    pub fn number(&self, arg: &str) -> Option<f64> {
        self.intent().number(arg)
    }
}

/// An intent waiting for the user's yes.
#[derive(Debug)]
pub struct Pending {
    validated: Validated,
}

impl Pending {
    pub fn intent(&self) -> &Intent {
        self.validated.intent()
    }

    pub fn spec(&self) -> &ActionSpec {
        self.validated.spec()
    }

    /// The user said yes. Only the panel calls this, on a second Enter (§2).
    pub fn confirm(self) -> Approved {
        Approved {
            validated: self.validated,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Denial {
    pub action: String,
    pub level: Level,
    pub origin: Origin,
    pub permission: Permission,
}

impl std::fmt::Display for Denial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({:?}) is not allowed at {:?} from {:?}",
            self.action, self.permission, self.level, self.origin
        )
    }
}

#[derive(Debug)]
pub enum Decision {
    Approved(Approved),
    NeedsConfirmation(Pending),
    Denied(Denial),
}

/// Unambiguous proposals: picked rows, typed gestures. The panel and CLI
/// call this; behaviour is identical to before 0.0.6.
pub fn decide(level: Level, validated: Validated, origin: Origin) -> Decision {
    decide_with(level, validated, origin, false)
}

/// Proposals that may need a human reading: model-sourced intents, low
/// confidence, destructive-looking arguments. The model cannot bypass
/// permission by shaping its output: ambiguity only ever adds confirmation.
pub fn decide_with(
    level: Level,
    validated: Validated,
    origin: Origin,
    ambiguous: bool,
) -> Decision {
    let spec = validated.spec();
    match rule(level, origin, spec.permission, spec.confirmation, ambiguous) {
        Rule::Approve => Decision::Approved(Approved { validated }),
        Rule::Confirm => Decision::NeedsConfirmation(Pending { validated }),
        Rule::Deny => Decision::Denied(Denial {
            action: validated.spec().id.to_owned(),
            level,
            origin,
            permission: validated.spec().permission,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Registry;
    use crate::intent::Source;
    use Level as L;
    use Origin::{Automatic as Auto, Spoken as Said, UserSelected as User};
    use Permission::{Destructive as X, Execute as E, LowRisk as S, Privileged as P, Read as R};
    use Rule::{Approve as A, Confirm as C, Deny as D};

    use Confirmation::{Always as Al, Never as Nv, WhenAmbiguous as Am};

    #[test]
    fn the_table() {
        // (level, origin, permission, confirmation, ambiguous) → rule.
        let table = [
            // Reads approve everywhere, however they arose.
            (L::ReadOnly, Auto, R, Nv, false, A),
            (L::ReadOnly, User, R, Nv, false, A),
            (L::Suggest, Said, R, Nv, false, A),
            // L0 denies every side effect.
            (L::ReadOnly, User, S, Nv, false, D),
            (L::ReadOnly, User, P, Al, false, D),
            // L4 approves everything.
            (L::Autonomous, Auto, X, Al, true, A),
            (L::Autonomous, Said, P, Al, true, A),
            // Nothing typed runs unpicked; a level that may propose asks.
            (L::Suggest, Auto, S, Nv, false, D),
            (L::Confirm, Auto, S, Nv, false, C),
            // A picked low-risk action runs.
            (L::Suggest, User, S, Nv, false, A),
            (L::Confirm, User, S, Nv, false, A),
            (L::Trusted, Auto, S, Nv, false, A),
            // Privileged and destructive always ask below L4.
            (L::Suggest, User, P, Al, false, C),
            (L::Trusted, User, P, Al, false, C),
            (L::Trusted, User, X, Al, false, C),
            // Execute/WhenAmbiguous: the typed gesture runs, the proposal asks.
            (L::Suggest, User, E, Am, false, A),
            (L::Suggest, User, E, Am, true, C),
            (L::Suggest, Auto, E, Am, true, D),
            // Always asks, however unambiguous.
            (L::Suggest, User, S, Al, false, C),
        ];
        for (level, origin, perm, conf, ambiguous, want) in table {
            assert_eq!(
                rule(level, origin, perm, conf, ambiguous),
                want,
                "{level:?} {origin:?} {perm:?} {conf:?} ambiguous={ambiguous}"
            );
        }
    }

    #[test]
    fn higher_levels_never_allow_less() {
        let rank = |r: Rule| match r {
            D => 0,
            C => 1,
            A => 2,
        };
        for origin in [Auto, User, Said] {
            for perm in [R, S, E, X, P] {
                for pair in Level::ALL.windows(2) {
                    // Reads approve everywhere, so they carry no signal.
                    if perm == R {
                        continue;
                    }
                    assert!(
                        rank(rule(pair[0], origin, perm, Nv, false))
                            <= rank(rule(pair[1], origin, perm, Nv, false))
                    );
                }
            }
        }
    }

    fn validated(action: &str) -> Validated {
        let intent = Intent::new(action, Source::Grammar);
        Registry::core().validate(intent).unwrap()
    }

    #[test]
    fn what_was_said_never_approves_itself() {
        // F32: a spoken Safe action waits for the panel's countdown, and a
        // spoken Privileged one waits for a real Enter.
        for action in ["system.lock", "system.suspend"] {
            assert!(
                matches!(
                    decide(CORE_LEVEL, validated(action), Said),
                    Decision::NeedsConfirmation(_)
                ),
                "{action} was not left pending"
            );
        }
        // Reading is still free, spoken or not.
        let search = Registry::core()
            .validate(Intent::new("search.files", Source::Grammar).arg("query", "x"))
            .unwrap();
        assert!(matches!(
            decide(CORE_LEVEL, search, Said),
            Decision::Approved(_)
        ));
        // And a pending one is the same yes the panel already knows how to
        // give.
        let Decision::NeedsConfirmation(pending) = decide(CORE_LEVEL, validated("system.lock"), Said)
        else {
            panic!("expected a pending")
        };
        assert_eq!(pending.spec().permission, Permission::LowRisk);
        assert_eq!(pending.confirm().action(), "system.lock");
    }

    #[test]
    fn core_runs_what_the_user_picks_and_asks_before_suspending() {
        assert!(matches!(
            decide(CORE_LEVEL, validated("system.lock"), User),
            Decision::Approved(_)
        ));
        assert!(matches!(
            decide(CORE_LEVEL, validated("system.lock"), Auto),
            Decision::Denied(_)
        ));
        let Decision::NeedsConfirmation(pending) =
            decide(CORE_LEVEL, validated("system.suspend"), User)
        else {
            panic!("suspend must ask");
        };
        assert_eq!(pending.spec().id, "system.suspend");
        let approved = pending.confirm();
        assert_eq!(approved.action(), "system.suspend");
    }

    #[test]
    fn nothing_typed_is_sent_unless_picked() {
        let action = || validated("system.lock");
        assert!(matches!(
            decide(CORE_LEVEL, action(), Auto),
            Decision::Denied(_)
        ));
        assert!(matches!(
            decide(CORE_LEVEL, action(), User),
            Decision::Approved(_)
        ));
    }

    #[test]
    fn a_denial_says_why() {
        let Decision::Denied(d) = decide(L::ReadOnly, validated("system.lock"), User) else {
            panic!()
        };
        assert_eq!(
            d.to_string(),
            "system.lock (LowRisk) is not allowed at ReadOnly from UserSelected"
        );
    }
}
