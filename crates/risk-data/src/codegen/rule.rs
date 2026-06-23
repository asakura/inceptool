//! Renders one scope's flag/combo-rule/operand-rule declarations — see [`render_ruleset`].

use super::render::{Renderer, crate_path};

use crate::types::{ComboRule, Flag, OperandRule, ProfilePatch, ValueRule};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

impl Renderer for ValueRule {
    /// One `[[command.flag.value_rule]]` rendered as `::inceptool_risk_data::ValueRuleEntry`
    /// struct-literal tokens.
    fn render(self) -> TokenStream {
        let value_rule_entry = crate_path("ValueRuleEntry");
        let pattern = self.pattern.as_ref();
        let effect = self.effect.render();
        let patch = self.profile.render();
        let reason = self.reason.as_ref();

        quote! {
            #value_rule_entry { pattern: #pattern, effect: #effect, patch: #patch, reason: #reason }
        }
    }
}

impl Renderer for Flag {
    /// One `[[command.flag]]`'s `::inceptool_risk_data::FlagEntry` struct-literal tokens —
    /// pairing this with each of its spellings is [`super::render_ruleset`]'s job, not this
    /// impl's, since a single `Flag` maps to more than one `flags` slice entry.
    fn render(self) -> TokenStream {
        let flag_entry = crate_path("FlagEntry");
        let value_rules = self.value_rule.into_iter().map(Renderer::render);
        let takes_value_path = crate_path("TakesValue");

        let takes_value = self.takes_value.map_or_else(
            || quote! { None },
            |t| {
                let variant = format_ident!("{t:?}");
                quote! { Some(#takes_value_path::#variant) }
            },
        );

        let effect = self.effect.render();
        let patch = self.profile.render();
        let reason = self.reason.as_ref();
        let spellings = self.spellings;

        let entry = quote! {
            #flag_entry {
                effect: #effect,
                patch: #patch,
                reason: #reason,
                takes_value: #takes_value,
                value_rules: &[#(#value_rules),*],
            }
        };

        spellings
            .into_iter()
            .map(|spelling| {
                let spelling = spelling.as_ref();

                quote! { (#spelling, #entry) }
            })
            .collect()
    }
}

impl Renderer for ComboRule {
    /// `self` rendered as `::inceptool_risk_data::ComboRuleEntry` struct-literal tokens.
    fn render(self) -> TokenStream {
        let combo_rule_entry = crate_path("ComboRuleEntry");
        let requires = self.requires.iter().map(AsRef::as_ref);
        let effect = self.effect.render();
        let patch = self.profile.render();
        let reason = self.reason.as_ref();

        quote! {
            #combo_rule_entry {
                requires: &[#(#requires),*],
                effect: #effect,
                patch: #patch,
                reason: #reason,
            }
        }
    }
}

impl Renderer for OperandRule {
    /// `self` rendered as `::inceptool_risk_data::OperandRuleEntry` struct-literal tokens.
    fn render(self) -> TokenStream {
        let operand_rule_entry = crate_path("OperandRuleEntry");
        let pattern = self.pattern.as_ref();
        let effect = self.effect.render();
        let patch = self.profile.render();
        let reason = self.reason.as_ref();

        quote! {
            #operand_rule_entry { pattern: #pattern, effect: #effect, patch: #patch, reason: #reason }
        }
    }
}

/// One scope's (a command's global scope, or one of its subcommands') baseline/flags/combo-rules/
/// operand-rules rendered as `::inceptool_risk_data::RuleSetEntry` struct-literal tokens — shared
/// by `Command` and `Subcommand`, neither of which owns this 6-argument shape as a single field a
/// `Renderer` impl could take as `&self`.
#[must_use = "rendering has no effect unless the caller uses the result"]
pub(super) fn render_ruleset(
    ruleset_entry: &TokenStream,
    baseline: ProfilePatch,
    baseline_reason: &str,
    short_flags_combinable: bool,
    flags: Vec<Flag>,
    combo_rules: Vec<ComboRule>,
    operand_rules: Vec<OperandRule>,
) -> TokenStream {
    let baseline = baseline.render();
    let combo_rules = combo_rules.into_iter().map(Renderer::render);
    let operand_rules = operand_rules.into_iter().map(Renderer::render);

    let flags = flags.into_iter().flat_map(Renderer::render);

    quote! {
        #ruleset_entry {
            baseline: #baseline,
            baseline_reason: #baseline_reason,
            short_flags_combinable: #short_flags_combinable,
            flags: &[#(#flags),*],
            combo_rules: &[#(#combo_rules),*],
            operand_rules: &[#(#operand_rules),*],
        }
    }
}
