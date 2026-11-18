//! Launch page: where the game starts and where its files live.
//!
//! Only launchers that are actually present are listed, so the page never
//! offers a dead end. On macOS that means Obelisk; the Windows launchers
//! (Steam / Epic / Heroic) are not part of the game's runtime here and would
//! only show up as "not detected" rows.

use super::HebnixApp;
use crate::i18n::t;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A folder counts as a Rocket League install when it has the game's own
/// content folders next to the binary.
pub(super) fn looks_like_rl(dir: &Path) -> bool {
    dir.join("TAGame").is_dir() || dir.join("Binaries").is_dir()
}

/// Obelisk is installed as an app bundle; the user's own Applications folder
/// counts as well as the system one.
fn obelisk_installed() -> bool {
    let mut candidates = vec![PathBuf::from("/Applications/Obelisk.app")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/Obelisk.app"));
    }
    candidates.iter().any(|path| path.exists())
}

fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

/// Obelisk keeps real installs under `Games/<store>/<id>`, outside the prefix.
fn obelisk_game_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let games = PathBuf::from(home)
        .join("Library/Application Support/com.zagrios.obelisk/Games");
    let mut found = Vec::new();
    for store in child_dirs(&games) {
        found.extend(child_dirs(&store));
    }
    found
}

/// Every `drive_c` folder inside an Obelisk data directory, at the two depths
/// the app uses for prefixes.
fn obelisk_drive_cs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let support = PathBuf::from(home).join("Library/Application Support");
    let mut found = Vec::new();
    for name in ["obelisk", "Obelisk"] {
        let root = support.join(name);
        for dir in std::iter::once(root.clone()).chain(child_dirs(&root)) {
            if dir.file_name().is_some_and(|name| name == "drive_c") {
                found.push(dir.clone());
            }
            for child in child_dirs(&dir) {
                if child.file_name().is_some_and(|name| name == "drive_c") {
                    found.push(child);
                }
            }
        }
    }
    found
}

/// Where a Rocket League install can live inside a Wine prefix.
const GAME_IN_PREFIX: [&str; 4] = [
    "Program Files/Epic Games/rocketleague",
    "Program Files (x86)/Epic Games/rocketleague",
    "Program Files (x86)/Steam/steamapps/common/rocketleague",
    "Program Files/Steam/steamapps/common/rocketleague",
];

/// Read-only search for an existing install: the configured folder first, then
/// the platform default, then the Obelisk prefixes.
pub(super) fn detect_install(current: &str) -> Option<PathBuf> {
    if !current.trim().is_empty() {
        let configured = PathBuf::from(current);
        if looks_like_rl(&configured) {
            return Some(configured);
        }
    }
    let default = PathBuf::from(crate::config::default_rl_path());
    if looks_like_rl(&default) {
        return Some(default);
    }
    for game in obelisk_game_dirs() {
        if looks_like_rl(&game) {
            return Some(game);
        }
    }
    for drive_c in obelisk_drive_cs() {
        for relative in GAME_IN_PREFIX {
            let candidate = drive_c.join(relative);
            if looks_like_rl(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn status_dot(ui: &mut egui::Ui, ok: bool) {
    let color = if ok {
        egui::Color32::from_rgb(90, 200, 140)
    } else {
        egui::Color32::from_rgb(214, 165, 74)
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

impl HebnixApp {
    pub(super) fn render_launch(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        if crate::theme::presentation(ui.ctx()).is_none_or(|theme| theme.layout.show_page_heading) {
            ui.heading(t("launch-heading"));
            ui.label(
                egui::RichText::new(t("launch-subtitle"))
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(10.0);
        }

        let running = hebnix_sdk::process::is_rocket_league_running();
        if !looks_like_rl(Path::new(&self.config.settings.rl_path))
            && let Some(found) = detect_install(&self.config.settings.rl_path)
        {
            self.apply_game_folder(found);
        }
        let game_dir = PathBuf::from(&self.config.settings.rl_path);
        let folder_ok = looks_like_rl(&game_dir);
        let obelisk = obelisk_installed();

        let mut launch_clicked = false;
        let mut browse_clicked = false;
        let mut detect_clicked = false;

        crate::theme::card_frame(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        status_dot(ui, obelisk);
                        let title = if obelisk {
                            "Obelisk".to_string()
                        } else {
                            t("launch-nothing-detected")
                        };
                        ui.strong(title);
                        ui.label(
                            egui::RichText::new(if obelisk {
                                t("launch-detected")
                            } else {
                                t("launch-not-detected")
                            })
                            .size(11.0)
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(if obelisk {
                            t("launch-obelisk-desc")
                        } else {
                            t("launch-nothing-detected-hint")
                        })
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if running {
                        t("launch-restart")
                    } else {
                        t("launch-game")
                    };
                    // Obelisk is the launcher; the game folder only matters for
                    // patching, so a missing folder must not block launching.
                    ui.scope(|ui| {
                        if !obelisk {
                            ui.disable();
                        }
                        if crate::theme::primary_button(ui, &label).clicked() {
                            launch_clicked = true;
                        }
                    });
                });
            });
        });

        ui.add_space(10.0);

        crate::theme::card_frame(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong(t("launch-game-files"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                status_dot(ui, folder_ok);
                ui.label(if folder_ok {
                    t("launch-folder-found")
                } else {
                    t("launch-folder-missing")
                });
            });
            ui.add(
                egui::Label::new(egui::RichText::new(game_dir.to_string_lossy()).monospace())
                    .truncate(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t("launch-choose-folder")).clicked() {
                    browse_clicked = true;
                }
                if ui.button(t("launch-detect-again")).clicked() {
                    detect_clicked = true;
                }
            });
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(t("launch-game-files-hint"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            status_dot(ui, running);
            ui.label(
                egui::RichText::new(if running {
                    t("launch-status-running")
                } else {
                    t("launch-status-idle")
                })
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
            );
        });

        if browse_clicked
            && let Some(dir) = rfd::FileDialog::new()
                .set_directory(&self.config.settings.rl_path)
                .pick_folder()
        {
            self.apply_game_folder(dir);
        }
        if detect_clicked {
            match detect_install(&self.config.settings.rl_path) {
                Some(dir) => self.apply_game_folder(dir),
                None => {
                    self.console
                        .write(format!("[Launch] {}", t("launch-detect-found-nothing")));
                }
            }
        }
        if launch_clicked {
            let cfg = self.config.rl_launch.clone();
            let result = if running {
                crate::winutil::restart_rocket_league(&cfg, &game_dir)
            } else {
                crate::winutil::start_rocket_league(&cfg, &game_dir)
            };
            if let Err(error) = result {
                self.console.write(format!(
                    "[Launch] {}: {error}",
                    t("launch-open-failed")
                ));
            }
        }
    }

    /// Store a folder picked by the user or found by detection.
    fn apply_game_folder(&mut self, dir: PathBuf) {
        self.config.settings.rl_path = dir.to_string_lossy().to_string();
        self.config.settings.rl_path_confirmed = true;
        self.remember_patcher_state(&self.config.settings.rl_path);
        self.save_config();
        self.refresh_stats_api_viewer();
    }
}
