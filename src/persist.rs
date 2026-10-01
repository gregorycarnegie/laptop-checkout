//! When to save the database, as a pure state machine.
//!
//! Writes mark the data dirty and start a short timer. Only the latest timer
//! saves (so a burst of writes is saved once), a save that finds another one
//! running is retried, and a failed save leaves the data dirty.
//!
//! ```
//! use laptop_checkout::persist::SaveState;
//! let mut s = SaveState::default();
//! let first = s.mark_dirty();
//! let second = s.mark_dirty();
//! assert!(!s.is_latest(first));
//! assert!(s.is_latest(second));
//! assert!(s.begin());
//! s.finish(Ok(()), 1_000);
//! assert!(!s.has_unsaved());
//! ```

/// Where the database is being saved, as shown in the app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DbStatus {
    pub fs_supported: bool,
    pub file_name: Option<String>,
    pub file_connected: bool,
    pub file_needs_permission: bool,
    pub dirty: bool,
    pub saving: bool,
    pub last_saved: Option<i64>,
    pub error: Option<String>,
}

/// What the browser reports about the linked `.sqlite` file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileInfo {
    pub fs_supported: bool,
    pub file_name: Option<String>,
    pub writable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveState {
    generation: u64,
    dirty: bool,
    saving: bool,
    last_saved: Option<i64>,
    error: Option<String>,
}

impl SaveState {
    /// Records a change. Returns a ticket; only the newest ticket should save.
    pub fn mark_dirty(&mut self) -> u64 {
        self.dirty = true;
        self.generation += 1;
        self.generation
    }

    /// Starts over for a newly connected database. Tickets handed out before
    /// go stale, so their timers can't save; a save already running still
    /// counts until it finishes.
    pub fn restart(&mut self) {
        *self = SaveState { generation: self.generation + 1, saving: self.saving, ..SaveState::default() };
    }

    pub fn is_latest(&self, ticket: u64) -> bool {
        ticket == self.generation
    }

    /// Starts a save. Returns false if one is already running (retry later).
    pub fn begin(&mut self) -> bool {
        if self.saving {
            return false;
        }
        self.saving = true;
        self.dirty = false;
        true
    }

    pub fn finish(&mut self, result: Result<(), String>, now: i64) {
        self.saving = false;
        match result {
            Ok(()) => {
                self.last_saved = Some(now);
                self.error = None;
            }
            Err(e) => {
                self.dirty = true;
                self.error = Some(format!("Last save failed: {e}"));
            }
        }
    }

    /// Records a save made some other way (e.g. creating a file).
    pub fn saved_at(&mut self, now: i64) {
        self.last_saved = Some(now);
    }

    pub fn has_unsaved(&self) -> bool {
        self.dirty || self.saving
    }

    pub fn status(&self, file: FileInfo) -> DbStatus {
        let linked = file.file_name.is_some();
        DbStatus {
            fs_supported: file.fs_supported,
            file_connected: linked && file.writable,
            file_needs_permission: linked && !file.writable,
            file_name: file.file_name,
            dirty: self.dirty,
            saving: self.saving,
            last_saved: self.last_saved,
            error: self.error.clone(),
        }
    }
}

/// The label for the save indicator: (CSS class, text).
pub fn badge(s: &DbStatus, stamp: impl Fn(i64) -> String) -> (&'static str, String) {
    if s.error.is_some() {
        ("pill late", "Not saved".into())
    } else if s.saving || s.dirty {
        ("pill soon", "Saving…".into())
    } else if let Some(t) = s.last_saved {
        ("pill ok", format!("Saved {}", stamp(t)))
    } else {
        ("pill ok", "Saved".into())
    }
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn file(name: Option<&str>, writable: bool) -> FileInfo {
        FileInfo { fs_supported: true, file_name: name.map(String::from), writable }
    }

    #[test]
    fn a_new_state_has_nothing_to_save() {
        assert!(!SaveState::default().has_unsaved());
    }

    #[test]
    fn marking_dirty_means_there_is_something_to_save() {
        let mut s = SaveState::default();
        s.mark_dirty();
        assert!(s.has_unsaved());
    }

    #[test]
    fn only_the_newest_ticket_saves() {
        let mut s = SaveState::default();
        let first = s.mark_dirty();
        let second = s.mark_dirty();
        assert_eq!((s.is_latest(first), s.is_latest(second)), (false, true));
    }

    #[test]
    fn a_save_cannot_start_while_another_runs() {
        let mut s = SaveState::default();
        s.mark_dirty();
        assert!(s.begin());
        assert!(!s.begin());
    }

    #[test]
    fn a_running_save_still_counts_as_unsaved() {
        let mut s = SaveState::default();
        s.mark_dirty();
        s.begin();
        assert!(s.has_unsaved());
    }

    #[test]
    fn a_successful_save_records_the_time_and_clears_errors() {
        let mut s = SaveState::default();
        s.mark_dirty();
        s.begin();
        s.finish(Err("disk full".into()), 1);
        s.begin();
        s.finish(Ok(()), 42);
        let status = s.status(FileInfo::default());
        assert_eq!((status.last_saved, status.error, status.dirty, status.saving), (Some(42), None, false, false));
    }

    #[test]
    fn a_failed_save_stays_dirty_and_explains_why() {
        let mut s = SaveState::default();
        s.mark_dirty();
        s.begin();
        s.finish(Err("disk full".into()), 1);
        let status = s.status(FileInfo::default());
        assert_eq!(status.error.as_deref(), Some("Last save failed: disk full"));
        assert!(status.dirty);
        assert_eq!(status.last_saved, None);
    }

    #[test]
    fn changes_during_a_save_are_not_lost() {
        let mut s = SaveState::default();
        s.mark_dirty();
        s.begin();
        s.mark_dirty();
        s.finish(Ok(()), 5);
        assert!(s.has_unsaved());
    }

    #[test]
    fn restarting_makes_earlier_tickets_stale() {
        let mut s = SaveState::default();
        let old = s.mark_dirty();
        s.finish(Err("disk full".into()), 5);
        s.restart();
        assert!(!s.is_latest(old));
        assert_eq!(s.status(FileInfo::default()), DbStatus::default());
        let new = s.mark_dirty();
        assert!(s.is_latest(new) && new > old);
    }

    #[test]
    fn restarting_keeps_a_running_save() {
        let mut s = SaveState::default();
        s.mark_dirty();
        s.begin();
        s.restart();
        assert!(s.status(FileInfo::default()).saving);
    }

    #[test]
    fn saved_at_records_an_outside_save() {
        let mut s = SaveState::default();
        s.saved_at(9);
        assert_eq!(s.status(FileInfo::default()).last_saved, Some(9));
    }

    #[rstest]
    #[case::no_file(None, false, false, false)]
    #[case::linked_and_allowed(Some("loans.sqlite"), true, true, false)]
    #[case::linked_but_needs_permission(Some("loans.sqlite"), false, false, true)]
    fn status_describes_the_linked_file(
        #[case] name: Option<&str>,
        #[case] writable: bool,
        #[case] connected: bool,
        #[case] needs_permission: bool,
    ) {
        let s = SaveState::default().status(file(name, writable));
        assert_eq!((s.file_connected, s.file_needs_permission), (connected, needs_permission));
        assert_eq!(s.file_name.as_deref(), name);
        assert!(s.fs_supported);
    }

    #[test]
    fn status_reports_a_save_in_progress() {
        let mut s = SaveState::default();
        s.mark_dirty();
        s.begin();
        assert!(s.status(FileInfo::default()).saving);
    }

    fn stamp(ms: i64) -> String {
        format!("at {ms}")
    }

    #[rstest]
    #[case::error(DbStatus { error: Some("x".into()), dirty: true, ..Default::default() }, "pill late", "Not saved")]
    #[case::dirty(DbStatus { dirty: true, ..Default::default() }, "pill soon", "Saving…")]
    #[case::saving(DbStatus { saving: true, ..Default::default() }, "pill soon", "Saving…")]
    #[case::saved(DbStatus { last_saved: Some(7), ..Default::default() }, "pill ok", "Saved at 7")]
    #[case::never_changed(DbStatus::default(), "pill ok", "Saved")]
    fn the_badge_shows_the_most_important_state(#[case] s: DbStatus, #[case] class: &str, #[case] text: &str) {
        assert_eq!(badge(&s, stamp), (class, text.to_string()));
    }
}
