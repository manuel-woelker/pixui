//! Native event conversion only. Shortcut and activation policy lives in engine.

use pixui_engine::ui::input::{
    ButtonState, Key, KeyLocation, KeyboardEvent, Modifiers, MouseButton, PhysicalKey,
};

pub(crate) fn button_state(state: winit::event::ElementState) -> ButtonState {
    match state {
        winit::event::ElementState::Pressed => ButtonState::Pressed,
        winit::event::ElementState::Released => ButtonState::Released,
    }
}

pub(crate) fn mouse_button(button: winit::event::MouseButton) -> MouseButton {
    match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(value) => MouseButton::Other(value),
    }
}

pub(crate) fn modifiers(state: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        control: state.control_key(),
        alt: state.alt_key(),
        super_key: state.super_key(),
    }
}

pub(crate) fn keyboard(
    event: winit::event::KeyEvent,
    modifiers: Modifiers,
    synthetic: bool,
) -> KeyboardEvent {
    KeyboardEvent {
        key: logical_key(event.logical_key),
        physical_key: physical_key(event.physical_key),
        location: match event.location {
            winit::keyboard::KeyLocation::Standard => KeyLocation::Standard,
            winit::keyboard::KeyLocation::Left => KeyLocation::Left,
            winit::keyboard::KeyLocation::Right => KeyLocation::Right,
            winit::keyboard::KeyLocation::Numpad => KeyLocation::Numpad,
        },
        state: button_state(event.state),
        repeat: event.repeat,
        text: event.text.map(|text| text.to_string()),
        modifiers,
        synthetic,
    }
}

fn logical_key(key: winit::keyboard::Key) -> Key {
    match key {
        winit::keyboard::Key::Character(text) => Key::Character(text.to_string()),
        winit::keyboard::Key::Named(key) => Key::Named(format!("{key:?}")),
        winit::keyboard::Key::Dead(character) => Key::Dead(character),
        winit::keyboard::Key::Unidentified(key) => Key::Unidentified(format!("{key:?}")),
    }
}

fn physical_key(key: winit::keyboard::PhysicalKey) -> PhysicalKey {
    match key {
        winit::keyboard::PhysicalKey::Code(code) => PhysicalKey::Code(format!("{code:?}")),
        winit::keyboard::PhysicalKey::Unidentified(code) => {
            PhysicalKey::Unidentified(format!("{code:?}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{KeyCode, ModifiersState, NamedKey, NativeKey, NativeKeyCode};

    #[test]
    fn conversion_preserves_arbitrary_key_meanings_and_physical_identity() {
        for key in [
            NamedKey::F11,
            NamedKey::Enter,
            NamedKey::Tab,
            NamedKey::AudioVolumeUp,
        ] {
            assert_eq!(
                logical_key(winit::keyboard::Key::Named(key)),
                Key::Named(format!("{key:?}"))
            );
        }
        assert_eq!(
            logical_key(winit::keyboard::Key::Character("日本語".into())),
            Key::Character("日本語".into())
        );
        assert_eq!(
            logical_key(winit::keyboard::Key::Dead(Some('^'))),
            Key::Dead(Some('^'))
        );
        assert_eq!(
            logical_key(winit::keyboard::Key::Unidentified(NativeKey::Unidentified)),
            Key::Unidentified("Unidentified".into())
        );
        assert_eq!(
            physical_key(winit::keyboard::PhysicalKey::Code(KeyCode::KeyQ)),
            PhysicalKey::Code("KeyQ".into())
        );
        assert_ne!(
            physical_key(winit::keyboard::PhysicalKey::Unidentified(
                NativeKeyCode::Xkb(42)
            )),
            physical_key(winit::keyboard::PhysicalKey::Unidentified(
                NativeKeyCode::Xkb(43)
            ))
        );
    }

    #[test]
    fn conversion_preserves_all_mouse_buttons_transitions_and_modifiers() {
        use winit::event::{ElementState, MouseButton as NativeButton};
        for (native, expected) in [
            (NativeButton::Left, MouseButton::Left),
            (NativeButton::Right, MouseButton::Right),
            (NativeButton::Middle, MouseButton::Middle),
            (NativeButton::Back, MouseButton::Back),
            (NativeButton::Forward, MouseButton::Forward),
            (NativeButton::Other(17), MouseButton::Other(17)),
        ] {
            assert_eq!(mouse_button(native), expected);
        }
        assert_eq!(button_state(ElementState::Pressed), ButtonState::Pressed);
        assert_eq!(button_state(ElementState::Released), ButtonState::Released);
        assert_eq!(
            modifiers(ModifiersState::all()),
            Modifiers {
                shift: true,
                control: true,
                alt: true,
                super_key: true
            }
        );
    }
}
