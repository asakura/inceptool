//! Renders a [`Command`] and its [`Subcommand`]s into `CommandEntry`/`RuleSetEntry` tokens.

use super::render::{Renderer, crate_path};
use super::rule::render_ruleset;

use crate::types::{Command, Subcommand};

use proc_macro2::TokenStream;
use quote::quote;

use std::iter;

impl Renderer for Subcommand {
    /// `self` rendered as `::inceptool_risk_data::RuleSetEntry` struct-literal tokens — pairing
    /// this with each of its names/aliases is [`Command`]'s `render` impl's job, mirroring how
    /// [`render_ruleset`] pairs a `Flag`'s rendered entry with each of its spellings.
    fn render(self) -> TokenStream {
        let ruleset_entry = crate_path("RuleSetEntry");

        let entry = render_ruleset(
            &ruleset_entry,
            self.baseline,
            &self.baseline_reason,
            self.short_flags_combinable,
            self.flag,
            self.combo_rule,
            self.operand_rule,
        );

        iter::once(self.name.as_ref())
            .chain(self.aliases.iter().map(AsRef::as_ref))
            .map(move |name| quote! { (#name, #entry) })
            .collect()
    }
}

impl Renderer for Command {
    /// `self` rendered as `::inceptool_risk_data::CommandEntry` struct-literal tokens — what
    /// every `Platform` variant of `self`'s names/aliases maps to, once grouped, in the generated
    /// `COMMANDS` table.
    fn render(self) -> TokenStream {
        let command_entry = crate_path("CommandEntry");
        let grammar = self.grammar.render();
        let case_sensitive = self.case_sensitive;
        let ruleset_entry = crate_path("RuleSetEntry");
        let subcommands = self.subcommands;

        let rules = render_ruleset(
            &ruleset_entry,
            self.baseline,
            &self.baseline_reason,
            self.short_flags_combinable,
            self.flag,
            self.combo_rule,
            self.operand_rule,
        );

        let subcommands = subcommands.into_iter().flat_map(Renderer::render);

        quote! {
            #command_entry {
                grammar: #grammar,
                case_sensitive: #case_sensitive,
                rules: #rules,
                subcommands: &[#(#subcommands),*],
            }
        }
    }
}
