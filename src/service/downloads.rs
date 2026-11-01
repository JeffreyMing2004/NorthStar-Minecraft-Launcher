use super::*;
impl GameService {
    pub fn run_queue(&mut self, settings: &Settings, ctx: &egui::Context) {
        self.paused = settings.source.paused.clone();
        let root = store::game_directory(settings);
        if !self.busy() && self.queue.active.is_none() && !self.queue.root_matches(&root) {
            self.queue = crate::download_tasks::Queue::load(&root);
        }
        if self.busy()
            || self.game.is_some()
            || self.resume_launch.is_some()
            || self.launch_prompt.is_some()
            || !self.queue.enabled
            || self.queue.error.is_some()
        {
            return;
        }
        if let Some(index) = self
            .queue
            .items
            .iter()
            .position(|i| i.state == crate::download_tasks::State::Queued)
        {
            let item = self.queue.items[index].clone();
            self.queue.items[index].state = crate::download_tasks::State::Running;
            self.queue.active = Some(item.id);
            self.queue.save();
            if self.queue.error.is_some() {
                return;
            }
            self.start_install(item.entry, item.loader, settings, ctx);
        }
    }
}
