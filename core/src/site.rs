// SPDX-License-Identifier: Apache-2.0

//! Serving a directory of files — the first surface here that exists for
//! somebody other than the device's owner.
//!
//! # Why this is a different surface from the panel, on a different port
//!
//! The panel is what the owner can already see by picking the phone up. A site
//! is content they wrote for other people, and it needs a policy that lets their
//! own stylesheet and their own script run. That policy is weaker than
//! [`crate::csp::control_surface`], and rather than weaken the panel's policy to
//! accommodate it, the two are separate origins: separate listener, separate
//! port, separate [`crate::csp::published_site`] policy. A page served from the
//! site cannot reach the panel with a same-origin request because it is not the
//! same origin.
//!
//! # Traversal is impossible by construction, not by checking
//!
//! [`resolve`] never concatenates the requested path onto the root. It splits
//! into segments, refuses every segment that is not a plain name, and joins the
//! survivors. A segment cannot be `..`, cannot contain a separator, and cannot
//! begin with a dot, so there is no sequence of segments whose join leaves the
//! root. A checker that normalises first and validates second is validating a
//! string the filesystem has already agreed to.
//!
//! # What this cannot see, said here rather than discovered later
//!
//! A symbolic link inside the site directory pointing outside it escapes every
//! check in this module, because [`Host`] cannot report link targets — it reads
//! and it tests existence, and a link is transparent to both. That containment
//! has to be enforced where real paths are resolved, against the real
//! filesystem, and this module is deliberately not that place. It says so rather
//! than implying a completeness it does not have.
//!
//! # The governor wins here too
//!
//! [`Availability::of`] takes the governor's level and the shed ladder's stage
//! and answers whether the site serves at all. There is no argument that
//! overrides it and no parameter that disables it, for the same reason
//! [`crate::ingress::shed_for`] has neither.

use crate::governor::Level;
use crate::host::Host;
use crate::shed::Stage;

/// A directory that has been checked to exist.
///
/// The field is private and there is no constructor but [`SiteRoot::open`], so a
/// root nobody asked the host about cannot be built:
///
/// ```compile_fail
/// use vayucell_core::site::SiteRoot;
/// // Absence is never protection, and a root that was assumed is a root that
/// // was not checked.
/// let root = SiteRoot { dir: "/tmp/site".to_owned() };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteRoot {
    dir: String,
}

/// Why a directory could not be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootError {
    /// No directory was named.
    Empty,
    /// The host cannot see it.
    Missing(String),
}

impl core::fmt::Display for RootError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RootError::Empty => f.write_str("no directory was given to serve"),
            RootError::Missing(d) => write!(
                f,
                "{d} cannot be seen by this process, so there is nothing to serve"
            ),
        }
    }
}

impl SiteRoot {
    /// Opens a directory to serve.
    ///
    /// # Errors
    ///
    /// Returns why the directory is not usable, in the words an operator reads.
    pub fn open(host: &dyn Host, dir: &str) -> Result<Self, RootError> {
        let dir = dir.trim_end_matches('/');
        if dir.is_empty() {
            return Err(RootError::Empty);
        }
        if !host.exists(dir) {
            return Err(RootError::Missing(dir.to_owned()));
        }
        Ok(Self {
            dir: dir.to_owned(),
        })
    }

    /// The directory being served.
    #[must_use]
    pub fn dir(&self) -> &str {
        &self.dir
    }

    /// Whether the root has something to serve at `/`.
    ///
    /// Not a failure — a site whose top level is a set of subdirectories is a
    /// real arrangement. It is reported so the operator hears it from the
    /// program rather than from a visitor.
    #[must_use]
    pub fn has_index(&self, host: &dyn Host) -> bool {
        host.exists(&format!("{}/{INDEX}", self.dir))
    }
}

/// The file served for a directory.
pub const INDEX: &str = "index.html";

/// Why a path was not served.
///
/// Every variant names what was refused. A bare `None` here would become "404"
/// in a log, and "404" is the one answer that cannot distinguish a typo from an
/// attempted traversal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A segment began with a dot.
    ///
    /// `.git`, `.env`, `.ssh` — the directories that turn "I served a folder"
    /// into a credential disclosure. Refused as a class rather than by
    /// blocklist, because a blocklist is a list of the ones somebody thought of.
    Hidden(String),
    /// A segment could not be part of a path under the root.
    Escape(String),
    /// Nothing is there.
    NotFound(String),
    /// A directory with no `index.html` in it.
    ///
    /// Deliberately not a generated listing. A listing turns every directory
    /// into an index of everything the operator happened to leave in it, which
    /// is a disclosure they did not ask for and would not see in a browser they
    /// tested with a URL they already knew.
    NoIndex(String),
}

impl core::fmt::Display for Refusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Refusal::Hidden(s) => write!(
                f,
                "{s:?} begins with a dot; hidden names are never served, because \
                 that is how a .git or a .env directory leaves a building"
            ),
            Refusal::Escape(s) => write!(
                f,
                "{s:?} is not a name that can appear under the site directory"
            ),
            Refusal::NotFound(p) => write!(f, "nothing is published at {p}"),
            Refusal::NoIndex(p) => write!(
                f,
                "{p} is a directory with no {INDEX} in it, and this does not \
                 generate a listing of what happens to be inside"
            ),
        }
    }
}

/// The status a refusal is served with.
///
/// A hidden name and a traversal attempt are both 404 rather than 403. Telling
/// somebody which paths exist but are forbidden is telling them where to look,
/// and the operator learns the real reason from the log either way.
#[must_use]
pub const fn status_for(refusal: &Refusal) -> u16 {
    match refusal {
        Refusal::Hidden(_) | Refusal::Escape(_) | Refusal::NotFound(_) | Refusal::NoIndex(_) => 404,
    }
}

/// What a request path resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A file to read and send.
    File {
        /// The full path, built from validated segments.
        path: String,
        /// The type to declare, from the extension and nothing else.
        content_type: &'static str,
    },
    /// Not served, and why.
    Refused(Refusal),
}

/// Resolves a request path against a site root.
///
/// `path` is expected to have come through [`crate::serve::parse_request_line`],
/// which has already refused percent-encoding, backslashes and `..`. This does
/// not rely on that. The two checks are independent on purpose: a later change
/// that made the parser more permissive would otherwise silently make this
/// module unsafe, and a defence that depends on a caller's discipline is a
/// convention rather than a defence.
#[must_use]
pub fn resolve(root: &SiteRoot, host: &dyn Host, path: &str) -> Resolved {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        // Empty segments come from a leading slash and from `//`. They add
        // nothing to the path and cannot leave the root, so they are dropped
        // rather than refused.
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." {
            return Resolved::Refused(Refusal::Escape(segment.to_owned()));
        }
        if segment.starts_with('.') {
            return Resolved::Refused(Refusal::Hidden(segment.to_owned()));
        }
        // A separator or a NUL inside what the split produced cannot happen for
        // '/', and can for the others. Checked rather than reasoned about,
        // because the reasoning is what changes when somebody edits the split.
        if segment.contains('\\') || segment.contains('\0') || segment.contains('/') {
            return Resolved::Refused(Refusal::Escape(segment.to_owned()));
        }
        segments.push(segment);
    }

    let base = if segments.is_empty() {
        root.dir.clone()
    } else {
        format!("{}/{}", root.dir, segments.join("/"))
    };

    // A trailing slash and the bare root are requests for a directory. Anything
    // else is asked of the filesystem, because the request's shape is the
    // visitor's guess and `is_file` is the answer.
    //
    // The first version of this used `exists` for both questions, since that was
    // all [`Host`] offered. `/blog` then resolved to the *directory* `blog` as
    // though it were a page, and the read failed with a 500 for a page that was
    // there the whole time. It passed every unit test, because a fake host where
    // every path is a file cannot express a directory. That is why [`Host`] now
    // has `is_file` and the fake has `with_dir`.
    let named_a_file = !path.ends_with('/') && !segments.is_empty() && host.is_file(&base);

    if named_a_file {
        let name = segments.last().copied().unwrap_or(INDEX);
        return Resolved::File {
            path: base,
            content_type: content_type(name),
        };
    }

    // Everything else is a directory: the root, a trailing slash, or a name that
    // exists and is not a file. A file and a directory of the same name resolve
    // to the file, above — the thing the operator actually put there.
    let candidate = format!("{base}/{INDEX}");
    if host.is_file(&candidate) {
        return Resolved::File {
            path: candidate,
            content_type: content_type(INDEX),
        };
    }

    // A directory that is there but has no index is a different answer from a
    // path that is not there at all, and the operator needs the difference: one
    // is a visitor's typo, the other is a page they forgot to write.
    if host.exists(&base) {
        return Resolved::Refused(Refusal::NoIndex(path.to_owned()));
    }

    Resolved::Refused(Refusal::NotFound(path.to_owned()))
}

/// The content type for a filename, from its extension.
///
/// An allowlist, and an unknown extension is `application/octet-stream` rather
/// than a guess. Nothing here inspects the bytes: content sniffing is how a file
/// somebody uploaded as a `.txt` gets executed as HTML, and the response also
/// carries `X-Content-Type-Options: nosniff` so the browser does not do it
/// either.
#[must_use]
pub fn content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/vnd.microsoft.icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "pdf" => "application/pdf",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Why the site is not serving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withheld {
    /// The governor is protecting the cell.
    Governor(Level),
    /// The outage ladder has shed non-essential services.
    Outage(Stage),
}

/// Whether the site serves right now.
///
/// There is deliberately no third variant. "Probably", "degraded" and "unknown"
/// are all ways of serving while telling somebody you might not be:
///
/// ```compile_fail
/// use vayucell_core::site::Availability;
/// // The type annotation is load-bearing: without it a bare variant name is a
/// // constructor, and this would compile as a function value.
/// let a: Availability = Availability::ProbablyFine;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Requests are answered from the site directory.
    Serving,
    /// Requests are answered with a refusal that says why.
    Withheld(Withheld),
}

impl Availability {
    /// What the governor and the outage ladder permit, together.
    ///
    /// `DERATED` still serves, and that is a decision rather than an oversight.
    /// Deration is a response to heat, a static file read on a home network is
    /// not what is producing the heat, and shedding a negligible load to address
    /// a thermal problem is theatre that costs the operator their site. The load
    /// that is making the device hot is high-thermal ingress, and
    /// [`crate::ingress::shed_for`] already sheds exactly that, first.
    ///
    /// `PROTECT` and `HALT` do not serve. At those levels the device is in
    /// trouble and every watt is the operator's, not a visitor's.
    ///
    /// [`Stage::Shed`] is where the outage ladder says "stopped non-essential
    /// services", and a website served from a phone during a power cut is the
    /// definition of one. Before that rung — `Serving` and `Announced` — nothing
    /// has been torn down and the site keeps running.
    #[must_use]
    pub const fn of(level: Level, stage: Stage) -> Self {
        match level {
            Level::Protect | Level::Halt => Self::Withheld(Withheld::Governor(level)),
            Level::Normal | Level::Derated => match stage {
                Stage::Serving | Stage::Announced => Self::Serving,
                Stage::Shed | Stage::Quiesced | Stage::ShuttingDown => {
                    Self::Withheld(Withheld::Outage(stage))
                }
            },
        }
    }

    /// Whether requests are being answered from the directory.
    #[must_use]
    pub const fn is_serving(&self) -> bool {
        matches!(self, Self::Serving)
    }

    /// What to tell a visitor, and what to log.
    ///
    /// Phrased for somebody who is not the owner and cannot see the device: it
    /// says the site is deliberately not being served and why, rather than
    /// presenting itself as a fault they might retry into.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Serving => "this site is being served from the device".to_owned(),
            Self::Withheld(Withheld::Governor(level)) => format!(
                "this site is not being served: the device holding it is at {level}, \
                 and protecting its battery outranks answering this request"
            ),
            Self::Withheld(Withheld::Outage(stage)) => format!(
                "this site is not being served: the device holding it is on battery \
                 and has {}",
                stage.obligation()
            ),
        }
    }
}
