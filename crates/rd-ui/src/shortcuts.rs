//! The command table: every action the shell can perform, with its
//! keyboard chord, its group, and the words it is known by.
//!
//! One table, three uses: the app binds keys from it, the command palette
//! searches it, and the shortcuts sheet renders it. A key that is not in
//! this table is not a shortcut, so the sheet cannot drift from the
//! application.
//!
//! Key names follow the platform-neutral convention the app translates at
//! the edge: `Mod` is ⌘ on macOS and Ctrl elsewhere.

/// One command the shell can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// Stable identifier, used by the app's dispatcher and by tests.
    pub id: &'static str,
    /// What the command does, in the operator's words.
    pub title: &'static str,
    /// The chord as displayed: `Mod+K`, `Alt+3`, `Shift+Right`.
    pub keys: &'static str,
    /// Grouping in the palette and the sheet.
    pub group: &'static str,
    /// Extra words the palette should match (`export, deliver, render`).
    pub aliases: &'static str,
    /// One line of consequence, when the action is not self-evident.
    pub hint: &'static str,
}

const fn cmd(
    id: &'static str,
    title: &'static str,
    keys: &'static str,
    group: &'static str,
    aliases: &'static str,
    hint: &'static str,
) -> CommandSpec {
    CommandSpec {
        id,
        title,
        keys,
        group,
        aliases,
        hint,
    }
}

/// Every command, grouped in the order the sheet shows them.
pub const COMMANDS: &[CommandSpec] = &[
    // Workflow
    cmd("stage.ingest", "Go to Ingest", "Alt+1", "Workflow", "sources files fixity add", ""),
    cmd("stage.inspect", "Go to Inspect", "Alt+2", "Workflow", "analysis findings measure", ""),
    cmd("stage.decide", "Go to Decide", "Alt+3", "Workflow", "chain modules operations", ""),
    cmd("stage.preview", "Go to Preview", "Alt+4", "Workflow", "segment compare before after", ""),
    cmd("stage.export", "Go to Export", "Alt+5", "Workflow", "deliver render preset", ""),
    cmd("stage.prove", "Go to Prove", "Alt+6", "Workflow", "verify sidecar proof log recipe", ""),
    cmd("stage.next", "Next stage", "Alt+Right", "Workflow", "advance continue", "Moves forward one stage; blocked stages say what they need."),
    cmd("stage.previous", "Previous stage", "Alt+Left", "Workflow", "back", ""),
    // Project & sources
    cmd("source.add", "Add source…", "Mod+O", "Project", "ingest open import file", "Sources are hashed on add and never modified."),
    cmd("source.rehash", "Re-check source fixity", "Mod+Shift+H", "Project", "hash verify integrity", "Recomputes SHA-256 and compares it with the recorded value."),
    cmd("project.save", "Save project", "Mod+S", "Project", "write project", "Saves recipes and readings; media is never rewritten."),
    cmd("prefs.open", "Preferences…", "Mod+,", "Project", "settings options theme density", ""),
    // Viewer
    cmd("viewer.program", "Viewer: program", "P", "Viewer", "picture image", ""),
    cmd("viewer.waveform", "Viewer: waveform", "W", "Viewer", "luma levels scope", ""),
    cmd("viewer.histogram", "Viewer: histogram", "H", "Viewer", "distribution levels", ""),
    cmd("viewer.vectorscope", "Viewer: vectorscope", "U", "Viewer", "chroma saturation scope", ""),
    cmd("viewer.field_split", "Viewer: field split", "F", "Viewer", "interlace fields combing", "Shows the two fields separately — combing is obvious here."),
    cmd("viewer.temporal", "Viewer: temporal difference", "D", "Viewer", "difference motion dropout", ""),
    cmd("viewer.compare", "Toggle before/after", "\\", "Viewer", "compare split wipe", "Compares the chain against the untouched source at the same frame."),
    cmd("viewer.zoom_fit", "Zoom to fit", "Shift+F", "Viewer", "scale fit window", ""),
    cmd("viewer.zoom_100", "Zoom 100%", "Shift+1", "Viewer", "actual pixels 1:1", ""),
    cmd("viewer.safe_areas", "Toggle safe areas", "'", "Viewer", "guides title action", ""),
    // Timeline & transport
    cmd("timeline.step_back", "Previous frame", "Left", "Timeline", "frame step", ""),
    cmd("timeline.step_forward", "Next frame", "Right", "Timeline", "frame step", ""),
    cmd("timeline.jump_back", "Back one second", "Shift+Left", "Timeline", "seek", ""),
    cmd("timeline.jump_forward", "Forward one second", "Shift+Right", "Timeline", "seek", ""),
    cmd("timeline.prev_mark", "Previous finding", "N", "Timeline", "anomaly mark worry", ""),
    cmd("timeline.next_mark", "Next finding", "Shift+N", "Timeline", "anomaly mark worry", ""),
    cmd("timeline.set_in", "Set preview in-point", "I", "Timeline", "range start selection", ""),
    cmd("timeline.set_out", "Set preview out-point", "O", "Timeline", "range end selection", ""),
    cmd("timeline.clear_range", "Clear preview range", "Esc", "Timeline", "selection deselect", ""),
    cmd("timeline.graphs", "Toggle measurement lanes", "G", "Timeline", "graphs signal lanes", ""),
    // Chain
    cmd("chain.add", "Add module…", "A", "Chain", "operation step deinterlace stabilize denoise finish", "Opens the module list; each module states what it can damage."),
    cmd("chain.move_up", "Move step earlier", "Alt+Up", "Chain", "reorder order", "Order is part of the recipe: reordering is a different recipe."),
    cmd("chain.move_down", "Move step later", "Alt+Down", "Chain", "reorder order", ""),
    cmd("chain.remove", "Remove selected step", "Delete", "Chain", "delete operation", "The source file is untouched; only the chain changes."),
    cmd("chain.reset", "Reset step to defaults", "Alt+Backspace", "Chain", "defaults restore", ""),
    cmd("chain.toggle_step", "Expand or collapse step", "Enter", "Chain", "open close details", ""),
    // Preview & deliver
    cmd("preview.range_suggestion", "Suggest a preview range", "Shift+I", "Deliver", "worst frames anomalies segment", "Centres a segment on the worst measured moment, not the first seconds."),
    cmd("preview.render", "Render preview segment", "Mod+Enter", "Deliver", "segment preview render test", "Renders the chain over the selected segment, with its proof."),
    cmd("export.open", "Open export panel", "Mod+E", "Deliver", "deliver render master", ""),
    cmd("export.run", "Render export", "Mod+Shift+Enter", "Deliver", "deliver render master", "Writes the output plus its recipe and export log beside it."),
    cmd("proof.verify", "Verify output", "Mod+Shift+V", "Deliver", "check hash prove sidecar", ""),
    cmd("proof.rerun", "Re-run recorded export", "Mod+Shift+R", "Deliver", "determinism compare repeat", ""),
    // Help
    cmd("help.palette", "Command palette", "Mod+K", "Help", "search commands find run", ""),
    cmd("help.shortcuts", "Keyboard shortcuts", "?", "Help", "keys help sheet", ""),
    cmd("help.about", "About ReelDeck", "", "Help", "version build engine tools", ""),
];

/// Look up a command by its identifier.
pub fn by_id(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Commands in one group, in table order.
pub fn in_group(group: &str) -> Vec<&'static CommandSpec> {
    COMMANDS.iter().filter(|c| c.group == group).collect()
}

/// Every group name in first-appearance order — so the sheet's sections
/// are ordered by the table, not alphabetically by accident.
pub fn groups() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for c in COMMANDS {
        if !out.contains(&c.group) {
            out.push(c.group);
        }
    }
    out
}

/// Palette search: case-insensitive over the title, the group, the
/// aliases and the chord. Every word in `query` must match, so
/// "stage export" finds the Export stage command and not every export
/// command.
pub fn search(query: &str) -> Vec<&'static CommandSpec> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return COMMANDS.iter().collect();
    }
    let words: Vec<&str> = needle.split_whitespace().collect();
    COMMANDS
        .iter()
        .filter(|c| {
            let haystack = format!(
                "{} {} {} {} {}",
                c.title.to_lowercase(),
                c.group.to_lowercase(),
                c.aliases.to_lowercase(),
                c.keys.to_lowercase(),
                c.id.to_lowercase()
            );
            words.iter().all(|w| haystack.contains(w))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_well_formed_and_ids_are_unique() {
        let mut ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "command ids must be unique");
        for c in COMMANDS {
            assert!(!c.title.is_empty(), "{} has no title", c.id);
            assert!(!c.group.is_empty(), "{} has no group", c.id);
        }
    }

    #[test]
    fn displayed_chords_are_unique() {
        // Two commands on one chord is a bug the shortcuts sheet would
        // otherwise document as if it worked.
        let mut keys: Vec<&str> = COMMANDS
            .iter()
            .filter(|c| !c.keys.is_empty())
            .map(|c| c.keys)
            .collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "duplicate keyboard chord in the table");
    }

    #[test]
    fn workflow_stages_have_contiguous_number_keys() {
        for n in 1..=6 {
            let id = format!("stage.{}", ["ingest", "inspect", "decide", "preview", "export", "prove"][n - 1]);
            let spec = by_id(&id).unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(spec.keys, format!("Alt+{n}"));
        }
    }

    #[test]
    fn lookup_and_groups_read_from_the_single_table() {
        assert_eq!(by_id("export.run").map(|c| c.title), Some("Render export"));
        assert!(by_id("no.such.command").is_none());
        let groups = groups();
        assert_eq!(groups.first(), Some(&"Workflow"));
        assert!(groups.contains(&"Chain"));
        assert_eq!(
            in_group("Chain").len(),
            COMMANDS.iter().filter(|c| c.group == "Chain").count()
        );
        assert_eq!(
            groups().iter().map(|g| in_group(g).len()).sum::<usize>(),
            COMMANDS.len(),
            "every command belongs to a listed group"
        );
    }

    #[test]
    fn palette_search_matches_titles_aliases_and_chords() {
        assert!(search("").len() == COMMANDS.len());
        let export = search("export");
        assert!(export.iter().any(|c| c.id == "export.run"));
        assert!(export.iter().any(|c| c.id == "stage.export"));
        // Aliases are searched, not just titles.
        assert!(search("master").iter().any(|c| c.id == "export.run"));
        assert!(search("hash").iter().any(|c| c.id == "proof.verify"));
        // Chords are searchable, so an operator who remembers the keys
        // can find the command.
        assert!(search("mod+k").iter().any(|c| c.id == "help.palette"));
        // Multi-word queries narrow rather than widen.
        let narrowed = search("stage prove");
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].id, "stage.prove");
        assert!(search("definitely-not-a-command").is_empty());
        // Case-insensitive.
        assert_eq!(search("PREVIEW").len(), search("preview").len());
    }

    #[test]
    fn consequential_commands_carry_a_hint() {
        // The three actions whose consequences are not obvious in their
        // names must explain themselves in the palette too.
        for id in ["source.add", "preview.render", "export.run", "chain.remove"] {
            assert!(
                !by_id(id).expect("command").hint.is_empty(),
                "{id} needs a one-line hint"
            );
        }
    }
}
