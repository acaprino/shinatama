//! The pages of one window's browser host: which are live in Moli, which were
//! discarded to stay under the live-page limit, who controls each, and which
//! local folder a local document may read. Pure state: no CDP here.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Page ids are unique for the whole run, across browser restarts, so a tab
/// that outlived a crashed host can never address a newer page.
static NEXT_PAGE: AtomicU64 = AtomicU64::new(1);

pub const DEFAULT_MAX_LIVE_PAGES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId(pub u64);

impl std::fmt::Display for PageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "p{}", self.0)
    }
}

impl std::str::FromStr for PageId {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        s.strip_prefix('p')
            .and_then(|n| n.parse().ok())
            .map(Self)
            .ok_or(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Controller {
    User,
    /// The instance id of the agent driving the page.
    Agent(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageState {
    Live { target: Option<String> },
    Discarded,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub state: PageState,
    pub controller: Controller,
    /// The agent that opened the page, kept while the user holds control so it
    /// can be handed back.
    pub agent: Option<String>,
    pub local_root: Option<PathBuf>,
    pub visible: bool,
    last_shown: u64,
}

#[derive(Debug)]
pub struct PageSet {
    max_live: usize,
    clock: u64,
    pages: HashMap<PageId, Page>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ControlError {
    UnknownPage,
    NotAnAgentPage,
}

impl PageSet {
    pub fn new(max_live: usize) -> Self {
        Self {
            max_live: max_live.max(1),
            clock: 0,
            pages: HashMap::new(),
        }
    }

    pub fn set_max_live(&mut self, max_live: usize) {
        self.max_live = max_live.max(1);
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Opens a page; returns its id and, when the live limit was reached, the
    /// page discarded to make room (never a visible one).
    pub fn open(
        &mut self,
        url: &str,
        controller: Controller,
        local_root: Option<PathBuf>,
    ) -> (PageId, Option<PageId>) {
        let discarded = self.make_room(None);
        let id = PageId(NEXT_PAGE.fetch_add(1, Ordering::Relaxed));
        let agent = match &controller {
            Controller::Agent(instance) => Some(instance.clone()),
            Controller::User => None,
        };
        let last_shown = self.tick();
        self.pages.insert(
            id,
            Page {
                url: url.to_owned(),
                title: String::new(),
                state: PageState::Live { target: None },
                controller,
                agent,
                local_root,
                visible: false,
                last_shown,
            },
        );
        (id, discarded)
    }

    fn live_count(&self) -> usize {
        self.pages
            .values()
            .filter(|p| matches!(p.state, PageState::Live { .. }))
            .count()
    }

    fn make_room(&mut self, keep: Option<PageId>) -> Option<PageId> {
        if self.live_count() < self.max_live {
            return None;
        }
        let victim = self
            .pages
            .iter()
            .filter(|(id, p)| {
                Some(**id) != keep && !p.visible && matches!(p.state, PageState::Live { .. })
            })
            .min_by_key(|(_, p)| p.last_shown)
            .map(|(id, _)| *id)?;
        self.pages.get_mut(&victim)?.state = PageState::Discarded;
        Some(victim)
    }

    pub fn get(&self, id: PageId) -> Option<&Page> {
        self.pages.get(&id)
    }

    pub fn set_target(&mut self, id: PageId, target: String) {
        if let Some(page) = self.pages.get_mut(&id) {
            page.state = PageState::Live {
                target: Some(target),
            };
        }
    }

    pub fn set_location(&mut self, id: PageId, url: &str, title: &str) {
        if let Some(page) = self.pages.get_mut(&id) {
            page.url = url.to_owned();
            page.title = title.to_owned();
        }
    }

    /// Marks a page visible. A discarded page comes back live (its caller
    /// reloads the saved URL); the returned id is any page discarded for it.
    pub fn shown(&mut self, id: PageId) -> Result<Option<PageId>, ControlError> {
        if !self.pages.contains_key(&id) {
            return Err(ControlError::UnknownPage);
        }
        let reviving = matches!(self.pages[&id].state, PageState::Discarded);
        let discarded = if reviving {
            self.make_room(Some(id))
        } else {
            None
        };
        let now = self.tick();
        let page = self.pages.get_mut(&id).ok_or(ControlError::UnknownPage)?;
        page.visible = true;
        page.last_shown = now;
        if reviving {
            page.state = PageState::Live { target: None };
        }
        Ok(discarded)
    }

    pub fn hidden(&mut self, id: PageId) {
        if let Some(page) = self.pages.get_mut(&id) {
            page.visible = false;
        }
    }

    pub fn close(&mut self, id: PageId) -> Option<Page> {
        self.pages.remove(&id)
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn ids(&self) -> Vec<PageId> {
        let mut ids: Vec<_> = self.pages.keys().copied().collect();
        ids.sort();
        ids
    }

    pub fn take_control(&mut self, id: PageId) -> Result<(), ControlError> {
        let page = self.pages.get_mut(&id).ok_or(ControlError::UnknownPage)?;
        page.controller = Controller::User;
        Ok(())
    }

    pub fn give_back(&mut self, id: PageId) -> Result<(), ControlError> {
        let page = self.pages.get_mut(&id).ok_or(ControlError::UnknownPage)?;
        let agent = page.agent.clone().ok_or(ControlError::NotAnAgentPage)?;
        page.controller = Controller::Agent(agent);
        Ok(())
    }

    /// True when `instance` may act on the page now.
    pub fn agent_may_act(&self, id: PageId, instance: &str) -> bool {
        matches!(self.pages.get(&id).map(|p| &p.controller), Some(Controller::Agent(a)) if a == instance)
    }
}

pub(crate) fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `path` canonicalised, if it lies inside `root` (Windows: case-insensitive).
pub(crate) fn inside(root: &Path, path: &Path) -> Option<PathBuf> {
    let (root, path) = (
        std::fs::canonicalize(root).ok()?,
        std::fs::canonicalize(path).ok()?,
    );
    let fold = |p: &Path| p.to_string_lossy().to_lowercase();
    let (r, p) = (fold(&root), fold(&path));
    let within = p == r
        || p.strip_prefix(&r)
            .is_some_and(|rest| rest.starts_with('\\') || rest.starts_with('/'));
    within.then_some(path)
}

/// Content type of a served local file, by extension.
pub fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `D:\a b\c.html` → `file:/// Lowercase hex of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> Controller {
        Controller::Agent("i_a".into())
    }

    #[test]
    fn ninth_page_discards_least_recently_shown_hidden() {
        let mut set = PageSet::new(8);
        let ids: Vec<_> = (0..8)
            .map(|i| {
                set.open(&format!("https://x/{i}"), Controller::User, None)
                    .0
            })
            .collect();
        set.shown(ids[0]).unwrap();
        set.shown(ids[3]).unwrap();
        set.hidden(ids[3]);
        let (_, discarded) = set.open("https://x/9", Controller::User, None);
        assert_eq!(
            discarded,
            Some(ids[1]),
            "oldest hidden page goes; the visible one never does"
        );
        assert_eq!(set.get(ids[1]).unwrap().state, PageState::Discarded);
        assert_eq!(set.get(ids[1]).unwrap().url, "https://x/1");
    }

    #[test]
    fn visible_pages_are_never_discarded() {
        let mut set = PageSet::new(2);
        let (a, _) = set.open("https://a", Controller::User, None);
        let (b, _) = set.open("https://b", Controller::User, None);
        set.shown(a).unwrap();
        set.shown(b).unwrap();
        let (_, discarded) = set.open("https://c", Controller::User, None);
        assert_eq!(discarded, None);
    }

    #[test]
    fn discarded_page_reloads_when_shown() {
        let mut set = PageSet::new(1);
        let (a, _) = set.open("https://a", Controller::User, None);
        let (b, discarded) = set.open("https://b", Controller::User, None);
        assert_eq!(discarded, Some(a));
        let again = set.shown(a).unwrap();
        assert_eq!(again, Some(b));
        assert_eq!(set.get(a).unwrap().state, PageState::Live { target: None });
    }

    #[test]
    fn agent_actions_need_user_give_back() {
        let mut set = PageSet::new(8);
        let (id, _) = set.open("https://a", agent(), None);
        assert!(set.agent_may_act(id, "i_a"));
        assert!(!set.agent_may_act(id, "i_b"));
        set.take_control(id).unwrap();
        assert!(!set.agent_may_act(id, "i_a"));
        set.give_back(id).unwrap();
        assert!(set.agent_may_act(id, "i_a"));
    }

    #[test]
    fn a_user_page_cannot_be_given_to_an_agent() {
        let mut set = PageSet::new(8);
        let (id, _) = set.open("https://a", Controller::User, None);
        assert_eq!(set.give_back(id), Err(ControlError::NotAnAgentPage));
    }

    #[test]
    fn page_ids_round_trip() {
        assert_eq!("p42".parse::<PageId>(), Ok(PageId(42)));
        assert_eq!(PageId(42).to_string(), "p42");
        assert!("42".parse::<PageId>().is_err());
    }

    #[test]
    fn page_ids_never_repeat_across_page_sets() {
        let (a, _) = PageSet::new(8).open("https://a", Controller::User, None);
        let (b, _) = PageSet::new(8).open("https://b", Controller::User, None);
        assert_ne!(
            a, b,
            "a restarted browser must not hand out an id an old tab still holds"
        );
    }
}
