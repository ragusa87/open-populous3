//! Loading: the boot jobs (`BootJob`, CPU work on a worker thread then installed on the main thread), the assets
//! Bevy loads in the background (`TrackedAssets`), and the `Loading` state that waits for both before the menu or a
//! game. `LoadTarget` says what a loading pass is for; `Loaded` is sent when it ends, the moment captures and
//! profiles start from (`dev`, `tools/trace_top.py`). The screen showing it is `screen.rs`. A game start's own map
//! build (`GameStep::Building`) runs the same off-thread-then-polled shape as a `BootJob`, driven from `menu.rs`
//! instead of this module (`menu::spawn_game_build`/`poll_new_game_build`) since it isn't a fixed boot-time plugin.

pub mod screen;

use crate::menu::{AppState, Start};
use bevy::prelude::*;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Work done once at boot: `run` on a worker thread (CPU only, no ECS access), its output then installed in the world
/// on the main thread (GPU uploads, resources) one chunk per frame (`install_step`), so a job with a lot to install
/// never blocks the loading screen (or the whole app: no frames drawn means no window repaint, no swapchain
/// present, and on some drivers a long enough stall is itself what makes the game crash rather than just freeze)
/// for longer than one chunk's worth of work (TODO.md "Loading screen freezes").
pub trait BootJob: Send + Sync + 'static {
    /// Shown on the loading screen.
    const LABEL: &'static str;
    /// What the job needs from the world, copied out of it.
    type Input: Send + 'static;
    /// Its result, handed to `install_step`. `Sync` too: `JobSlot` (a `Resource`) holds it directly while
    /// chunking it in, between `Task<Output>`'s own poll and the last `install_step` call.
    type Output: Send + Sync + 'static;
    fn input(world: &World) -> Self::Input;
    /// How many `Progress::step`s `run` makes.
    fn steps(input: &Self::Input) -> u32;
    fn run(input: Self::Input, progress: Progress) -> Self::Output;
    /// How many `install_step` calls `output` will take, for the loading screen's own install-phase progress.
    /// Defaults to one: a plain, unchunked install has nothing finer to report.
    fn install_steps(output: &Self::Output) -> u32 {
        let _ = output;
        1
    }
    /// Installs one chunk of `output` into `world`; called again next frame while chunks remain, so the main
    /// thread never does more than one chunk's install work before the next frame can draw.
    fn install_step(output: Self::Output, world: &mut World) -> InstallProgress<Self::Output>;
}

/// What a chunked install has left to do, returned by `BootJob::install_step`.
pub enum InstallProgress<T> {
    /// More to install: call `install_step` again next frame with this.
    More(T),
    Done,
}

/// Steps a job has done: it can only go forward, one step at a time.
#[derive(Clone, Debug, Default)]
pub struct Progress(Arc<AtomicU32>);

impl Progress {
    pub fn step(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub fn done(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobPhase {
    Running,
    Installing,
    Done,
}

/// One job as the loading screen sees it.
#[derive(Clone, Debug)]
pub struct JobStatus {
    pub label: &'static str,
    pub progress: Progress,
    pub total: u32,
    pub phase: JobPhase,
    /// How many `install_step` chunks are done, once `phase` is `Installing`.
    pub install_progress: Progress,
    pub install_total: u32,
}

/// Every boot job's status, in the order they were registered.
#[derive(Resource, Default, Debug)]
pub struct BootStatus(pub Vec<JobStatus>);

impl BootStatus {
    /// How far the boot is, 0 to 1: each job weighs the same, a done one counts whole. `Running` fills 0 to 0.9
    /// of a job's own share, `Installing` the last 0.9 to 1 (by its chunks done) - so a job with real install
    /// work to do still visibly advances instead of sitting still at a near-100% cap until it finishes.
    pub fn fraction(&self) -> f32 {
        if self.0.is_empty() {
            return 1.0;
        }
        let job = |j: &JobStatus| match j.phase {
            JobPhase::Done => 1.0,
            JobPhase::Installing => 0.9 + 0.1 * (j.install_progress.done() as f32 / j.install_total.max(1) as f32).min(1.0),
            JobPhase::Running => (j.progress.done() as f32 / j.total.max(1) as f32).min(0.9),
        };
        self.0.iter().map(job).sum::<f32>() / self.0.len() as f32
    }

    pub fn is_done(&self) -> bool {
        self.0.iter().all(|j| j.phase == JobPhase::Done)
    }

    /// The first job not done yet.
    pub fn current(&self) -> Option<&JobStatus> {
        self.0.iter().find(|j| j.phase != JobPhase::Done)
    }

    fn set_phase(&mut self, label: &'static str, phase: JobPhase) {
        if let Some(j) = self.0.iter_mut().find(|j| j.label == label) {
            j.phase = phase;
        }
    }

    /// `Running` -> `Installing`, now that `total` chunks of install work are known.
    fn begin_install(&mut self, label: &'static str, total: u32) {
        if let Some(j) = self.0.iter_mut().find(|j| j.label == label) {
            j.phase = JobPhase::Installing;
            j.install_total = total.max(1);
        }
    }

    fn step_install(&mut self, label: &'static str) {
        if let Some(j) = self.0.iter_mut().find(|j| j.label == label) {
            j.install_progress.step();
        }
    }
}

/// A job's slot: its task while it runs, then its output while `install_step` is chunking it in.
#[derive(Resource)]
enum JobSlot<J: BootJob> {
    Pending,
    Running(Task<J::Output>),
    Installing(J::Output),
    Done,
}

/// Registers the boot job `J`: started the first time `Loading` is entered, installed once its task ends.
pub struct BootJobPlugin<J>(PhantomData<J>);

impl<J> Default for BootJobPlugin<J> {
    fn default() -> Self {
        BootJobPlugin(PhantomData)
    }
}

impl<J: BootJob> Plugin for BootJobPlugin<J> {
    fn build(&self, app: &mut App) {
        app.init_resource::<BootStatus>()
            .insert_resource(JobSlot::<J>::Pending)
            .add_systems(OnEnter(AppState::Loading), start_job::<J>)
            .add_systems(Update, poll_job::<J>.run_if(in_state(AppState::Loading)).before(LoadingSystems));
    }
}

fn start_job<J: BootJob>(world: &mut World) {
    if !matches!(world.resource::<JobSlot<J>>(), JobSlot::Pending) {
        return;
    }
    let input = J::input(world);
    let progress = Progress::default();
    let total = J::steps(&input);
    world.resource_mut::<BootStatus>().0.push(JobStatus {
        label: J::LABEL,
        progress: progress.clone(),
        total,
        phase: JobPhase::Running,
        install_progress: Progress::default(),
        install_total: 1,
    });
    let task = AsyncComputeTaskPool::get().spawn(async move { J::run(input, progress) });
    world.insert_resource(JobSlot::<J>::Running(task));
}

/// Advances job `J` by at most one step: polls its task (then starts installing once it finishes), or installs
/// one chunk of an already-running install - never both in the same frame, and never more than one chunk, so a
/// job with a lot of either never costs more than one frame.
fn poll_job<J: BootJob>(world: &mut World) {
    match std::mem::replace(&mut *world.resource_mut::<JobSlot<J>>(), JobSlot::Done) {
        JobSlot::Running(mut task) => {
            *world.resource_mut::<JobSlot<J>>() = match block_on(poll_once(&mut task)) {
                Some(output) => {
                    let total = J::install_steps(&output);
                    world.resource_mut::<BootStatus>().begin_install(J::LABEL, total);
                    JobSlot::Installing(output)
                }
                None => JobSlot::Running(task),
            };
        }
        JobSlot::Installing(output) => {
            let t = std::time::Instant::now();
            let progress = J::install_step(output, world);
            let ms = t.elapsed().as_secs_f32() * 1000.0;
            world.resource_mut::<BootStatus>().step_install(J::LABEL);
            match progress {
                InstallProgress::More(next) => {
                    trace!("boot: {} installed a chunk in {ms:.0} ms", J::LABEL);
                    *world.resource_mut::<JobSlot<J>>() = JobSlot::Installing(next);
                }
                InstallProgress::Done => {
                    info!("boot: {} finished installing (last chunk {ms:.0} ms)", J::LABEL);
                    world.resource_mut::<BootStatus>().set_phase(J::LABEL, JobPhase::Done);
                    *world.resource_mut::<JobSlot<J>>() = JobSlot::Done;
                }
            }
        }
        other => *world.resource_mut::<JobSlot<J>>() = other,
    }
}

/// Handles of assets Bevy loads in the background that the boot waits for (glTF scenes).
#[derive(Resource, Default)]
pub struct TrackedAssets(pub Vec<UntypedHandle>);

impl TrackedAssets {
    /// How many are loaded (or failed: a missing asset never blocks the boot), out of how many.
    pub fn count(&self, server: &AssetServer) -> (usize, usize) {
        let settled = |h: &&UntypedHandle| server.is_loaded_with_dependencies(h.id()) || matches!(server.get_load_state(h.id()), Some(bevy::asset::LoadState::Failed(_)));
        (self.0.iter().filter(settled).count(), self.0.len())
    }
}

/// What a loading pass is for: the boot (then the menu, or straight into a game), or a game start.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadTarget {
    Boot { then: Option<Start> },
    Game { start: Start, step: GameStep },
}

/// Where a game start is: its screen shown for some frames first (so "preparing the map" is drawn before the map
/// build starts), then the map built on a worker thread while the loading screen keeps rendering frames
/// (`Building`, `menu::begin_game`/`poll_new_game_build` - unlike `BootJob`, a game start isn't a fixed boot-time
/// plugin, so it gets its own small task instead of reusing that trait), then installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameStep {
    Show { frames: u8 },
    Building,
    Built,
}

/// Frames the loading screen is drawn before a game's map is built.
pub const SHOW_FRAMES: u8 = 2;

impl LoadTarget {
    pub fn game(start: Start) -> Self {
        LoadTarget::Game { start, step: GameStep::Show { frames: SHOW_FRAMES } }
    }
}

/// The step a loading pass is at, for the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Boot,
    Assets { loaded: usize, total: usize },
    Map,
}

/// Sent once a loading pass ends: the menu or the game is ready. Captures count their frames from it.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loaded(pub AppState);

/// The loading systems, after the jobs' polls.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoadingSystems;

/// Starting a game (`menu`): its map is built behind the loading screen.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeginGame(pub Start);

/// What the loading pass does next, from where it is (pure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Wait,
    /// Build the game's map now.
    Begin(Start),
    Finish(AppState),
    /// The boot is over and a game follows: load it.
    Load(Start),
}

/// The next move of a loading pass towards `target`, with the boot jobs `jobs_done`, the tracked assets
/// `assets_done`.
pub fn next(target: LoadTarget, jobs_done: bool, assets_done: bool) -> Next {
    match target {
        LoadTarget::Boot { .. } if !jobs_done || !assets_done => Next::Wait,
        LoadTarget::Boot { then: Some(start) } => Next::Load(start),
        LoadTarget::Boot { then: None } => Next::Finish(AppState::Menu),
        LoadTarget::Game { start, step: GameStep::Show { frames: 0 } } => Next::Begin(start),
        LoadTarget::Game { step: GameStep::Show { .. }, .. } => Next::Wait,
        // Building -> Built is driven by `menu::poll_new_game_build` once its task finishes, not by this pure
        // function (it has no way to poll a `Task`): it just keeps waiting meanwhile, same as `Show`.
        LoadTarget::Game { step: GameStep::Building, .. } => Next::Wait,
        LoadTarget::Game { step: GameStep::Built, .. } if !assets_done => Next::Wait,
        LoadTarget::Game { step: GameStep::Built, .. } => Next::Finish(AppState::Playing),
    }
}

pub struct BootPlugin;

impl Plugin for BootPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BootStatus>()
            .init_resource::<TrackedAssets>()
            .add_message::<Loaded>()
            .add_message::<BeginGame>()
            .add_plugins(screen::LoadingScreenPlugin)
            .add_systems(Update, advance.in_set(LoadingSystems).run_if(in_state(AppState::Loading)));
    }
}

/// Moves the loading pass on: waits, builds the map, or leaves for the menu or the game (`Loaded`, and a `ready`
/// span marking the moment in traces).
fn advance(
    status: Res<BootStatus>,
    tracked: Res<TrackedAssets>,
    server: Res<AssetServer>,
    mut target: ResMut<LoadTarget>,
    mut state: ResMut<NextState<AppState>>,
    mut begin: MessageWriter<BeginGame>,
    mut loaded: MessageWriter<Loaded>,
) {
    let (done, total) = tracked.count(&server);
    match next(*target, status.is_done(), done == total) {
        Next::Wait => {
            if let LoadTarget::Game { step: GameStep::Show { frames }, .. } = &mut *target {
                *frames = frames.saturating_sub(1);
            }
        }
        Next::Begin(start) => {
            begin.write(BeginGame(start));
            *target = LoadTarget::Game { start, step: GameStep::Building };
        }
        Next::Load(start) => *target = LoadTarget::game(start),
        Next::Finish(to) => {
            let _ready = info_span!("ready").entered();
            info!("loaded: {to:?}");
            loaded.write(Loaded(to));
            state.set(to);
        }
    }
}

/// The stage of a loading pass, for the screen.
pub fn stage(target: LoadTarget, status: &BootStatus, assets: (usize, usize)) -> Stage {
    match target {
        LoadTarget::Boot { .. } if !status.is_done() => Stage::Boot,
        _ if assets.0 < assets.1 => Stage::Assets { loaded: assets.0, total: assets.1 },
        LoadTarget::Boot { .. } => Stage::Assets { loaded: assets.0, total: assets.1 },
        LoadTarget::Game { .. } => Stage::Map,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(label: &'static str, done: u32, total: u32, phase: JobPhase) -> JobStatus {
        let progress = Progress::default();
        (0..done).for_each(|_| progress.step());
        JobStatus { label, progress, total, phase, install_progress: Progress::default(), install_total: 1 }
    }

    #[test]
    fn boot_fraction_weighs_jobs_alike_and_ends_only_when_all_are_installed() {
        assert_eq!(BootStatus::default().fraction(), 1.0, "nothing to load");
        let s = BootStatus(vec![job("a", 5, 10, JobPhase::Running), job("b", 0, 3, JobPhase::Done)]);
        assert!((s.fraction() - 0.75).abs() < 1e-6);
        assert!(!s.is_done());
        assert_eq!(s.current().map(|j| j.label), Some("a"));
        let installing = BootStatus(vec![job("a", 10, 10, JobPhase::Installing)]);
        assert!(installing.fraction() < 1.0 && !installing.is_done(), "every step run, not installed yet");
        assert!(BootStatus(vec![job("a", 10, 10, JobPhase::Done)]).is_done());
    }

    #[test]
    fn a_boot_goes_to_the_menu_or_loads_its_game_a_game_is_built_once_shown() {
        let boot = |then| LoadTarget::Boot { then };
        assert_eq!(next(boot(None), false, true), Next::Wait);
        assert_eq!(next(boot(None), true, false), Next::Wait, "models still loading");
        assert_eq!(next(boot(None), true, true), Next::Finish(AppState::Menu));
        assert_eq!(next(boot(Some(Start::SandboxUnits)), true, true), Next::Load(Start::SandboxUnits));
        let game = |step| LoadTarget::Game { start: Start::NewGame, step };
        assert_eq!(next(game(GameStep::Show { frames: 1 }), true, true), Next::Wait, "the screen drawn first");
        assert_eq!(next(game(GameStep::Show { frames: 0 }), true, true), Next::Begin(Start::NewGame), "shown: now build the map");
        assert_eq!(next(game(GameStep::Building), true, true), Next::Wait, "building on a worker thread, not installed yet");
        assert_eq!(next(game(GameStep::Built), true, false), Next::Wait, "models still loading");
        assert_eq!(next(game(GameStep::Built), true, true), Next::Finish(AppState::Playing));
    }

    #[test]
    fn the_screen_says_boot_then_models_then_map() {
        let busy = BootStatus(vec![job("a", 1, 2, JobPhase::Running)]);
        let done = BootStatus(vec![job("a", 2, 2, JobPhase::Done)]);
        assert_eq!(stage(LoadTarget::Boot { then: None }, &busy, (0, 2)), Stage::Boot);
        assert_eq!(stage(LoadTarget::Boot { then: None }, &done, (1, 2)), Stage::Assets { loaded: 1, total: 2 });
        assert_eq!(stage(LoadTarget::game(Start::NewGame), &done, (2, 2)), Stage::Map);
    }

    /// Adds 1 to 3 numbers off-thread, installs their sum.
    struct Sum;

    #[derive(Resource, Debug, PartialEq)]
    struct Total(u32);

    impl BootJob for Sum {
        const LABEL: &'static str = "sum";
        type Input = Vec<u32>;
        type Output = u32;
        fn input(_: &World) -> Vec<u32> {
            vec![1, 2, 3]
        }
        fn steps(input: &Vec<u32>) -> u32 {
            input.len() as u32
        }
        fn run(input: Vec<u32>, progress: Progress) -> u32 {
            input.iter().map(|n| {
                progress.step();
                n + 1
            }).sum()
        }
        fn install_step(output: u32, world: &mut World) -> InstallProgress<u32> {
            world.insert_resource(Total(output));
            InstallProgress::Done
        }
    }

    #[test]
    fn a_job_runs_off_thread_is_installed_and_the_boot_ends_in_the_menu() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin, AssetPlugin::default()))
            .init_state::<AppState>()
            .insert_resource(LoadTarget::Boot { then: None })
            .init_resource::<BootStatus>()
            .init_resource::<TrackedAssets>()
            .add_message::<Loaded>()
            .add_message::<BeginGame>()
            .add_plugins(BootJobPlugin::<Sum>::default())
            .add_systems(Update, advance.in_set(LoadingSystems).run_if(in_state(AppState::Loading)));
        for _ in 0..200 {
            app.update();
            if *app.world().resource::<State<AppState>>().get() == AppState::Menu {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(app.world().get_resource::<Total>(), Some(&Total(9)));
        assert_eq!(*app.world().resource::<State<AppState>>().get(), AppState::Menu);
        let status = app.world().resource::<BootStatus>();
        assert_eq!((status.0.len(), status.0[0].progress.done(), status.is_done()), (1, 3, true));
    }

    /// Installs a `Vec<u32>` one number per frame, appending each to `Installed` - so a test can watch an
    /// install actually spread across several `poll_job` calls instead of landing in one.
    struct Chunks;

    #[derive(Resource, Default, Debug, PartialEq)]
    struct Installed(Vec<u32>);

    impl BootJob for Chunks {
        const LABEL: &'static str = "chunks";
        type Input = Vec<u32>;
        type Output = Vec<u32>;
        fn input(_: &World) -> Vec<u32> {
            vec![10, 20, 30]
        }
        fn steps(_: &Vec<u32>) -> u32 {
            1
        }
        fn run(input: Vec<u32>, progress: Progress) -> Vec<u32> {
            progress.step();
            input
        }
        fn install_steps(output: &Vec<u32>) -> u32 {
            output.len() as u32
        }
        fn install_step(mut output: Vec<u32>, world: &mut World) -> InstallProgress<Vec<u32>> {
            let n = output.remove(0);
            world.resource_mut::<Installed>().0.push(n);
            if output.is_empty() { InstallProgress::Done } else { InstallProgress::More(output) }
        }
    }

    #[test]
    fn a_chunked_install_lands_one_piece_per_frame_and_the_bar_climbs_through_it() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin, AssetPlugin::default()))
            .init_state::<AppState>()
            .insert_resource(LoadTarget::Boot { then: None })
            .init_resource::<BootStatus>()
            .init_resource::<TrackedAssets>()
            .init_resource::<Installed>()
            .add_message::<Loaded>()
            .add_message::<BeginGame>()
            .add_plugins(BootJobPlugin::<Chunks>::default());
        // Let the off-thread `run` finish, then one chunk should land per update from here on.
        for _ in 0..50 {
            app.update();
            if matches!(*app.world().resource::<JobSlot<Chunks>>(), JobSlot::Installing(_)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(matches!(*app.world().resource::<JobSlot<Chunks>>(), JobSlot::Installing(_)), "run finished, installing now");
        assert_eq!(app.world().resource::<Installed>().0, Vec::<u32>::new(), "not chunked in yet");
        app.update();
        assert_eq!(app.world().resource::<Installed>().0, vec![10], "one piece this frame");
        let mid = app.world().resource::<BootStatus>().fraction();
        assert!(mid > 0.9 && mid < 1.0, "installing: past the run cap, not done yet: {mid}");
        app.update();
        assert_eq!(app.world().resource::<Installed>().0, vec![10, 20], "one more");
        app.update();
        assert_eq!(app.world().resource::<Installed>().0, vec![10, 20, 30], "the last piece");
        assert!(app.world().resource::<BootStatus>().is_done());
    }
}
