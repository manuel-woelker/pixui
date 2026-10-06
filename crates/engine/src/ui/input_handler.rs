//! Default application input policy. Native hosts forward events without choosing
//! shortcuts, navigation behavior, wheel speed, or activation targets.

use super::{
    geometry::Point,
    input::{ButtonState, Key, MouseButton, UiInput, WheelDelta},
};
use pixui_base::{PixuiResult, pixui_error};

pub(super) enum InputIntent {
    Hover(Point),
    ClearHover,
    Activate(Point),
    ActivateFocused,
    FocusNext { backwards: bool },
    Scroll(f32),
    ToggleDiagnostics,
    Ignore,
}

/// Left-button release activates; key releases and repeats never activate or
/// toggle diagnostics. Line scrolling currently uses forty logical pixels/line.
/// Text, IME, other buttons and other keys have no default behavior yet.
pub(super) fn interpret(input: UiInput) -> PixuiResult<InputIntent> {
    let intent = match input {
        UiInput::PointerMoved(point) => {
            validate_point(point)?;
            InputIntent::Hover(point)
        }
        UiInput::PointerLeft => InputIntent::ClearHover,
        UiInput::MouseButton {
            button,
            state,
            position,
            ..
        } => {
            validate_point(position)?;
            if button == MouseButton::Left && state == ButtonState::Released {
                InputIntent::Activate(position)
            } else {
                InputIntent::Ignore
            }
        }
        UiInput::MouseWheel { delta, .. } => {
            let (x, y, factor) = match delta {
                WheelDelta::Lines { x, y } => (x, y, 40.0),
                WheelDelta::Pixels { x, y } => (x, y, 1.0),
            };
            if !x.is_finite() || !y.is_finite() || !(y * factor).is_finite() {
                return Err(pixui_error!("invalid wheel delta"));
            }
            InputIntent::Scroll(-y * factor)
        }
        UiInput::Keyboard(event)
            if event.state == ButtonState::Pressed && !event.repeat && !event.synthetic =>
        {
            match event.key {
                Key::Named(name) => match name.as_str() {
                    "F11" => InputIntent::ToggleDiagnostics,
                    "Tab" => InputIntent::FocusNext {
                        backwards: event.modifiers.shift,
                    },
                    "Enter" | "Space" => InputIntent::ActivateFocused,
                    _ => InputIntent::Ignore,
                },
                _ => InputIntent::Ignore,
            }
        }
        _ => InputIntent::Ignore,
    };
    Ok(intent)
}

fn validate_point(point: Point) -> PixuiResult<()> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(pixui_error!("invalid pointer coordinates"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::input::{KeyboardEvent, Modifiers};

    #[test]
    fn keyboard_policy_ignores_releases_repeats_synthetic_and_unhandled_keys() {
        for name in ["Enter", "Space", "Tab", "F11", "Escape"] {
            for (state, repeat, synthetic) in [
                (ButtonState::Released, false, false),
                (ButtonState::Pressed, true, false),
                (ButtonState::Pressed, false, true),
            ] {
                let event = KeyboardEvent {
                    state,
                    repeat,
                    synthetic,
                    ..KeyboardEvent::named(name)
                };
                assert!(matches!(
                    interpret(UiInput::Keyboard(event)).unwrap(),
                    InputIntent::Ignore
                ));
            }
        }
        assert!(matches!(
            interpret(UiInput::Keyboard(KeyboardEvent::named("Escape"))).unwrap(),
            InputIntent::Ignore
        ));
        for name in ["Enter", "Space"] {
            assert!(matches!(
                interpret(UiInput::Keyboard(KeyboardEvent::named(name))).unwrap(),
                InputIntent::ActivateFocused
            ));
        }
        let event = KeyboardEvent {
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
            ..KeyboardEvent::named("Tab")
        };
        assert!(matches!(
            interpret(UiInput::Keyboard(event)).unwrap(),
            InputIntent::FocusNext { backwards: true }
        ));
    }

    #[test]
    fn only_left_release_activates_and_wheel_units_are_interpreted_on_worker() {
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Back,
            MouseButton::Forward,
            MouseButton::Other(42),
        ] {
            for state in [ButtonState::Pressed, ButtonState::Released] {
                let intent = interpret(UiInput::MouseButton {
                    button,
                    state,
                    position: Point::default(),
                    modifiers: Modifiers::default(),
                })
                .unwrap();
                assert_eq!(
                    matches!(intent, InputIntent::Activate(_)),
                    button == MouseButton::Left && state == ButtonState::Released
                );
            }
        }
        for delta in [
            WheelDelta::Lines { x: 2.0, y: -2.0 },
            WheelDelta::Pixels { x: 20.0, y: -80.0 },
        ] {
            assert!(matches!(
                interpret(UiInput::MouseWheel {
                    delta,
                    modifiers: Modifiers::default()
                })
                .unwrap(),
                InputIntent::Scroll(80.0)
            ));
        }
        assert!(
            interpret(UiInput::PointerMoved(Point {
                x: f32::NAN,
                y: 0.0
            }))
            .is_err()
        );
        for delta in [
            WheelDelta::Lines {
                x: 0.0,
                y: f32::MAX,
            },
            WheelDelta::Pixels {
                x: f32::NAN,
                y: 0.0,
            },
        ] {
            assert!(
                interpret(UiInput::MouseWheel {
                    delta,
                    modifiers: Modifiers::default()
                })
                .is_err()
            );
        }
    }
}
