//! UPK patching and item swapping implementation.
//!
//! Keeping these modules together makes the patching boundary explicit while
//! the compatibility re-exports in main.rs preserve the existing call sites.
pub mod actions;
pub mod anthem_patcher;
pub mod background_merger;
pub mod backup_guard;
pub mod ball;
pub mod ball_visual;
pub mod boost_patcher;
pub mod bots_freeplay;
pub mod car_patcher;
pub mod catalog;
pub mod celebration;
pub mod chat_export;
pub mod chat_paste;
pub mod colours;
pub mod cosmetic_thumbnail;
pub mod cosmetic_upk;
pub mod decal_patcher;
pub mod decal_variant;
pub mod delete_backups;
pub mod dispatcher;
pub mod forfeit_command;
pub mod game_path;
pub mod game_update;
pub mod goal_explosion;
pub mod goal_explosion_volume;
pub mod heatseeker;
pub mod instant_suite;
pub mod item_name;
pub mod lan_commands;
pub mod map_swap;
pub mod menu_map;
pub mod painted_swap;
pub mod patch_core;
pub mod profile_pictures;
pub mod rl_font;
pub mod script_patch;
pub mod speed_patch;
pub mod swapper;
pub mod thumbnail_patcher;
pub mod title_icons;
pub mod title_patcher;
pub mod uninstall;
pub mod upk_keys;
pub mod upk_package;
pub mod wheel_alignment;
pub mod workshop_host;
pub mod workshop_lan_trial;
pub mod workshop_menu;
pub mod workshop_menu_ui;
pub mod workshop_slots;

use crate::config::PatchSource;
use crate::i18n::t;

pub(crate) fn patch_source_selector(ui: &mut eframe::egui::Ui, source: &mut PatchSource) -> bool {
    ui.horizontal(|ui| {
        let mut changed = ui
            .selectable_value(
                source,
                PatchSource::Catalog,
                t("patch-source-selector-catalog"),
            )
            .changed();
        ui.label("|");
        changed |= ui
            .selectable_value(
                source,
                PatchSource::Custom,
                t("patch-source-selector-local"),
            )
            .changed();
        changed
    })
    .inner
}
pub mod upk_history;
