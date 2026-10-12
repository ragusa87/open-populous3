//! The loading screen, shown while `AppState::Loading`: a title, a bar and the current step (`screen_text`).

use super::{stage, BootStatus, LoadTarget, Stage, TrackedAssets};
use crate::camera::OverlayCamera;
use crate::menu::AppState;
use bevy::prelude::*;

const TEXT: Color = Color::srgb(0.95, 0.85, 0.6);
const BAR_BACK: Color = Color::srgba(0.30, 0.18, 0.06, 0.9);
const BAR_FILL: Color = Color::srgb(0.80, 0.56, 0.20);
const BAR_WIDTH: f32 = 420.0;

#[derive(Component)]
struct LoadingRoot;
#[derive(Component)]
struct LoadingFill;
#[derive(Component)]
struct LoadingLabel;

/// The bar's fill (0 to 1) and the step's line for a loading pass at `stage`.
pub fn screen_text(stage: Stage, status: &BootStatus) -> (f32, String) {
    match stage {
        Stage::Boot => {
            let line = status.current().map_or_else(String::new, |j| format!("{} {}/{}", j.label, j.progress.done().min(j.total), j.total));
            (status.fraction(), line)
        }
        Stage::Assets { loaded, total } => (1.0, format!("Models {loaded}/{total}")),
        Stage::Map => (1.0, "Preparing the map".to_string()),
    }
}

pub struct LoadingScreenPlugin;

impl Plugin for LoadingScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_screen).add_systems(Update, update_screen);
    }
}

fn spawn_screen(mut commands: Commands, overlay: Single<Entity, With<OverlayCamera>>) {
    commands
        .spawn((
            LoadingRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(14),
                ..default()
            },
            GlobalZIndex(i32::MAX - 5),
            UiTargetCamera(*overlay),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((Text::new("Loading"), TextFont { font_size: FontSize::Px(40.0), ..default() }, TextColor(TEXT)));
            root.spawn((Node { width: px(BAR_WIDTH), height: px(14), ..default() }, BackgroundColor(BAR_BACK)))
                .with_child((LoadingFill, Node { width: px(0), height: percent(100), ..default() }, BackgroundColor(BAR_FILL)));
            root.spawn((LoadingLabel, Text::new(""), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(TEXT.with_alpha(0.8))));
        });
}

fn update_screen(
    state: Res<State<AppState>>,
    target: Res<LoadTarget>,
    status: Res<BootStatus>,
    tracked: Res<TrackedAssets>,
    server: Res<AssetServer>,
    mut root: Query<&mut Visibility, With<LoadingRoot>>,
    mut fill: Query<&mut Node, With<LoadingFill>>,
    mut label: Query<&mut Text, With<LoadingLabel>>,
) {
    let loading = *state.get() == AppState::Loading;
    for mut v in &mut root {
        v.set_if_neq(if loading { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !loading {
        return;
    }
    let (fraction, line) = screen_text(stage(*target, &status, tracked.count(&server)), &status);
    for mut n in &mut fill {
        n.width = px(BAR_WIDTH * fraction.clamp(0.0, 1.0));
    }
    for mut t in &mut label {
        if t.0 != line {
            t.0.clone_from(&line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::{JobPhase, JobStatus, Progress};

    #[test]
    fn the_line_names_the_running_job_with_its_steps() {
        let progress = Progress::default();
        (0..12).for_each(|_| progress.step());
        let status = BootStatus(vec![JobStatus { label: "Unit sprites", progress, total: 28, phase: JobPhase::Running }]);
        let (fraction, line) = screen_text(Stage::Boot, &status);
        assert_eq!(line, "Unit sprites 12/28");
        assert!((fraction - 12.0 / 28.0).abs() < 1e-6);
        assert_eq!(screen_text(Stage::Assets { loaded: 1, total: 2 }, &status).1, "Models 1/2");
        assert_eq!(screen_text(Stage::Map, &status), (1.0, "Preparing the map".to_string()));
    }
}
