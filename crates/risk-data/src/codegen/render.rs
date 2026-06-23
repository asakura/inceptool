//! The [`Renderer`] trait every other [`super`] submodule implements to turn a `crate::types`
//! schema value into codegen tokens, plus the [`crate_path`] helper every impl resolves its named
//! types through — see both.

use crate::types::{Effect, FlagGrammar, Platform, ProfilePatch};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use std::fmt;

/// Renders a `crate::types` schema value into the [`proc_macro2::TokenStream`]
/// [`super::generate_command_table`] splices into its generated source.
pub(super) trait Renderer {
    /// `self` rendered as the token shape appropriate to its own type (a variant path, a
    /// struct-literal, ...) — see each impl for which.
    #[must_use = "rendering has no effect unless the caller uses the result"]
    fn render(self) -> TokenStream;
}

/// `name` rendered as an absolute path into this crate, e.g. `crate_path("CommandEntry")` ⇒
/// `::inceptool_risk_data::CommandEntry` — every generated reference to one of this crate's own
/// runtime/schema types resolves through this helper, so [`super::generate_command_table`]'s
/// output never depends on a `use` statement at the `include!` site. A free function rather than
/// a `Renderer` default method, since it doesn't read `Self` — a default method no impl ever
/// overrides just trips `clippy::missing_trait_methods` for no benefit.
#[must_use = "rendering has no effect unless the caller uses the result"]
pub(super) fn crate_path(name: &str) -> TokenStream {
    let ident = format_ident!("{name}");

    quote! { ::inceptool_risk_data::#ident }
}

/// `value` rendered as `Some(#type_name::#variant)`/`None` tokens — the one shape every
/// `ProfilePatch` axis field shares, since each axis enum's `Debug` output is exactly its variant
/// name.
#[must_use = "rendering has no effect unless the caller uses the result"]
fn render_optional<T>(value: Option<T>, type_name: &str) -> TokenStream
where
    T: fmt::Debug,
{
    value.map_or_else(
        || quote! { None },
        |v| {
            let path = crate_path(type_name);
            let variant = format_ident!("{v:?}");

            quote! { Some(#path::#variant) }
        },
    )
}

impl Renderer for ProfilePatch {
    /// `self` rendered as `::inceptool_risk_data::ProfilePatch` struct-literal tokens.
    fn render(self) -> TokenStream {
        let profile_patch = crate_path("ProfilePatch");

        if self.is_empty() {
            return quote! { #profile_patch::EMPTY };
        }

        let trust = render_optional(self.trust, "TrustImpact");
        let reversibility = render_optional(self.reversibility, "Reversibility");
        let blast_radius = render_optional(self.blast_radius, "BlastRadius");
        let disclosure = render_optional(self.disclosure, "Disclosure");
        let persistence = render_optional(self.persistence, "Persistence");
        let privilege = render_optional(self.privilege, "Privilege");
        let auditability = render_optional(self.auditability, "Auditability");
        let exposure = render_optional(self.exposure, "Exposure");
        let verification = render_optional(self.verification, "Verification");

        quote! {
            #profile_patch {
                trust: #trust,
                reversibility: #reversibility,
                blast_radius: #blast_radius,
                disclosure: #disclosure,
                persistence: #persistence,
                privilege: #privilege,
                auditability: #auditability,
                exposure: #exposure,
                verification: #verification,
            }
        }
    }
}

impl Renderer for Effect {
    /// `self` rendered as an `::inceptool_risk_data::Effect` variant path.
    fn render(self) -> TokenStream {
        let effect_path = crate_path("Effect");
        let variant = format_ident!(
            "{}",
            match self {
                Self::Escalate => "Escalate",
                Self::Mitigate => "Mitigate",
            }
        );

        quote! { #effect_path::#variant }
    }
}

impl Renderer for FlagGrammar {
    /// `self` rendered as a `::inceptool_risk_data::FlagGrammar` variant path.
    fn render(self) -> TokenStream {
        let grammar_path = crate_path("FlagGrammar");
        let variant = format_ident!(
            "{}",
            match self {
                Self::Gnu => "Gnu",
                Self::Bsd => "Bsd",
                Self::Go => "Go",
            }
        );

        quote! { #grammar_path::#variant }
    }
}

impl Renderer for Platform {
    /// `self` rendered as a `::inceptool_risk_data::Platform` variant path.
    fn render(self) -> TokenStream {
        let platform_path = crate_path("Platform");
        let variant = format_ident!(
            "{}",
            match self {
                Self::GnuLinux => "GnuLinux",
                Self::Bsd => "Bsd",
                Self::MacOs => "MacOs",
                Self::Busybox => "Busybox",
            }
        );

        quote! { #platform_path::#variant }
    }
}
