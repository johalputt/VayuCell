// SPDX-License-Identifier: Apache-2.0
//! A nonce that escapes its directive rewrites the policy protecting the page.
//!
//! `Nonce::new` is the one place a string this project did not write ends up
//! inside a security header, so the property worth fuzzing is that an accepted
//! nonce can never carry a character that ends the directive.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vayucell_core::csp::Nonce;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };

    if let Ok(nonce) = Nonce::new(text) {
        let rendered = nonce.as_str();
        for forbidden in ['\'', '"', ';', ' ', '\n', '\r', '\\'] {
            assert!(
                !rendered.contains(forbidden),
                "an accepted nonce carried {forbidden:?}, which ends the directive: {rendered:?}"
            );
        }
    }
});
