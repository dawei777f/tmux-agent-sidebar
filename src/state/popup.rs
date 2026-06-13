use super::AppState;

/// At-most-one popup state for the sidebar.
#[derive(Debug, Clone, Default)]
pub enum PopupState {
    #[default]
    None,
    Repo {
        selected: usize,
        area: Option<ratatui::layout::Rect>,
    },
    Notices {
        area: Option<ratatui::layout::Rect>,
    },
}

impl PopupState {
    pub fn set_repo_area(&mut self, rect: Option<ratatui::layout::Rect>) {
        if let Self::Repo { area, .. } = self {
            *area = rect;
        }
    }

    pub fn set_notices_area(&mut self, rect: Option<ratatui::layout::Rect>) {
        if let Self::Notices { area } = self {
            *area = rect;
        }
    }
}

impl AppState {
    pub fn is_repo_popup_open(&self) -> bool {
        matches!(self.popup, PopupState::Repo { .. })
    }

    pub fn repo_popup_selected(&self) -> usize {
        match &self.popup {
            PopupState::Repo { selected, .. } => *selected,
            _ => 0,
        }
    }

    pub fn set_repo_popup_selected(&mut self, n: usize) {
        if let PopupState::Repo { selected, .. } = &mut self.popup {
            *selected = n;
        }
    }

    pub fn repo_popup_area(&self) -> Option<ratatui::layout::Rect> {
        match &self.popup {
            PopupState::Repo { area, .. } => *area,
            _ => None,
        }
    }

    pub fn toggle_repo_popup(&mut self) {
        if self.is_repo_popup_open() {
            self.close_repo_popup();
            return;
        }
        let names = self.repo_names();
        let selected = match &self.global.repo_filter {
            super::RepoFilter::All => 0,
            super::RepoFilter::Repo(name) => names.iter().position(|n| n == name).unwrap_or(0),
        };
        self.popup = PopupState::Repo {
            selected,
            area: None,
        };
    }

    pub fn confirm_repo_popup(&mut self) {
        let selected = self.repo_popup_selected();
        let names = self.repo_names();
        if let Some(name) = names.get(selected) {
            self.global.repo_filter = if selected == 0 {
                super::RepoFilter::All
            } else {
                super::RepoFilter::Repo(name.clone())
            };
        }
        self.popup = PopupState::None;
        self.global.save_repo_filter();
        self.rebuild_row_targets();
    }

    pub fn close_repo_popup(&mut self) {
        self.popup = PopupState::None;
    }

    pub fn is_notices_popup_open(&self) -> bool {
        matches!(self.popup, PopupState::Notices { .. })
    }

    pub fn notices_popup_area(&self) -> Option<ratatui::layout::Rect> {
        match &self.popup {
            PopupState::Notices { area } => *area,
            _ => None,
        }
    }

    pub fn toggle_notices_popup(&mut self) {
        if self.is_notices_popup_open() {
            self.close_notices_popup();
        } else {
            self.popup = PopupState::Notices { area: None };
        }
    }

    pub fn close_notices_popup(&mut self) {
        self.popup = PopupState::None;
        self.notices.copy_targets.clear();
        self.notices.copied_at = None;
    }
}
