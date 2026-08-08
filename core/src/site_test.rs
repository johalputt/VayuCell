// SPDX-License-Identifier: Apache-2.0

//! Site tests, in the attacker's voice.
//!
//! The interesting cases are not "does index.html get served". They are the
//! request that walks out of the directory, the request for the `.git` folder
//! somebody left beside their site, and the request that arrives while the phone
//! is on battery and shedding.

use crate::governor::Level;
use crate::host::{FakeHost, Host};
use crate::shed::Stage;
use crate::site::{
    content_type, resolve, status_for, Availability, Refusal, Resolved, RootError, SiteRoot,
    Withheld, INDEX,
};

const ROOT: &str = "/home/u/site";

fn site() -> FakeHost {
    FakeHost::new()
        .with_dir(ROOT)
        .with_dir(&format!("{ROOT}/about"))
        .with_dir(&format!("{ROOT}/.git"))
        .with_file(&format!("{ROOT}/{INDEX}"), "<h1>hello</h1>")
        .with_file(&format!("{ROOT}/style.css"), "body{}")
        .with_file(&format!("{ROOT}/photo.png"), "\u{fffd}PNG")
        .with_file(&format!("{ROOT}/about/{INDEX}"), "<h1>about</h1>")
        .with_file(&format!("{ROOT}/.git/config"), "[core]")
        .with_file(&format!("{ROOT}/.env"), "TOKEN=shouldnotleak")
}

fn root(host: &FakeHost) -> SiteRoot {
    SiteRoot::open(host, ROOT).expect("the fixture creates this directory")
}

// ── Opening a root ────────────────────────────────────────────────────────────

#[test]
fn a_directory_the_host_cannot_see_is_not_opened() {
    // Absence is never protection. A root that was assumed would produce 404s
    // that look like an empty site rather than a misconfiguration.
    let host = FakeHost::new();
    assert_eq!(
        SiteRoot::open(&host, "/nope"),
        Err(RootError::Missing("/nope".to_owned()))
    );
}

#[test]
fn an_empty_directory_name_is_refused_rather_than_defaulted() {
    let host = FakeHost::new();
    assert_eq!(SiteRoot::open(&host, ""), Err(RootError::Empty));
    assert_eq!(SiteRoot::open(&host, "/"), Err(RootError::Empty));
}

#[test]
fn a_trailing_slash_does_not_produce_a_doubled_separator() {
    let host = site();
    let r = SiteRoot::open(&host, &format!("{ROOT}/")).expect("the directory exists");
    assert_eq!(r.dir(), ROOT);
    assert!(matches!(
        resolve(&r, &host, "/style.css"),
        Resolved::File { ref path, .. } if path == &format!("{ROOT}/style.css")
    ));
}

#[test]
fn a_root_with_no_index_is_reported_rather_than_refused() {
    // A site whose top level is only subdirectories is a real arrangement. It is
    // reported so the operator hears it from the program rather than a visitor.
    let host = FakeHost::new()
        .with_dir("/d")
        .with_dir("/d/a")
        .with_file("/d/a/index.html", "x");
    let r = SiteRoot::open(&host, "/d").expect("the directory exists");
    assert!(!r.has_index(&host));
    assert!(root(&site()).has_index(&site()));
}

// ── The ordinary path ─────────────────────────────────────────────────────────

#[test]
fn the_root_path_serves_the_index() {
    let host = site();
    assert_eq!(
        resolve(&root(&host), &host, "/"),
        Resolved::File {
            path: format!("{ROOT}/{INDEX}"),
            content_type: "text/html; charset=utf-8",
        }
    );
}

#[test]
fn a_named_file_is_served_with_the_type_its_extension_declares() {
    let host = site();
    assert_eq!(
        resolve(&root(&host), &host, "/style.css"),
        Resolved::File {
            path: format!("{ROOT}/style.css"),
            content_type: "text/css; charset=utf-8",
        }
    );
}

#[test]
fn a_directory_without_a_trailing_slash_still_finds_its_index() {
    // `/about` and `/about/` are the same page to a person typing them.
    let host = site();
    for path in ["/about", "/about/"] {
        assert_eq!(
            resolve(&root(&host), &host, path),
            Resolved::File {
                path: format!("{ROOT}/about/{INDEX}"),
                content_type: "text/html; charset=utf-8",
            },
            "{path}"
        );
    }
}

#[test]
fn a_file_wins_over_a_directory_of_the_same_name() {
    // The operator put a file there. Serving the directory's index instead would
    // be this program deciding which of their two things they meant.
    // `about` is both: a file of that name, and a directory of that name with
    // an index in it. Only a filesystem that allowed both could produce this,
    // and the point is which one wins if it ever does.
    let host = site()
        .with_file(&format!("{ROOT}/about"), "the file")
        .with_file(&format!("{ROOT}/about/{INDEX}"), "the directory");
    assert_eq!(
        resolve(&root(&host), &host, "/about"),
        Resolved::File {
            path: format!("{ROOT}/about"),
            content_type: "application/octet-stream",
        }
    );
}

#[test]
fn empty_segments_are_dropped_rather_than_refused() {
    // A leading slash produces one, and `//` produces another. Neither can leave
    // the root, so neither is worth a refusal an operator has to interpret.
    let host = site();
    assert_eq!(
        resolve(&root(&host), &host, "//style.css"),
        Resolved::File {
            path: format!("{ROOT}/style.css"),
            content_type: "text/css; charset=utf-8",
        }
    );
}

// ── Getting out of the directory ──────────────────────────────────────────────

#[test]
fn a_path_that_walks_upward_is_refused_by_the_segment_that_does_it() {
    let host = site();
    for path in [
        "/../etc/passwd",
        "/a/../../etc/passwd",
        "/about/../../../../root/.ssh/id_rsa",
        "/..",
    ] {
        assert_eq!(
            resolve(&root(&host), &host, path),
            Resolved::Refused(Refusal::Escape("..".to_owned())),
            "{path} escaped"
        );
    }
}

#[test]
fn resolution_refuses_traversal_on_its_own_without_help_from_the_parser() {
    // The parser in serve.rs also refuses these. That is the point: two
    // independent checks, so a later change that makes the parser permissive
    // cannot silently make this module unsafe. This test calls resolve directly
    // with strings the parser would never pass through.
    let host = site();
    let cases = [
        ("/a\\..\\b", "a\\..\\b"),
        ("/x\0y", "x\0y"),
        ("/./secret", "."),
    ];
    for (path, offender) in cases {
        assert_eq!(
            resolve(&root(&host), &host, path),
            Resolved::Refused(Refusal::Escape(offender.to_owned())),
            "{path:?} was not refused"
        );
    }
}

#[test]
fn no_sequence_of_ordinary_segments_can_produce_a_path_outside_the_root() {
    // The property the design rests on, asserted rather than argued. Every
    // resolution that produces a file must produce one under the root.
    let host = site();
    let r = root(&host);
    for path in [
        "/",
        "/style.css",
        "/about",
        "/about/",
        "//style.css",
        "/nope",
        "/a/b/c/d",
        "/style.css/",
    ] {
        if let Resolved::File { path: resolved, .. } = resolve(&r, &host, path) {
            assert!(
                resolved.starts_with(&format!("{ROOT}/")),
                "{path} resolved to {resolved}, which is outside the root"
            );
        }
    }
}

// ── The files nobody meant to publish ─────────────────────────────────────────

#[test]
fn a_hidden_name_is_refused_as_a_class_rather_than_by_blocklist() {
    // .git and .env are the two that turn "I served a folder" into a credential
    // disclosure, and a blocklist is a list of the ones somebody thought of.
    let host = site();
    for (path, offender) in [
        ("/.env", ".env"),
        ("/.git/config", ".git"),
        ("/about/.hidden", ".hidden"),
        ("/.well-known/anything", ".well-known"),
    ] {
        assert_eq!(
            resolve(&root(&host), &host, path),
            Resolved::Refused(Refusal::Hidden(offender.to_owned())),
            "{path} was served"
        );
    }
}

#[test]
fn a_hidden_file_that_exists_is_still_refused() {
    // The fixture really contains .env with a token in it. Refusal must not
    // depend on the file being absent.
    let host = site();
    assert!(host.exists(&format!("{ROOT}/.env")));
    let out = resolve(&root(&host), &host, "/.env");
    assert!(matches!(out, Resolved::Refused(Refusal::Hidden(_))));
    if let Resolved::Refused(r) = &out {
        assert!(!r.to_string().contains("shouldnotleak"));
    }
}

#[test]
fn a_directory_with_no_index_does_not_become_a_listing() {
    // A generated listing publishes everything the operator happened to leave in
    // a folder, which is a disclosure they never asked for.
    let host = site()
        .with_dir(&format!("{ROOT}/private"))
        .with_file(&format!("{ROOT}/private/notes.txt"), "x");
    for path in ["/private/", "/private"] {
        assert_eq!(
            resolve(&root(&host), &host, path),
            Resolved::Refused(Refusal::NoIndex(path.to_owned())),
            "{path}"
        );
    }
}

#[test]
fn every_refusal_is_a_404_so_it_does_not_map_the_directory() {
    // A 403 on the paths that exist and a 404 on the ones that do not is a
    // directory listing delivered one status code at a time.
    for refusal in [
        Refusal::Hidden(".env".to_owned()),
        Refusal::Escape("..".to_owned()),
        Refusal::NotFound("/x".to_owned()),
        Refusal::NoIndex("/d/".to_owned()),
    ] {
        assert_eq!(status_for(&refusal), 404, "{refusal:?}");
    }
}

#[test]
fn a_missing_path_says_so_without_naming_the_directory_on_disk() {
    // The refusal is read by a visitor. Where the site lives on the phone is not
    // theirs to learn.
    let host = site();
    let out = resolve(&root(&host), &host, "/nope.html");
    let Resolved::Refused(r) = out else {
        panic!("a missing file must be refused")
    };
    assert_eq!(r, Refusal::NotFound("/nope.html".to_owned()));
    assert!(!r.to_string().contains(ROOT), "{r}");
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[test]
fn an_unknown_extension_is_an_octet_stream_rather_than_a_guess() {
    for name in ["thing", "thing.", "thing.weird", "thing.exe", ""] {
        assert_eq!(content_type(name), "application/octet-stream", "{name}");
    }
}

#[test]
fn the_type_comes_from_the_extension_and_is_case_insensitive() {
    assert_eq!(content_type("A.PNG"), "image/png");
    assert_eq!(content_type("a.HtMl"), "text/html; charset=utf-8");
}

#[test]
fn a_dotted_filename_uses_its_last_extension() {
    // `archive.tar.gz` is not a tar, and `page.html.txt` is emphatically not
    // HTML — it is the file somebody renamed to stop it being HTML.
    assert_eq!(content_type("page.html.txt"), "text/plain; charset=utf-8");
    assert_eq!(content_type("archive.tar.gz"), "application/octet-stream");
}

#[test]
fn every_declared_type_is_a_single_header_value() {
    // A type carrying CR, LF or a semicolon in the wrong place would split the
    // response header. The values are constants, so this is cheap to guarantee
    // and expensive to discover otherwise.
    for name in [
        "a.html",
        "a.css",
        "a.js",
        "a.json",
        "a.txt",
        "a.xml",
        "a.svg",
        "a.png",
        "a.jpg",
        "a.gif",
        "a.webp",
        "a.avif",
        "a.ico",
        "a.woff2",
        "a.woff",
        "a.ttf",
        "a.pdf",
        "a.wasm",
        "a.unknown",
    ] {
        let t = content_type(name);
        assert!(!t.contains('\r') && !t.contains('\n'), "{name}: {t:?}");
        assert!(!t.is_empty(), "{name}");
    }
}

// ── The governor, which outranks all of the above ─────────────────────────────

#[test]
fn a_healthy_device_on_mains_serves_the_site() {
    assert_eq!(
        Availability::of(Level::Normal, Stage::Serving),
        Availability::Serving
    );
    assert!(Availability::of(Level::Normal, Stage::Serving).is_serving());
}

#[test]
fn a_derated_device_keeps_serving_and_that_is_deliberate() {
    // Deration answers heat. A static file read on a home network is not what is
    // producing the heat, and shedding it would cost the operator their site
    // while changing nothing about the temperature. The load worth shedding is
    // high-thermal ingress, and ingress::shed_for sheds exactly that, first.
    assert_eq!(
        Availability::of(Level::Derated, Stage::Serving),
        Availability::Serving
    );
}

#[test]
fn protect_and_halt_stop_the_site_whatever_the_outage_ladder_says() {
    for level in [Level::Protect, Level::Halt] {
        for stage in [Stage::Serving, Stage::Announced] {
            assert_eq!(
                Availability::of(level, stage),
                Availability::Withheld(Withheld::Governor(level)),
                "{level} {stage:?}"
            );
        }
    }
}

#[test]
fn the_shed_rung_is_where_a_website_stops() {
    // Stage::Shed's obligation is literally "stopped non-essential services",
    // and a website served from a phone during a power cut is the definition of
    // one. Before that rung nothing has been torn down, so the site continues.
    assert_eq!(
        Availability::of(Level::Normal, Stage::Announced),
        Availability::Serving
    );
    for stage in [Stage::Shed, Stage::Quiesced, Stage::ShuttingDown] {
        assert_eq!(
            Availability::of(Level::Normal, stage),
            Availability::Withheld(Withheld::Outage(stage)),
            "{stage:?}"
        );
    }
}

#[test]
fn the_governor_is_named_before_the_ladder_when_both_would_withhold() {
    // Both are true at once during a hot outage. The governor is the more
    // serious fact and is the one the operator is told about.
    assert_eq!(
        Availability::of(Level::Halt, Stage::ShuttingDown),
        Availability::Withheld(Withheld::Governor(Level::Halt))
    );
}

#[test]
fn a_withheld_site_explains_itself_to_someone_who_cannot_see_the_device() {
    // A visitor is not the owner. "Error" invites a retry; this says the device
    // is deliberately not answering, and why.
    let g = Availability::of(Level::Protect, Stage::Serving).describe();
    assert!(g.contains("PROTECT"), "{g}");
    assert!(g.contains("not being served"), "{g}");

    let o = Availability::of(Level::Normal, Stage::Shed).describe();
    assert!(o.contains("stopped non-essential services"), "{o}");
    assert!(o.contains("on battery"), "{o}");
}

#[test]
fn availability_is_decided_only_by_the_governor_and_the_ladder() {
    // Every combination is covered, so a level or stage added later cannot fall
    // through to a default that serves. The match in `of` is exhaustive; this
    // asserts the resulting table has no accidental gaps.
    let levels = [Level::Normal, Level::Derated, Level::Protect, Level::Halt];
    let stages = [
        Stage::Serving,
        Stage::Announced,
        Stage::Shed,
        Stage::Quiesced,
        Stage::ShuttingDown,
    ];
    let mut serving = 0;
    for level in levels {
        for stage in stages {
            if Availability::of(level, stage).is_serving() {
                serving += 1;
            }
        }
    }
    // Normal and Derated, each with Serving and Announced. Nothing else.
    assert_eq!(serving, 4, "the set of states that serve has changed");
}

// ── The bug a fake made of files alone could not express ──────────────────────

#[test]
fn a_directory_is_not_resolved_as_though_it_were_a_page() {
    // The defect a live request found and every unit test missed. `exists` was
    // being asked a question it cannot answer — `/blog` exists, and existing is
    // not the same as being readable content — so the directory resolved as a
    // file and the read failed with a server error for a page that was there.
    //
    // It could not be caught before because the fake host had no directories:
    // every path in it was a file, so the case did not exist in the test world.
    let host = site();
    assert_eq!(
        resolve(&root(&host), &host, "/about"),
        Resolved::File {
            path: format!("{ROOT}/about/{INDEX}"),
            content_type: "text/html; charset=utf-8",
        },
        "a directory with an index must serve the index, not itself"
    );
}

#[test]
fn a_directory_that_is_there_is_told_apart_from_a_path_that_is_not() {
    // Different answers because they are different problems: one is a visitor's
    // typo, the other is a page the operator forgot to write.
    let host = site().with_dir(&format!("{ROOT}/empty"));
    assert_eq!(
        resolve(&root(&host), &host, "/empty"),
        Resolved::Refused(Refusal::NoIndex("/empty".to_owned()))
    );
    assert_eq!(
        resolve(&root(&host), &host, "/never-existed"),
        Resolved::Refused(Refusal::NotFound("/never-existed".to_owned()))
    );
    // Both are still 404 on the wire, so the difference never reaches a stranger.
    assert_eq!(status_for(&Refusal::NoIndex("/empty".to_owned())), 404);
    assert_eq!(status_for(&Refusal::NotFound("/x".to_owned())), 404);
}

#[test]
fn an_unreadable_file_is_still_a_file_rather_than_a_directory() {
    // Collapsing unreadable into absent is the mistake this project exists to
    // refuse. A file that cannot be read must not silently become a directory
    // lookup that answers with somebody else's index.html.
    let host = site().with_unreadable(&format!("{ROOT}/locked.html"));
    assert_eq!(
        resolve(&root(&host), &host, "/locked.html"),
        Resolved::File {
            path: format!("{ROOT}/locked.html"),
            content_type: "text/html; charset=utf-8",
        }
    );
}
