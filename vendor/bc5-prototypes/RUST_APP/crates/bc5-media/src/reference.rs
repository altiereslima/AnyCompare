//! Audit-only reference for BC5 5.2.7.32913 Media Compare.
//!
//! Sources: BCompare.chm (supplied original help), Scooter Software's
//! commandsmedia.html/viewother.html and the exact TMEDIAACTIONS/TMEDIAVIEW
//! resources from the supplied Windows x64 BCompare.exe.
//!
//! A documented or resource-observed command is not proof that our prototype
//! implements it or reproduces BC5's internals. This catalog intentionally
//! separates evidence from implementation claims.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementationState {
    /// A Rust path exists, but its behavior has not been compared to BC5 live.
    PrototypeUnverified,
    /// No BC5-equivalent implementation in the supplied Rust code.
    Pending,
}

#[derive(Debug, Clone, Copy)]
pub struct MediaCommandReference {
    pub id: &'static str,
    pub bc5_label: &'static str,
    pub group: &'static str,
    pub implementation: ImplementationState,
}

/// UI and command inventory grounded in BC5 5.x help. Statuses describe
/// *our Rust implementation* as of the BC5 Media Stage 4 handoff.
pub const MEDIA_COMMANDS: &[MediaCommandReference] = &[
    MediaCommandReference { id: "session.save", bc5_label: "Save Session", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.save_as", bc5_label: "Save Session As", group: "Session", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "session.settings", bc5_label: "Session Settings", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.locked", bc5_label: "Locked", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.clear", bc5_label: "Clear Session", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.swap", bc5_label: "Swap Sides", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.reload", bc5_label: "Reload Files", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.recompare", bc5_label: "Recompare Files", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.report", bc5_label: "Media Compare Report", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.info", bc5_label: "Media Compare Info", group: "Session", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "session.play_both", bc5_label: "Play Both", group: "Session", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "session.compare_using", bc5_label: "Compare Files Using", group: "Session", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "session.compare_parent", bc5_label: "Compare Parent Folders", group: "Session", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "file.open", bc5_label: "Open File", group: "File", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "file.play", bc5_label: "Play", group: "File", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "file.open_with", bc5_label: "Open With > Associated Application", group: "File", implementation: ImplementationState::PrototypeUnverified },
    // Changelog oficial da v5 (5.0.0.29773): Open Clipboard disponível
    // também no Media Compare. Implementação permanece pendente.
    MediaCommandReference { id: "file.open_clipboard", bc5_label: "Open Clipboard", group: "File", implementation: ImplementationState::Pending },
    // A v5 abre Hex/Picture Compare ao clicar duas vezes numa tag.
    MediaCommandReference { id: "tag.open_hex_or_picture", bc5_label: "Double-click tag -> Hex or Picture Compare", group: "Tags", implementation: ImplementationState::Pending },
    // Commands that were missing from earlier inventories; verified in
    // the official v5 Media Compare Commands help, NOT implemented 1:1.
    MediaCommandReference { id: "file.open_local", bc5_label: "Open File on File System", group: "File", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "file.open_profile", bc5_label: "Open File Using Profile", group: "File", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "file.explorer", bc5_label: "Explorer", group: "File", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "edit.copy", bc5_label: "Copy", group: "Edit", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "edit.select_all", bc5_label: "Select All", group: "Edit", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "search.next_difference_item", bc5_label: "Next Difference Item", group: "Search", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "search.previous_difference_item", bc5_label: "Previous Difference Item", group: "Search", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "search.next_difference_files", bc5_label: "Next Difference Files", group: "Search", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "search.previous_difference_files", bc5_label: "Previous Difference Files", group: "Search", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "search.copy_right_next", bc5_label: "Copy File to Right and Open Next Difference", group: "Search", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "search.copy_left_next", bc5_label: "Copy File to Left and Open Next Difference", group: "Search", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "search.find", bc5_label: "Find", group: "Search", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.display_font", bc5_label: "Display Font", group: "View", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "view.increase_font", bc5_label: "Increase Display Font Size", group: "View", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "view.decrease_font", bc5_label: "Decrease Display Font Size", group: "View", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "view.reset_font", bc5_label: "Reset Display Font Size", group: "View", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "view.toolbar", bc5_label: "Toolbar", group: "View", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "file.find_next", bc5_label: "Find Next", group: "File", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "file.find_previous", bc5_label: "Find Previous", group: "File", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.all", bc5_label: "Show All", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.differences", bc5_label: "Show Differences", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.same", bc5_label: "Show Same", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.none", bc5_label: "Show None", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.ignore_unimportant", bc5_label: "Ignore Unimportant Differences", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.important", bc5_label: "Item Is Important", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.whitespace", bc5_label: "Visible Whitespace", group: "View", implementation: ImplementationState::Pending },
    MediaCommandReference { id: "view.side_by_side", bc5_label: "Side-by-Side Layout", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.over_under", bc5_label: "Over-Under Layout", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.text_details", bc5_label: "Text Details", group: "View", implementation: ImplementationState::PrototypeUnverified },
    MediaCommandReference { id: "view.file_info", bc5_label: "File Info", group: "View", implementation: ImplementationState::PrototypeUnverified },
];

/// Four locally declared Delphi actions found in the exact TMEDIAACTIONS
/// binary DFM in BOTH x86/x64 v5.2.7.32913. Other commands are inherited
/// from shared action trees; this is not a method list nor a behavioral proof.
#[derive(Debug, Clone, Copy)]
pub struct MediaResourceAction {
    pub object_name: &'static str,
    pub label: &'static str,
    pub implementation: ImplementationState,
}
pub const TMEDIAACTIONS_SHA256: &str =
    "9bb20a2369163e2a545385e2d5bc0914bee2bc3fd031f4ccd87bc30461e686a6";
pub const MEDIA_LOCAL_RESOURCE_ACTIONS: &[MediaResourceAction] = &[
    MediaResourceAction { object_name:"Play2Action", label:"Play Both", implementation:ImplementationState::Pending },
    MediaResourceAction { object_name:"PlayAction", label:"Play", implementation:ImplementationState::Pending },
    MediaResourceAction { object_name:"IsSigAction", label:"Item Is Important", implementation:ImplementationState::PrototypeUnverified },
    MediaResourceAction { object_name:"ShowDetailsAction", label:"Text Details", implementation:ImplementationState::PrototypeUnverified },
];

/// Confirmed container families from BC5 official v5 help. This does not
/// promise every tag/profile/extension within a container behaves identically.
pub const DOCUMENTED_FAMILIES: &[&str] = &["FLAC", "MP3", "MP4/AAC"];


/// Nomes recuperados diretamente de RTTI de ambas as arquiteturas do EXE.
/// Um nome de propriedade/método não revela o algoritmo, parâmetros ou efeito.
pub const OBSERVED_RTTI_MEDIA: &[(&str, &[&str])] = &[
    ("TMediaRules", &["AKeyTag", "AValTag", "KeyHigh", "NameStrs", "ValHigh"]),
    ("TMediaContent", &["Destroy", "NeedsChanged", "TagKind"]),
    ("TMediaMapTree", &["GetIsPicture", "ANode", "IsPicture"]),
    ("TMediaCompareSession", &["ClassCaption", "ClassLongHint", "ClassShortCaption", "ClassTitleEn"]),
    ("TMediaCompareSessionState", &["MediaSession", "DisplayFilter"]),
];

/// Ações específicas presentes em TMEDIAACTIONS (Delphi DFM) nas duas arquiteturas.
pub const OBSERVED_DFM_MEDIA_ACTIONS: &[&str] = &[
    "Play2Action", "PlayAction", "IsSigAction", "ShowDetailsAction",
];

/// SHA-256 of the x64 executable whose resources were inspected.
pub const REFERENCE_BCOMPARE_X64_SHA256: &str =
    "c8f68dc33424c02a50f432ce7327b14fffd78ea3b1ca4b14ac52e4c275eeb086";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn no_duplicate_command_ids_or_unearned_complete_flags() {
        let mut seen = BTreeSet::new();
        for command in MEDIA_COMMANDS {
            assert!(seen.insert(command.id));
            assert!(!command.bc5_label.is_empty());
            assert!(matches!(command.implementation,
                ImplementationState::PrototypeUnverified | ImplementationState::Pending));
        }
        assert!(MEDIA_COMMANDS.iter().any(|c| c.id == "session.play_both"
            && c.implementation == ImplementationState::Pending));
        assert_eq!(DOCUMENTED_FAMILIES.len(), 3);
        for id in ["file.open_clipboard", "tag.open_hex_or_picture"] {
            assert!(MEDIA_COMMANDS.iter().any(|c| c.id==id &&
                c.implementation==ImplementationState::Pending));
        }
    }
}

#[cfg(test)]
mod stage22_resource_checks {
    use super::*;
    #[test]
    fn local_action_names_are_unique_and_exactly_four() {
        let mut names=std::collections::BTreeSet::new();
        for a in MEDIA_LOCAL_RESOURCE_ACTIONS { assert!(names.insert(a.object_name)); }
        assert_eq!(names.len(),4);
        assert_eq!(TMEDIAACTIONS_SHA256.len(),64);
    }
}
