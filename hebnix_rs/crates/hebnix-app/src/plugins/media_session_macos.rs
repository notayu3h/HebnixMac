//! now-playing info for plugins ("media" tab), macOS.
//!
//! The Linux build reads MPRIS over D-Bus, the Windows one the SMTC session.
//! macOS exposes the same idea through `MPNowPlayingInfoCenter`, but only to
//! the process that owns the player, so reading another app's now-playing
//! needs a private/undocumented route. Rather than ship something that reads
//! like it works and never updates, this reports "nothing playing" until a
//! supported route exists.

#[derive(Clone, Default)]
pub struct MediaSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub app: String,
    pub status: &'static str,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub updated_unix_ms: i64,
    pub thumb_rev: u64,
}

/// latest now-playing info, None when nothing has a media session
pub fn snapshot() -> Option<MediaSnapshot> {
    None
}

/// raw cover bytes for the current snapshot's thumb_rev, None until one loads
pub fn thumbnail() -> Option<Vec<u8>> {
    None
}
