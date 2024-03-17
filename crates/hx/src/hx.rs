//! Vim support for Zed.

// #[cfg(test)]
// mod test;

// mod command;
// mod editor_events;
// mod insert;
// mod mode_indicator;
// mod motion;
// mod normal;
// mod object;
// mod replace;
// mod state;
// mod utils;
// mod visual;

use anyhow::Result;
use collections::HashMap;
use command_palette_hooks::{CommandPaletteFilter, CommandPaletteInterceptor};
use editor::{
    movement::{self, FindRange},
    Editor, EditorEvent, EditorMode,
};
use gpui::{
    actions, impl_actions, Action, AppContext, EntityId, Global, KeyContext, KeystrokeEvent, Subscription, View, ViewContext, WeakView, WindowContext
};
use language::{CursorShape, Point, Selection, SelectionGoal, TransactionId};
// pub use mode_indicator::ModeIndicator;
// use motion::Motion;
// use normal::normal_replace;
// use replace::multi_replace;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_derive::Serialize;
use settings::{update_settings_file, Settings, SettingsStore};
use ui::Key;
// use state::{EditorState, Mode, Operator, RecordedSelection, WorkspaceState};
use std::{default, ops::Range, sync::Arc};
// use visual::{visual_block_motion, visual_replace};
use workspace::{self, Workspace};

// use crate::state::ReplayableAction;

/// Whether or not to enable Vim mode (work in progress).
///
/// Default: false
pub struct HelixModeSetting(pub bool);

/// An Action to Switch between modes
// #[derive(Clone, Deserialize, PartialEq)]
// pub struct SwitchMode(pub Mode);

/// PushOperator is used to put vim into a "minor" mode,
/// where it's waiting for a specific next set of keystrokes.
/// For example 'd' needs a motion to complete.
// #[derive(Clone, Deserialize, PartialEq)]
// pub struct PushOperator(pub Operator);

/// Number is used to manage vim's count. Pushing a digit
/// multiplis the current value by 10 and adds the digit.
#[derive(Clone, Deserialize, PartialEq)]
struct Number(usize);

pub enum Actions {
    AddCursorBelow,
}

actions!(hx, [AddCursorBelow]);

#[derive(Default, Clone)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
}

mod normal {
    use super::motion::*;
    use super::*;
    use editor::scroll::Autoscroll;
    pub fn register(workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) {
   }
    pub fn normal_motion(
        motion: Motion,
        // operator: Option<Operator>,
        times: Option<usize>,
        cx: &mut WindowContext,
    ) {
        Helix::update(cx, |hx, cx| move_cursor(hx, motion, times, cx));
    }

    pub fn move_cursor(
        hx: &mut Helix,
        motion: Motion,
        times: Option<usize>,
        cx: &mut WindowContext,
    ) {
        hx.update_active_editor(cx, |_, editor, cx| {
            let text_layout_details = editor.text_layout_details(cx);
            editor.change_selections(Some(Autoscroll::fit()), cx, |s| {
                s.move_cursors_with(|map, cursor, goal| {
                    motion
                        .move_point(map, cursor, goal, times, &text_layout_details)
                        .unwrap_or((cursor, goal))
                })
            })
        });
    }
}
mod motion {
    use editor::display_map::DisplaySnapshot;
    use editor::movement::TextLayoutDetails;
    use editor::DisplayPoint;

    use super::normal::*;
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Motion {
        Down,
    }
    actions!(hx, [Down]);
    impl Motion {
        pub fn move_point(
            &self,
            map: &DisplaySnapshot,
            point: DisplayPoint,
            goal: SelectionGoal,
            maybe_times: Option<usize>,
            text_layout_details: &TextLayoutDetails,
        ) -> Option<(DisplayPoint, SelectionGoal)> {
            let times = maybe_times.unwrap_or(1);
            use Motion::*;

            let (new_point, goal) = match self {
                Down => down_display(map, point, goal, times, &text_layout_details),
            };

            (new_point != point).then_some((new_point, goal))
        }
    }

    fn down_display(
        map: &DisplaySnapshot,
        mut point: DisplayPoint,
        mut goal: SelectionGoal,
        times: usize,
        text_layout_details: &TextLayoutDetails,
    ) -> (DisplayPoint, SelectionGoal) {
        for _ in 0..times {
            //
            (point, goal) = movement::down(map, point, goal, true, text_layout_details)
        }
        (point, goal)
    }

    pub fn register(workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) {
        workspace
            .register_action(|_: &mut Workspace, action: &Down, cx: _| motion(Motion::Down, cx));
    }

    pub fn motion(motion: Motion, cx: &mut WindowContext) {
        // let operator = todo!();
        let count = None;
        println!("motion");

        match Helix::read(cx).state().mode {
            Mode::Normal => normal_motion(motion, count, cx),
            Mode::Insert => {
                // do nothing if in insert mode
            }
        }
    }
}

#[derive(Default, Clone)]
pub struct EditorState {
    pub mode: Mode,
}

impl EditorState {
    pub fn cursor_shape(&self) -> CursorShape {
        match self.mode {
            Mode::Normal => CursorShape::Block,
            Mode::Insert => CursorShape::Bar,
        }
    }

    pub fn helix_controlled(&self) -> bool {
        let is_insert_mode = matches!(self.mode, Mode::Insert);
        if !is_insert_mode {
            return true;
        }
        matches!(
            self.mode,
            Mode::Normal
        )
    }

    pub fn keymap_context_layer(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.set(
            "helix_mode", match self.mode {
                Mode::Normal => "normal",
                Mode::Insert => "insert",
            },
        );

        if self.helix_controlled() {
            context.add("HelixControl");
        }

        context
    }

}
// actions!(
//     vim,
//     [Tab, Enter, Object, InnerObject, FindForward, FindBackward]
// );

// in the workspace namespace so it's not filtered out when vim is disabled.
// actions!(workspace, [ToggleVimMode]);

// impl_actions!(vim, [SwitchMode, PushOperator, Number]);

/// Initializes the `vim` crate.
pub fn init(cx: &mut AppContext) {
    println!("init hx");
    cx.set_global(Helix::default());
    HelixModeSetting::register(cx);

    cx.observe_keystrokes(observe_keystrokes).detach();

    // editor_events::init(cx);

    cx.observe_new_views(|workspace: &mut Workspace, cx| register(workspace, cx))
        .detach();

    cx.observe_new_views(|_, cx: &mut ViewContext<Editor>| {
       let editor = cx.view().clone();
        cx.subscribe(&editor, |_, editor, event: &EditorEvent, cx| match event {
            EditorEvent::Focused => {
                Helix::update(cx, |hx, cx| {
                    if !hx.enabled {
                        return;
                    }
                    hx.activate_editor(editor, cx);
                });
            }
            _ => {}
        }).detach()
    }).detach();

    // // Any time settings change, update vim mode to match. The Vim struct
    // // will be initialized as disabled by default, so we filter its commands
    // // out when starting up.
    // cx.update_global::<CommandPaletteFilter, _>(|filter, _| {
    //     filter.hidden_namespaces.insert("vim");
    // });
    // cx.update_global(|vim: &mut Vim, cx: &mut AppContext| {
    //     vim.set_enabled(VimModeSetting::get_global(cx).0, cx)
    // });
    // cx.observe_global::<SettingsStore>(|cx| {
    //     cx.update_global(|vim: &mut Vim, cx: &mut AppContext| {
    //         vim.set_enabled(VimModeSetting::get_global(cx).0, cx)
    //     });
    // })
    // .detach();
}

fn register(workspace: &mut Workspace, cx: &mut ViewContext<Workspace>) {
    workspace.register_action(|workspace: &mut Workspace, _: &AddCursorBelow, cx: _| {
        println!("a");
        //
    });
    // workspace.register_action(|_: &mut Workspace, &SwitchMode(mode): &SwitchMode, cx| {
    //     Vim::update(cx, |vim, cx| vim.switch_mode(mode, false, cx))
    // });
    // workspace.register_action(
    //     |_: &mut Workspace, PushOperator(operator): &PushOperator, cx| {
    //         Vim::update(cx, |vim, cx| vim.push_operator(operator.clone(), cx))
    //     },
    // );
    // workspace.register_action(|_: &mut Workspace, n: &Number, cx: _| {
    //     Vim::update(cx, |vim, cx| vim.push_count_digit(n.0, cx));
    // });

    // workspace.register_action(|_: &mut Workspace, _: &Tab, cx| {
    //     Vim::active_editor_input_ignored(" ".into(), cx)
    // });

    // workspace.register_action(|_: &mut Workspace, _: &Enter, cx| {
    //     Vim::active_editor_input_ignored("\n".into(), cx)
    // });

    // workspace.register_action(|workspace: &mut Workspace, _: &ToggleVimMode, cx| {
    //     let fs = workspace.app_state().fs.clone();
    //     let currently_enabled = VimModeSetting::get_global(cx).0;
    //     update_settings_file::<VimModeSetting>(fs, cx, move |setting| {
    //         *setting = Some(!currently_enabled)
    //     })
    // });

    normal::register(workspace, cx);
    // insert::register(workspace, cx);
    motion::register(workspace, cx);
    // command::register(workspace, cx);
    // replace::register(workspace, cx);
    // object::register(workspace, cx);
    // visual::register(workspace, cx);
}

/// Called whenever an keystroke is typed so vim can observe all actions
/// and keystrokes accordingly.
fn observe_keystrokes(keystroke_event: &KeystrokeEvent, cx: &mut WindowContext) {
    dbg!(keystroke_event);
    // if let Some(action) = keystroke_event
    //     .action
    //     .as_ref()
    //     .map(|action| action.boxed_clone())
    // {
    //     Vim::update(cx, |vim, _| {
    //         if vim.workspace_state.recording {
    //             vim.workspace_state
    //                 .recorded_actions
    //                 .push(ReplayableAction::Action(action.boxed_clone()));

    //             if vim.workspace_state.stop_recording_after_next_action {
    //                 vim.workspace_state.recording = false;
    //                 vim.workspace_state.stop_recording_after_next_action = false;
    //             }
    //         }
    //     });

    //     // Keystroke is handled by the vim system, so continue forward
    //     if action.name().starts_with("vim::") {
    //         return;
    //     }
    // } else if cx.has_pending_keystrokes() {
    //     return;
    // }

    // Vim::update(cx, |vim, cx| match vim.active_operator() {
    //     Some(Operator::FindForward { .. } | Operator::FindBackward { .. } | Operator::Replace) => {}
    //     Some(_) => {
    //         vim.clear_operator(cx);
    //     }
    //     _ => {}
    // });
}

/// The state pertaining to Vim mode.
#[derive(Default)]
struct Helix {
    active_editor: Option<WeakView<Editor>>,
    // editor_subscription: Option<Subscription>,
    enabled: bool,
    editor_states: HashMap<EntityId, EditorState>,
    // workspace_state: WorkspaceState,
    default_state: EditorState,
}

impl Global for Helix {}

impl Helix {
    fn read(cx: &mut AppContext) -> &Self {
        dbg!("hi! 1");
        cx.global::<Self>()
    }

    fn update<F, S>(cx: &mut WindowContext, update: F) -> S
    where
        F: FnOnce(&mut Self, &mut WindowContext) -> S,
    {
        dbg!("hi! 2");
        cx.update_global(update)
    }

    fn activate_editor(&mut self, editor: View<Editor>, cx: &mut WindowContext) {
        self.sync_helix_settings(cx);
    }

    pub fn state(&self) -> &EditorState {
        if let Some(active_editor) = self.active_editor.as_ref() {
            if let Some(state) = self.editor_states.get(&active_editor.entity_id()) {
                return state;
            }
        }

        &self.default_state
    }

    fn update_active_editor<S>(
        &mut self,
        cx: &mut WindowContext,
        update: impl FnOnce(&mut Helix, &mut Editor, &mut ViewContext<Editor>) -> S,
    ) -> Option<S> {
        let editor = self.active_editor.clone()?.upgrade()?;
        Some(editor.update(cx, |editor, cx| update(self, editor, cx)))
    }

    fn sync_helix_settings(&mut self, cx: &mut WindowContext) {
        self.update_active_editor(cx, |hx, editor, cx| {
            let state = hx.state();

            editor.set_cursor_shape(state.cursor_shape(), cx);
            if editor.is_focused(cx) {
                editor.set_keymap_context_layer::<Self>(state.keymap_context_layer(), cx);
            } else {
                editor.remove_keymap_context_layer::<Self>(cx);
            }
        });
    }
}

impl Settings for HelixModeSetting {
    const KEY: Option<&'static str> = Some("helix_mode");

    type FileContent = Option<bool>;

    fn load(
        default_value: &Self::FileContent,
        user_values: &[&Self::FileContent],
        _: &mut AppContext,
    ) -> Result<Self> {
        Ok(Self(user_values.iter().rev().find_map(|v| **v).unwrap_or(
            default_value.ok_or_else(Self::missing_default)?,
        )))
    }
}
