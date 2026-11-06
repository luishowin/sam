//! The engine and every provider, built once and shared by the panel and the
//! CLI.

use std::sync::Arc;

use sam_core::engine::{Engine, Provider, Results};
use sam_core::rank::Candidate;
use sam_linux::apps::AppIndex;
use sam_linux::config::Config;
use sam_linux::files::FileIndex;
use sam_linux::host::LinuxHost;
use sam_linux::info::{Settings, SystemInfo};
use sam_linux::paths::PathProvider;
use sam_linux::projects::ProjectIndex;

pub struct World {
    pub engine: Engine,
    pub apps: AppIndex,
    pub projects: ProjectIndex,
    /// Shared with the threads that run file searches.
    pub files: Arc<FileIndex>,
    pub paths: PathProvider,
    pub info: SystemInfo,
    pub settings: Settings,
}

impl World {
    pub fn new() -> World {
        let config = Config::load();
        let projects = ProjectIndex::new(config.project_roots(), config.projects.depth);
        let world = World {
            engine: Engine::new(),
            apps: AppIndex::new(),
            files: Arc::new(FileIndex::new(projects.clone())),
            projects,
            paths: PathProvider,
            info: SystemInfo,
            settings: Settings,
        };
        world.projects.refresh_if_stale();
        world
    }

    /// The synchronous half of a query: everything but files.
    pub fn search(&self, text: &str) -> Results {
        let providers: [&dyn Provider; 5] =
            [&self.apps, &self.projects, &self.paths, &self.info, &self.settings];
        self.engine.search(text, &LinuxHost, &providers)
    }

    /// What the panel lists.
    pub fn panel_rows(&self, results: &Results, limit: usize) -> Vec<Candidate> {
        self.engine.rank(results, limit)
    }

}
