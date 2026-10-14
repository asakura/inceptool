//! The grouping-pass accumulator behind [`super::generate_command_table`]: [`PlatformPair`] and [`PlatformPairs`].

use super::render::{Renderer, crate_path};

use crate::types::Platform;

use proc_macro2::TokenStream;
use quote::quote;

/// One name/alias's rendered [`crate::entry::CommandEntry`] tokens, tagged with the [`Platform`]
/// it models — one per [`PlatformPairs`] entry.
pub struct PlatformPair {
    variant: (Platform, TokenStream),
}

impl PlatformPair {
    #[must_use = "constructing a pair has no effect unless the caller stores it"]
    pub const fn new(platform: Platform, entry: TokenStream) -> Self {
        Self {
            variant: (platform, entry),
        }
    }
}

/// Every [`Platform`]-tagged rendered [`crate::entry::CommandEntry`] one command name/alias
/// resolves to, accumulated by [`super::generate_command_table`]'s grouping pass before being
/// rendered into a single `&[PlatformEntry]` slice.
#[derive(Default)]
pub struct PlatformPairs {
    pub variants: Vec<PlatformPair>,
}

impl Renderer for PlatformPairs {
    /// `self` rendered as the `&'static [::inceptool_risk_data::PlatformEntry]` slice that one
    /// command name/alias maps to in the generated `COMMANDS` table. Table-structure-specific,
    /// not a single schema value's own rendering, so this stays its own impl rather than living
    /// alongside the schema-type impls in [`super::render`].
    fn render(self) -> TokenStream {
        let platform_entry = crate_path("PlatformEntry");

        let rendered = self.variants.iter().map(
            |PlatformPair {
                 variant: (platform, entry),
             }| {
                let platform = platform.render();

                quote! { #platform_entry { platform: #platform, entry: #entry } }
            },
        );

        quote! { &[#(#rendered),*] }
    }
}
