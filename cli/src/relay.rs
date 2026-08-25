// SPDX-License-Identifier: Apache-2.0

//! The relay half of sovereign ingress (ADR-0003 §2, ADR-0013).
//!
//! # What this module is, and what it deliberately is not
//!
//! A relay path means a machine you rent sits on the public internet,
//! accepts visitors by a DNS name, and forwards every connection to this
//! device — dialing **in**, which is the only direction the charter permits
//! anyone to dial ([charter] Article V). This module has no code for
//! configuring that rented machine: configuring somebody else's server is
//! that server's job, and a cell that reached out to do it would be a cell
//! that dials.
//!
//! What the cell owes instead is the two things it can do from where it
//! stands: refuse to *be* configured with a name it could not have been
//! given honestly ([`validate_host`]), and say out loud — before anything
//! binds, in the words of [`startup_lines`] — who can end this path and
//! what it costs. The profile those sentences come from lives in
//! [`crate::ingress::Mode::Relay`], declared like every other mode's.

use vayucell_core::ingress::{self, Mode};

/// Validates a relay hostname at the moment it is typed.
///
/// The rules are the ones a name must satisfy for the banner's sentences
/// about it to be true: it is one label-path of ASCII letters, digits,
/// hyphens and dots; nothing empty, nothing with a scheme, a port, a
/// space, or a `@` bolted on; nothing ending in a dot as if more name were
/// coming. Anything else is refused naming the rule, because an argument
/// that silently mangles a hostname produces a banner promising
/// reachability at an address nobody can type.
///
/// # Errors
///
/// One message per refusal, each naming what to type instead.
pub fn validate_host(raw: &str) -> Result<String, String> {
    let host = raw.trim().to_ascii_lowercase();
    if host.is_empty() {
        return Err("--relay-via needs a hostname, like relay.example.org".to_owned());
    }
    if host.len() > 253 {
        return Err(format!(
            "{host} is {} characters long and a hostname tops out at 253",
            host.len()
        ));
    }
    if host.starts_with('-') || host.ends_with('-') {
        return Err(format!(
            "{host} starts or ends with a hyphen, which no DNS label may"
        ));
    }
    if host.starts_with('.') || host.ends_with('.') {
        return Err(format!(
            "{host} starts or ends with a dot — give the name alone"
        ));
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    {
        return Err(format!(
            "{host} carries a character a DNS name cannot — letters, digits, \
             hyphens and dots only; no scheme, no port, no user part"
        ));
    }
    Ok(host)
}

/// The startup lines declaring a relay path, printed before anything binds.
///
/// Three facts, none optional: the dependency sentence (who ends this and
/// what the middle reads), the forwarding instruction (what the rented side
/// must be pointed at, since this program will never go and configure it),
/// and the standing of the path itself — unverified, and what unverified
/// means here. Every line names the host, so a typo in `--relay-via` is
/// visible in the same output that promises reachability under it.
#[must_use]
pub fn startup_lines(host: &str, site_addr: &str, vault_addr: &str) -> Vec<String> {
    let p = Mode::Relay.profile();
    let mut out = Vec::new();
    for d in ingress::disclosures(Mode::Relay, false) {
        out.push(d);
    }
    out.push(format!(
        "reachable via {host} — configure the relay to forward to \
         http://{site_addr} (site) and http://{vault_addr} (vault) on this \
         device; the panel is not published through it"
    ));
    out.push(format!(
        "relay  {host}  UNVERIFIED until a request arrives from \
         outside — {}",
        p.middle_sees
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::{startup_lines, validate_host};

    fn ok(s: &str) -> String {
        validate_host(s).expect("valid")
    }

    #[test]
    fn a_hostname_is_lowercased_and_carried_as_the_cell_will_name_it() {
        assert_eq!(ok("Relay.Example.ORG"), "relay.example.org");
    }

    #[test]
    fn every_way_to_mistype_a_relay_is_refused_by_its_rule() {
        for bad in [
            "",
            "   ",
            "https://relay.example.org",
            "relay.example.org:8443",
            "me@relay.example.org",
            "relay example org",
            "-relay.example.org",
            "relay.example.org-",
            ".relay.example.org",
            "relay.example.org.",
            "relay_example.org",
        ] {
            let e = validate_host(bad).expect_err(bad);
            assert!(!e.is_empty(), "{bad:?} refused with an empty reason");
        }
    }

    #[test]
    fn a_name_longer_than_dns_allows_is_refused_with_the_count() {
        let huge = format!("{}.example.org", "a".repeat(250));
        let e = validate_host(&huge).expect_err("too long");
        assert!(e.contains("253"), "{e}");
    }

    #[test]
    fn the_startup_lines_name_the_host_both_ways_and_forward_no_panel() {
        let lines = startup_lines(
            "relay.example.org",
            "192.168.1.20:8081",
            "192.168.1.20:8082",
        );
        let joined = lines.join("\n");
        // Named twice: once as where to point the forwarder, once as the
        // address whose standing is being reported.
        assert!(joined.matches("relay.example.org").count() >= 2, "{joined}");
        assert!(joined.contains("192.168.1.20:8081"), "{joined}");
        assert!(joined.contains("192.168.1.20:8082"), "{joined}");
        // The battery panel never travels through somebody else's server:
        // its address appears nowhere in what a relay deployment announces.
        assert!(!lines.iter().any(|l| l.contains(":8080")), "{joined}");
        assert!(joined.contains("UNVERIFIED"), "{joined}");
    }

    #[test]
    fn the_dependency_sentences_come_from_the_declared_profile_not_from_prose() {
        // The supplier sentence is generated from Mode::Relay's profile, so a
        // profile edit changes the banner without anybody re-wording strings.
        let lines = startup_lines("r.example.org", "h:1", "h:2");
        let joined = lines.join("\n");
        assert!(
            joined.contains("ended by the provider, at will"),
            "{joined}"
        );
    }
}
