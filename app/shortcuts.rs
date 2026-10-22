//! Typed, app-owned Windows shortcut bindings.

pub const SHORTCUT_COUNT: usize = 13;
pub const MOD_CONTROL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ShortcutAction {
    ToggleIsland = 0,
    CommandBar = 1,
    AiChat = 2,
    VoiceToText = 3,
    ScreenRecording = 4,
    ScreenshotArea = 5,
    ScreenshotScreen = 6,
    ScreenOcr = 7,
    ClipboardHistory = 8,
    PastePlainText = 9,
    MicrophoneMute = 10,
    ColorPicker = 11,
    ScreenQrCode = 12,
}

impl ShortcutAction {
    pub const ALL: [Self; SHORTCUT_COUNT] = [
        Self::ToggleIsland,
        Self::CommandBar,
        Self::AiChat,
        Self::VoiceToText,
        Self::ScreenRecording,
        Self::ScreenshotArea,
        Self::ScreenshotScreen,
        Self::ScreenOcr,
        Self::ClipboardHistory,
        Self::PastePlainText,
        Self::MicrophoneMute,
        Self::ColorPicker,
        Self::ScreenQrCode,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::ToggleIsland => "toggle_island",
            Self::CommandBar => "command_bar",
            Self::AiChat => "ai_chat",
            Self::VoiceToText => "voice_to_text",
            Self::ScreenRecording => "screen_recording",
            Self::ScreenshotArea => "screenshot_area",
            Self::ScreenshotScreen => "screenshot_screen",
            Self::ScreenOcr => "screen_ocr",
            Self::ClipboardHistory => "clipboard_history",
            Self::PastePlainText => "paste_plain_text",
            Self::MicrophoneMute => "microphone_mute",
            Self::ColorPicker => "color_picker",
            Self::ScreenQrCode => "screen_qr_code",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "toggle_island" => Some(Self::ToggleIsland),
            "command_bar" => Some(Self::CommandBar),
            "ai_chat" => Some(Self::AiChat),
            "voice_to_text" => Some(Self::VoiceToText),
            "screen_recording" => Some(Self::ScreenRecording),
            "screenshot_area" => Some(Self::ScreenshotArea),
            "screenshot_screen" => Some(Self::ScreenshotScreen),
            "screen_ocr" => Some(Self::ScreenOcr),
            "clipboard_history" => Some(Self::ClipboardHistory),
            "paste_plain_text" => Some(Self::PastePlainText),
            "microphone_mute" => Some(Self::MicrophoneMute),
            "color_picker" => Some(Self::ColorPicker),
            "screen_qr_code" => Some(Self::ScreenQrCode),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutBinding {
    pub action: ShortcutAction,
    pub modifiers: u8,
    pub virtual_key: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutSet([ShortcutBinding; SHORTCUT_COUNT]);

impl ShortcutSet {
    pub const fn bindings(self) -> [ShortcutBinding; SHORTCUT_COUNT] {
        self.0
    }

    pub fn is_valid(self) -> bool {
        self.0.iter().enumerate().all(|(index, binding)| {
            binding_valid(*binding)
                && binding.action.index() == index
                && !self.0[..index].iter().any(|previous| {
                    previous.modifiers == binding.modifiers
                        && previous.virtual_key == binding.virtual_key
                })
        })
    }

    pub fn encode_json(self) -> String {
        serde_json::json!(
            self.0
                .iter()
                .map(|binding| serde_json::json!({
                    "action": binding.action.id(),
                    "modifiers": binding.modifiers,
                    "virtual_key": binding.virtual_key,
                }))
                .collect::<Vec<_>>()
        )
        .to_string()
    }

    pub fn decode_json(value: &str) -> Option<Self> {
        let values = serde_json::from_str::<serde_json::Value>(value)
            .ok()?
            .as_array()?
            .clone();
        if values.len() != SHORTCUT_COUNT {
            return None;
        }
        let mut bindings = [None; SHORTCUT_COUNT];
        for value in values {
            let action = ShortcutAction::from_id(value.get("action")?.as_str()?)?;
            let binding = ShortcutBinding {
                action,
                modifiers: u8::try_from(value.get("modifiers")?.as_u64()?).ok()?,
                virtual_key: u16::try_from(value.get("virtual_key")?.as_u64()?).ok()?,
            };
            if bindings[action.index()].replace(binding).is_some() {
                return None;
            }
        }
        let mut decoded = ShortcutSet::default().bindings();
        for (index, binding) in bindings.into_iter().enumerate() {
            decoded[index] = binding?;
        }
        let set = Self(decoded);
        set.is_valid().then_some(set)
    }
}

impl Default for ShortcutSet {
    fn default() -> Self {
        let chord = |action, modifiers, key| ShortcutBinding {
            action,
            modifiers,
            virtual_key: key,
        };
        Self([
            chord(
                ShortcutAction::ToggleIsland,
                MOD_CONTROL | MOD_ALT,
                b'I' as u16,
            ),
            // Ctrl+Alt+Space avoids replacing Windows' Alt+Space system menu.
            chord(ShortcutAction::CommandBar, MOD_CONTROL | MOD_ALT, 0x20),
            chord(ShortcutAction::AiChat, MOD_CONTROL | MOD_ALT, b'A' as u16),
            chord(
                ShortcutAction::VoiceToText,
                MOD_CONTROL | MOD_ALT,
                b'D' as u16,
            ),
            chord(
                ShortcutAction::ScreenRecording,
                MOD_CONTROL | MOD_ALT,
                b'R' as u16,
            ),
            chord(
                ShortcutAction::ScreenshotArea,
                MOD_CONTROL | MOD_ALT,
                b'S' as u16,
            ),
            chord(
                ShortcutAction::ScreenshotScreen,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT,
                b'S' as u16,
            ),
            chord(
                ShortcutAction::ScreenOcr,
                MOD_CONTROL | MOD_ALT,
                b'O' as u16,
            ),
            chord(
                ShortcutAction::ClipboardHistory,
                MOD_CONTROL | MOD_ALT,
                b'V' as u16,
            ),
            chord(
                ShortcutAction::PastePlainText,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT,
                b'V' as u16,
            ),
            chord(
                ShortcutAction::MicrophoneMute,
                MOD_CONTROL | MOD_ALT,
                b'M' as u16,
            ),
            chord(
                ShortcutAction::ColorPicker,
                MOD_CONTROL | MOD_ALT,
                b'C' as u16,
            ),
            chord(
                ShortcutAction::ScreenQrCode,
                MOD_CONTROL | MOD_ALT,
                b'Q' as u16,
            ),
        ])
    }
}

fn binding_valid(binding: ShortcutBinding) -> bool {
    let modifiers = binding.modifiers;
    let key = binding.virtual_key;
    modifiers & !(MOD_CONTROL | MOD_ALT | MOD_SHIFT) == 0
        && modifiers & (MOD_CONTROL | MOD_ALT) != 0
        && !(modifiers & MOD_ALT != 0 && key == 0x73)
        && !(modifiers == MOD_ALT && key == 0x20)
        && ((b'A' as u16..=b'Z' as u16).contains(&key)
            || (b'0' as u16..=b'9' as u16).contains(&key)
            || (0x70..=0x7b).contains(&key)
            || key == 0x20)
}

#[cfg(test)]
mod tests {
    use super::{MOD_ALT, MOD_CONTROL, ShortcutAction, ShortcutSet, binding_valid};

    #[test]
    fn reference_defaults_are_unique_and_round_trip() {
        let defaults = ShortcutSet::default();
        assert!(defaults.is_valid());
        assert_eq!(
            ShortcutSet::decode_json(&defaults.encode_json()),
            Some(defaults)
        );
        assert_eq!(ShortcutAction::ALL.len(), 13);
    }
    #[test]
    fn reserved_windows_chords_and_corrupt_registry_are_rejected() {
        assert!(!binding_valid(super::ShortcutBinding {
            action: ShortcutAction::CommandBar,
            modifiers: MOD_ALT,
            virtual_key: 0x20,
        }));
        assert!(!binding_valid(super::ShortcutBinding {
            action: ShortcutAction::CommandBar,
            modifiers: MOD_ALT | MOD_CONTROL,
            virtual_key: 0x73,
        }));
        assert_eq!(ShortcutSet::decode_json("[]"), None);
        assert!(ShortcutSet::decode_json(&ShortcutSet::default().encode_json()).is_some());
    }
}
