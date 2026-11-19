//! Input events from a browser tab, as the desktop sends them, mapped to CDP
//! `Input.*` calls. Coordinates are CSS pixels of the page viewport.

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseKind {
    Pressed,
    Released,
    Moved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    Down,
    Up,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum InputEvent {
    #[serde(rename_all = "camelCase")]
    Mouse { kind: MouseKind, x: f64, y: f64, button: String, buttons: u32, modifiers: u32, click_count: u32 },
    #[serde(rename_all = "camelCase")]
    Wheel { x: f64, y: f64, dx: f64, dy: f64, modifiers: u32 },
    #[serde(rename_all = "camelCase")]
    Key { kind: KeyKind, key: String, code: String, text: Option<String>, modifiers: u32, key_code: u32 },
    Text { text: String },
}

const MAX_TEXT: usize = 64 * 1024;

pub fn to_cdp(event: &InputEvent) -> Option<(&'static str, Value)> {
    Some(match event {
        InputEvent::Mouse { kind, x, y, button, buttons, modifiers, click_count } => {
            let button = match button.as_str() {
                "left" | "middle" | "right" | "back" | "forward" | "none" => button.as_str(),
                _ => return None,
            };
            let kind = match kind {
                MouseKind::Pressed => "mousePressed",
                MouseKind::Released => "mouseReleased",
                MouseKind::Moved => "mouseMoved",
            };
            ("Input.dispatchMouseEvent", json!({
                "type": kind, "x": finite(*x)?, "y": finite(*y)?, "button": button,
                "buttons": buttons, "modifiers": modifiers & 0xF, "clickCount": (*click_count).min(3),
            }))
        }
        InputEvent::Wheel { x, y, dx, dy, modifiers } => ("Input.dispatchMouseEvent", json!({
            "type": "mouseWheel", "x": finite(*x)?, "y": finite(*y)?,
            "deltaX": finite(*dx)?, "deltaY": finite(*dy)?, "modifiers": modifiers & 0xF,
        })),
        InputEvent::Key { kind, key, code, text, modifiers, key_code } => {
            let mut params = json!({
                "type": match (kind, text) { (KeyKind::Down, Some(_)) => "keyDown", (KeyKind::Down, None) => "rawKeyDown", (KeyKind::Up, _) => "keyUp" },
                "key": key, "code": code, "modifiers": modifiers & 0xF, "windowsVirtualKeyCode": key_code,
            });
            if let (KeyKind::Down, Some(text)) = (kind, text) {
                params["text"] = Value::String(text.clone());
            }
            ("Input.dispatchKeyEvent", params)
        }
        InputEvent::Text { text } if text.len() <= MAX_TEXT => ("Input.insertText", json!({ "text": text })),
        InputEvent::Text { .. } => return None,
    })
}

fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_maps_to_dispatch_key_event() {
        let event: InputEvent = serde_json::from_value(json!({
            "type": "key", "kind": "down", "key": "a", "code": "KeyA", "text": "a", "modifiers": 0, "keyCode": 65
        }))
        .unwrap();
        let (method, params) = to_cdp(&event).unwrap();
        assert_eq!(method, "Input.dispatchKeyEvent");
        assert_eq!(params, json!({"type": "keyDown", "key": "a", "code": "KeyA", "text": "a", "modifiers": 0, "windowsVirtualKeyCode": 65}));
    }

    #[test]
    fn a_key_without_text_is_raw_and_up_never_carries_text() {
        let down = InputEvent::Key { kind: KeyKind::Down, key: "ArrowLeft".into(), code: "ArrowLeft".into(), text: None, modifiers: 0, key_code: 37 };
        assert_eq!(to_cdp(&down).unwrap().1["type"], "rawKeyDown");
        let up = InputEvent::Key { kind: KeyKind::Up, key: "a".into(), code: "KeyA".into(), text: Some("a".into()), modifiers: 0, key_code: 65 };
        let (_, params) = to_cdp(&up).unwrap();
        assert_eq!(params["type"], "keyUp");
        assert!(params.get("text").is_none());
    }

    #[test]
    fn mouse_keeps_page_coordinates_and_rejects_bad_values() {
        let click = InputEvent::Mouse { kind: MouseKind::Pressed, x: 150.0, y: 20.5, button: "left".into(), buttons: 1, modifiers: 2, click_count: 1 };
        assert_eq!(to_cdp(&click).unwrap().1, json!({"type": "mousePressed", "x": 150.0, "y": 20.5, "button": "left", "buttons": 1, "modifiers": 2, "clickCount": 1}));
        let nan = InputEvent::Mouse { kind: MouseKind::Moved, x: f64::NAN, y: 0.0, button: "none".into(), buttons: 0, modifiers: 0, click_count: 0 };
        assert!(to_cdp(&nan).is_none());
        let odd = InputEvent::Mouse { kind: MouseKind::Moved, x: 0.0, y: 0.0, button: "x".into(), buttons: 0, modifiers: 0, click_count: 0 };
        assert!(to_cdp(&odd).is_none());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_value::<InputEvent>(json!({"type": "text", "text": "a", "eval": "x"})).is_err());
    }

    #[test]
    fn oversized_text_is_dropped() {
        assert!(to_cdp(&InputEvent::Text { text: "a".repeat(MAX_TEXT + 1) }).is_none());
    }
}
