// SPDX-License-Identifier: Apache-2.0

//! The safety panel, per ADR-0002 §5–6 and Charter Article IV.
//!
//! Every honesty rule in this project converges here. The governor can be
//! correct, the sysfs layer can refuse to assemble a partial reading, the shed
//! ladder can hold its reserve — and all of it is undone by one panel that says
//! `PROTECTED` on a device where nothing was checked. This is the only module a
//! user actually looks at, so it is the only module where being wrong is
//! guaranteed to reach them.
//!
//! # Every row cites something, including the rows that admit ignorance
//!
//! [`Finding`] has three variants and **all three carry [`Evidence`]**. There is
//! no way to write down "verified" without saying what verified it, and no way
//! to write down "could not check" without saying what stopped it. A panel whose
//! unknowns are blank is a panel that looks calm.
//!
//! # An unverified row can never produce a protected headline
//!
//! [`Panel::overall`] is computed from the rows, never set. Charter Article IV:
//! what cannot be checked is unverified, never clean — so a single
//! [`Finding::Unverified`] is enough to take the headline off
//! [`Overall::Protected`], no matter how many rows around it are green.
//!
//! # Software cannot see a swollen cell, and the type says so
//!
//! [`Confidence`] has no `High` variant, and cannot be given one without
//! changing this file — see the proof on [`SwellingRisk`]. A swelling estimate
//! is assembled from cycle count, capacity fade, thermal history and charge
//! acceptance, none of which measure a millimetre of deformation. The panel
//! renders it as an estimate and then asks the only instrument that can settle
//! it, which is a person with a flat table.

use crate::governor::Level;
use crate::shed::UpsClaim;
use crate::sysfs::Kind;
use crate::tier::Verdict;

/// Why a row says what it says.
///
/// Cannot be blank. A row whose evidence is an empty string renders as a
/// confident-looking claim with nothing behind it, which is the exact shape of
/// the failure this whole module exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence(String);

impl Evidence {
    /// Records what was observed.
    ///
    /// Returns `None` for anything blank, so the refusal happens at
    /// construction rather than at render time, where the only options left are
    /// to print nothing or to invent something.
    #[must_use]
    pub fn new(what: &str) -> Option<Self> {
        if what.trim().is_empty() {
            return None;
        }
        Some(Self(what.trim().to_owned()))
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Evidence {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a check concluded.
///
/// There is deliberately no variant without [`Evidence`]:
///
/// ```compile_fail
/// use vayucell_core::panel::Finding;
/// // There is no evidence-free way to say a thing was verified. The type
/// // annotation is load-bearing: without it this compiles, because a bare
/// // `Finding::Verified` is the tuple constructor — a function value, not a
/// // Finding — and the proof would pass while proving nothing.
/// let f: Finding = Finding::Verified;
/// ```
///
/// And deliberately three variants rather than two. A boolean forces the third
/// case — checked and could not tell — into one of the other two, and the one it
/// gets forced into is always the reassuring one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// Checked, and it holds.
    Verified(Evidence),
    /// Checked, and it does not hold.
    Refused(Evidence),
    /// Could not be checked, and this is what stopped it.
    Unverified(Evidence),
}

impl Finding {
    /// The single word this renders under.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Finding::Verified(_) => "VERIFIED",
            Finding::Refused(_) => "FAILED",
            Finding::Unverified(_) => "UNVERIFIED",
        }
    }

    /// The evidence behind it.
    #[must_use]
    pub const fn evidence(&self) -> &Evidence {
        match self {
            Finding::Verified(e) | Finding::Refused(e) | Finding::Unverified(e) => e,
        }
    }
}

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What was checked.
    pub subject: &'static str,
    /// What came of it.
    pub finding: Finding,
}

impl core::fmt::Display for Row {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:<12} {:<28} {}",
            self.finding.label(),
            self.subject,
            self.finding.evidence()
        )
    }
}

/// The headline.
///
/// Ordered by how much it should worry somebody, so the worst row wins by
/// [`Ord`] rather than by a precedence table somebody has to keep correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Overall {
    /// Everything on the panel was checked and holds.
    Protected,
    /// Something could not be checked. Not a failure, and not protection.
    Unverified,
    /// Something was checked and does not hold.
    Unsafe,
}

impl core::fmt::Display for Overall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Overall::Protected => "PROTECTED",
            Overall::Unverified => "NOT FULLY VERIFIED",
            Overall::Unsafe => "UNSAFE",
        })
    }
}

/// How much weight a swelling estimate can carry.
///
/// There is no `High`, and that is the point — see [`SwellingRisk`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// One or two weak proxies.
    Low,
    /// Several proxies agreeing.
    Moderate,
}

impl core::fmt::Display for Confidence {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Confidence::Low => "low confidence",
            Confidence::Moderate => "moderate confidence",
        })
    }
}

/// An inferred risk that the cell is deforming. ADR-0002 §6.
///
/// Assembled from cycle count against age, capacity fade, internal-resistance
/// drift, accumulated time above 40 °C and falling charge acceptance. Not one of
/// those measures a millimetre of deformation, so the estimate is never more
/// than an estimate — and [`Confidence`] has no `High` variant to promote it
/// with:
///
/// ```compile_fail
/// use vayucell_core::panel::Confidence;
/// // Software cannot be highly confident about a physical dimension it has no
/// // instrument for. There is no variant to write here.
/// let c = Confidence::High;
/// ```
///
/// The type could have carried a percentage instead, and a percentage is what
/// makes this dangerous: `risk: 0.91` reads as a measurement to everyone who
/// sees it, and the number would be an average of proxies with no calibration
/// behind it. A coarse level with an explicit confidence cannot be mistaken for
/// something an instrument produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwellingRisk {
    /// How concerning the proxies look.
    pub level: RiskLevel,
    /// How much the proxies are worth.
    pub confidence: Confidence,
    /// Which proxies contributed.
    pub basis: Vec<&'static str>,
}

/// How concerning the ageing proxies look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// Nothing in the proxies stands out.
    Nominal,
    /// The proxies are drifting the wrong way.
    Elevated,
}

/// The instruction that resolves what software cannot.
///
/// Rendered on **every** panel, at every risk level. Making it conditional on an
/// elevated estimate would mean the prompt is absent in exactly the case the
/// estimate is wrong — and the estimate is built from proxies precisely because
/// the thing it is estimating cannot be measured here.
pub const INSPECTION: &str = "Physical inspection is the definitive check, and \
this software cannot perform it. Put the phone face-down on a flat table. If it \
rocks, does not lie flat, or the screen or back cover is lifting at any edge, \
stop using it now and take it to hazardous-waste handling.";

/// A rendered safety panel.
#[derive(Debug, Clone)]
pub struct Panel {
    rows: Vec<Row>,
    risk: SwellingRisk,
}

impl Panel {
    /// Builds a panel from the state of a node.
    ///
    /// Every argument is required. An `Option` here would let a caller omit the
    /// charge mechanism on a device that has none and get a panel with one fewer
    /// row — quietly turning "this device cannot hold a charge ceiling", which
    /// is the T0 case and the most common device, into a panel that simply does
    /// not mention charge ceilings.
    #[must_use]
    pub fn build(
        tier: &Verdict,
        level: Level,
        mechanism: Option<Kind>,
        ceiling: &Finding,
        ups: &UpsClaim,
        risk: SwellingRisk,
    ) -> Self {
        let mut rows = Vec::new();

        rows.push(Row {
            subject: "device tier",
            finding: match tier {
                Verdict::Established(t) => Finding::Verified(evidence(&format!(
                    "{t:?} established from positive evidence on this device"
                ))),
                Verdict::Unverified(why) => Finding::Unverified(evidence(why)),
                Verdict::Unknown => Finding::Unverified(evidence(
                    "nothing recognised this machine as a target device",
                )),
            },
        });

        rows.push(Row {
            subject: "charge mechanism",
            finding: match mechanism {
                Some(k) => Finding::Verified(evidence(&format!(
                    "{} answered at {}",
                    k.node(),
                    if k.is_ceiling() {
                        "a readable ceiling"
                    } else {
                        "a control that is not a readable ceiling"
                    }
                ))),
                // Not a failure of this software, and not something to soften.
                // On an unrooted stock phone there is no supported way to stop
                // the charger, and a row that went quiet about it would leave
                // the user believing a ceiling is being held.
                None => Finding::Refused(evidence(
                    "this device exposes no charge control, so no ceiling can be held",
                )),
            },
        });

        rows.push(Row {
            subject: "charge ceiling",
            finding: ceiling.clone(),
        });

        rows.push(Row {
            subject: "battery governor",
            finding: match level {
                Level::Normal => {
                    Finding::Verified(evidence("governor at NORMAL; no threshold crossed"))
                }
                other => Finding::Refused(evidence(&format!(
                    "governor at {other}; the workload has been reduced or stopped"
                ))),
            },
        });

        rows.push(Row {
            subject: "outage reserve",
            finding: match ups {
                UpsClaim::Backed { reserve } => Finding::Verified(evidence(&format!(
                    "a cell carries this node and it shuts down holding {reserve}"
                ))),
                UpsClaim::Unbacked { why } => Finding::Refused(evidence(why)),
            },
        });

        Self { rows, risk }
    }

    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The swelling estimate.
    #[must_use]
    pub const fn risk(&self) -> &SwellingRisk {
        &self.risk
    }

    /// The headline, computed from the rows.
    ///
    /// Never stored and never passed in. A headline that could be set
    /// independently of the rows is a headline that will eventually disagree
    /// with them, and the disagreement will be in the reassuring direction
    /// because that is the direction nobody files a bug about.
    #[must_use]
    pub fn overall(&self) -> Overall {
        self.rows
            .iter()
            .map(|r| match r.finding {
                Finding::Verified(_) => Overall::Protected,
                Finding::Unverified(_) => Overall::Unverified,
                Finding::Refused(_) => Overall::Unsafe,
            })
            .max()
            .unwrap_or(Overall::Unverified)
    }

    /// The whole panel as text.
    #[must_use]
    pub fn render(&self) -> String {
        use core::fmt::Write as _;

        let mut out = format!("BATTERY SAFETY: {}\n\n", self.overall());
        for row in &self.rows {
            // Writing into a String cannot fail, and a panel is not a place to
            // start propagating an error that cannot happen.
            let _ = writeln!(out, "  {row}");
        }
        let basis = if self.risk.basis.is_empty() {
            "no proxies at all".to_owned()
        } else {
            self.risk.basis.join(", ")
        };
        let _ = write!(
            out,
            "\nSwelling risk: {:?}, {} — an estimate from {basis}, not a measurement.\n",
            self.risk.level, self.risk.confidence,
        );
        let _ = write!(out, "\n{INSPECTION}\n");
        out
    }
}

/// Evidence from text this crate produced.
///
/// Every call site passes a `format!` of something observed, so blankness would
/// be a bug in this file rather than bad input — and the fallback says exactly
/// that instead of rendering an empty cell. It is deliberately not an `unwrap`:
/// panicking here would take down a device that is mid-outage in order to
/// complain about a formatting bug.
fn evidence(what: &str) -> Evidence {
    Evidence::new(what).unwrap_or_else(|| {
        Evidence(
            "a check reported no detail, which is a defect in this software — treat \
             this row as unverified"
                .to_owned(),
        )
    })
}
