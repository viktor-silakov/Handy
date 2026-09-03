use enigo::{Enigo, Key, Keyboard, Mouse, Settings};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod macos {
    use super::Key;
    use log::{debug, warn};
    use std::ffi::c_void;

    type TisInputSourceRef = *const c_void;
    type CfDataRef = *const c_void;
    type CfStringRef = *const c_void;

    // kVK_ANSI_V. This is the behavior Handy used before layout-aware
    // resolution and remains the safest fallback if macOS cannot expose the
    // active layout.
    const ANSI_V_KEYCODE: u16 = 9;
    const KEYCODE_COUNT: u16 = 128;
    const UC_KEY_ACTION_DISPLAY: u16 = 3;
    const UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK: u32 = 1;
    // Carbon's cmdKey is bit 8. UCKeyTranslate expects Carbon modifiers shifted
    // right by 8, so Command is represented by bit 0 here.
    const COMMAND_MODIFIER_STATE: u32 = 1;

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> TisInputSourceRef;
        fn TISGetInputSourceProperty(
            input_source: TisInputSourceRef,
            property_key: CfStringRef,
        ) -> CfDataRef;
        static kTISPropertyUnicodeKeyLayoutData: CfStringRef;
        fn UCKeyTranslate(
            key_layout: *const u8,
            virtual_key_code: u16,
            key_action: u16,
            modifier_key_state: u32,
            keyboard_type: u32,
            key_translate_options: u32,
            dead_key_state: *mut u32,
            max_string_length: usize,
            actual_string_length: *mut usize,
            unicode_string: *mut u16,
        ) -> i32;
        fn LMGetKbdType() -> u8;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataGetBytePtr(data: CfDataRef) -> *const u8;
        fn CFRelease(value: *const c_void);
    }

    struct InputSource(TisInputSourceRef);

    impl Drop for InputSource {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: TISCopyCurrentKeyboardLayoutInputSource returned this
                // retained reference, so this balances that ownership.
                unsafe { CFRelease(self.0) };
            }
        }
    }

    fn find_keycode(mut matches: impl FnMut(u16) -> bool) -> Option<u16> {
        (0..KEYCODE_COUNT).find(|&keycode| matches(keycode))
    }

    /// Resolves the physical key that macOS interprets as `v` while Command is
    /// held. Including Command is important: non-Latin layouts commonly map
    /// Cmd shortcuts to their ANSI equivalents, while standard Dvorak does not.
    ///
    /// TIS APIs must run on the main thread. Handy's paste path already enters
    /// through `AppHandle::run_on_main_thread` before reaching this function.
    fn resolve_command_v_keycode() -> Result<u16, String> {
        // SAFETY: This function is called on the macOS main thread. The returned
        // source follows the Create Rule and is released by InputSource::drop.
        let source = InputSource(unsafe { TISCopyCurrentKeyboardLayoutInputSource() });
        if source.0.is_null() {
            return Err("macOS returned no current keyboard layout input source".into());
        }

        // SAFETY: The source remains retained for the duration of the scan and
        // the property constant is provided by Carbon.
        let layout_data =
            unsafe { TISGetInputSourceProperty(source.0, kTISPropertyUnicodeKeyLayoutData) };
        if layout_data.is_null() {
            return Err("current macOS keyboard layout has no Unicode layout data".into());
        }

        // SAFETY: layout_data is a CFData owned by the retained input source and
        // remains valid until source is dropped after the scan.
        let layout = unsafe { CFDataGetBytePtr(layout_data) };
        if layout.is_null() {
            return Err("current macOS keyboard layout data is empty".into());
        }

        // SAFETY: LMGetKbdType has no arguments and returns the current physical
        // keyboard type used by UCKeyTranslate.
        let keyboard_type = unsafe { LMGetKbdType() } as u32;
        let keycode = find_keycode(|keycode| {
            let mut dead_key_state = 0;
            let mut chars = [0_u16; 4];
            let mut length = 0_usize;

            // SAFETY: layout points to valid UCKeyboardLayout bytes while source
            // is retained. All output pointers reference initialized local
            // storage of the declared sizes.
            let status = unsafe {
                UCKeyTranslate(
                    layout,
                    keycode,
                    UC_KEY_ACTION_DISPLAY,
                    COMMAND_MODIFIER_STATE,
                    keyboard_type,
                    UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK,
                    &mut dead_key_state,
                    chars.len(),
                    &mut length,
                    chars.as_mut_ptr(),
                )
            };

            status == 0 && length == 1 && chars[0] == u16::from(b'v')
        })
        .ok_or_else(|| "could not map Cmd+V in the current macOS keyboard layout".to_string())?;

        Ok(keycode)
    }

    pub(super) fn command_v_key() -> Key {
        match resolve_command_v_keycode() {
            Ok(keycode) => {
                debug!("Resolved Cmd+V for the active macOS layout to keycode {keycode}");
                Key::Other(u32::from(keycode))
            }
            Err(error) => {
                warn!(
                    "Could not resolve Cmd+V for the active macOS layout ({error}); using ANSI V keycode {ANSI_V_KEYCODE}"
                );
                Key::Other(u32::from(ANSI_V_KEYCODE))
            }
        }
    }
}

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

/// Get the current mouse cursor position using the managed Enigo instance.
/// Returns None if the state is not available or if getting the location fails.
pub fn get_cursor_position(app_handle: &AppHandle) -> Option<(i32, i32)> {
    let enigo_state = app_handle.try_state::<EnigoState>()?;
    let enigo = enigo_state.0.lock().ok()?;
    enigo.location().ok()
}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
///
/// `hold_ms` is how long the modifier stays held after the V click before being
/// released. Most applications read the modifier from the V event's flags and
/// need no hold at all, but applications that poll global keyboard state when
/// handling the key need the modifier to still be down — the hold insures
/// against those. Callers that can detect a failed chord (e.g. the
/// receipt-sequenced paste path) may use a much shorter hold.
pub fn send_paste_ctrl_v(enigo: &mut Enigo, hold_ms: u64) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, macos::command_v_key());
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press modifier + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(hold_ms));

    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo, hold_ms: u64) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, macos::command_v_key());
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press Ctrl/Cmd + Shift + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(hold_ms));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;
    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_shift_insert(enigo: &mut Enigo, hold_ms: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let insert_key_code = Key::Other(0x2D); // VK_INSERT
    #[cfg(not(target_os = "windows"))]
    let insert_key_code = Key::Other(0x76); // XK_Insert (keycode 118 / 0x76, also used as fallback)

    // Press Shift + Insert
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(insert_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click Insert key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(hold_ms));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;

    Ok(())
}

/// Pastes text directly using the enigo text method.
/// This tries to use system input methods if possible, otherwise simulates keystrokes one by one.
pub fn paste_text_direct(enigo: &mut Enigo, text: &str) -> Result<(), String> {
    enigo
        .text(text)
        .map_err(|e| format!("Failed to send text directly: {}", e))?;

    Ok(())
}

/// Reverse-maps characters to the (keycode, modifiers) that produce them, across
/// all enabled keyboard layouts, and switches the active input source so mixed
/// scripts (e.g. Cyrillic + Latin) can be typed through Screen Sharing — which
/// derives the forwarded character from the keycode under the *current* layout.
#[cfg(target_os = "macos")]
mod remote_keymap {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation::url::CFURL;
    use std::collections::HashMap;
    use std::os::raw::c_void;

    /// Modifier bits stored in the reverse map (which real modifier keys to hold).
    pub const MOD_SHIFT: u8 = 1;
    pub const MOD_OPTION: u8 = 2;

    /// Delay after switching the active input source, so the change propagates
    /// system-wide (Screen Sharing must see the new layout before we send keys).
    const SWITCH_SETTLE_MS: u64 = 90;

    /// A private keyboard layout whose BASE (no-modifier) keys are the punctuation
    /// characters Screen Sharing can't otherwise deliver. Screen Sharing forwards
    /// the *unshifted* char of a keycode and the remote folds Shift onto letters
    /// only, so shifted punctuation ("?", "!") arrives as its base key ("/", "1").
    /// Selecting this layout lets us type each punctuation char as an unshifted key,
    /// which forwards verbatim (the same path that already carries Cyrillic).
    const HANDYPUNCT_ID: &str = "org.unknown.keylayout.HandyPunct";
    const HANDYPUNCT_XML: &str = r####"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE keyboard SYSTEM "file://localhost/System/Library/DTDs/KeyboardLayout.dtd">
<keyboard group="126" id="-28513" name="HandyPunct" maxout="1">
  <layouts>
    <layout first="0" last="127" mapSet="m" modifiers="mods"/>
  </layouts>
  <modifierMap id="mods" defaultIndex="0">
    <keyMapSelect mapIndex="0"><modifier keys=""/></keyMapSelect>
  </modifierMap>
  <keyMapSet id="m">
    <keyMap index="0">
      <key code="0" output="?"/>
      <key code="1" output="!"/>
      <key code="2" output="@"/>
      <key code="3" output="#"/>
      <key code="4" output="$"/>
      <key code="5" output="%"/>
      <key code="6" output="^"/>
      <key code="7" output="&#x0026;"/>
      <key code="8" output="*"/>
      <key code="9" output="("/>
      <key code="11" output=")"/>
      <key code="12" output="_"/>
      <key code="13" output="+"/>
      <key code="14" output="{"/>
      <key code="15" output="}"/>
      <key code="16" output="|"/>
      <key code="17" output=":"/>
      <key code="31" output="&#x0022;"/>
      <key code="32" output="&#x003C;"/>
      <key code="34" output="&#x003E;"/>
      <key code="35" output="~"/>
      <key code="37" output="&#x2116;"/>
    </keyMap>
  </keyMapSet>
</keyboard>
"####;

    type TISInputSourceRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFURLRef = *const c_void;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn TISCopyCurrentKeyboardInputSource() -> TISInputSourceRef;
        fn TISCreateInputSourceList(properties: CFTypeRef, include_all: u8) -> CFArrayRef;
        fn TISSelectInputSource(source: TISInputSourceRef) -> i32;
        fn TISEnableInputSource(source: TISInputSourceRef) -> i32;
        fn TISRegisterInputSource(location: CFURLRef) -> i32;
        fn TISGetInputSourceProperty(source: TISInputSourceRef, key: CFStringRef) -> CFTypeRef;
        static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
        static kTISPropertyInputSourceID: CFStringRef;
        #[allow(clippy::too_many_arguments)]
        fn UCKeyTranslate(
            key_layout_ptr: *const u8,
            virtual_key_code: u16,
            key_action: u16,
            modifier_key_state: u32,
            keyboard_type: u32,
            key_translate_options: u32,
            dead_key_state: *mut u32,
            max_string_length: usize,
            actual_string_length: *mut usize,
            unicode_string: *mut u16,
        ) -> i32;
        fn LMGetKbdType() -> u8;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(arr: CFArrayRef) -> isize;
        fn CFArrayGetValueAtIndex(arr: CFArrayRef, idx: isize) -> CFTypeRef;
        fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
        fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
        fn CFRelease(cf: CFTypeRef);
    }

    /// Reads a CFStringRef (borrowed) into a Rust String.
    unsafe fn cf_string_to_rust(s: CFTypeRef) -> Option<String> {
        if s.is_null() {
            return None;
        }
        let cf = CFString::wrap_under_get_rule(s as core_foundation::string::CFStringRef);
        Some(cf.to_string())
    }

    /// Finds an input source by its input-source id in either the enabled list
    /// (`include_all = 0`) or all installed sources (`include_all = 1`). Returns a
    /// retained ref (caller releases) or null.
    unsafe fn find_source_by_id(id: &str, include_all: u8) -> TISInputSourceRef {
        let list = TISCreateInputSourceList(std::ptr::null(), include_all);
        if list.is_null() {
            return std::ptr::null();
        }
        let count = CFArrayGetCount(list);
        let mut found: TISInputSourceRef = std::ptr::null();
        for i in 0..count {
            let src = CFArrayGetValueAtIndex(list, i);
            if src.is_null() {
                continue;
            }
            let src_id = TISGetInputSourceProperty(src, kTISPropertyInputSourceID);
            if cf_string_to_rust(src_id).as_deref() == Some(id) {
                found = CFRetain(src);
                break;
            }
        }
        CFRelease(list);
        found
    }

    /// Writes, registers, and (only if necessary) enables the HandyPunct layout;
    /// returns its source ref (retained — caller releases) or null.
    ///
    /// Enabling an input source triggers a one-time macOS security prompt ("Allow
    /// Handy to enable HandyPunct"). To keep it truly one-time, we only enable when
    /// it isn't already enabled, and we never disable it afterward — so switching to
    /// it (like the user's other layouts) needs no further prompt.
    unsafe fn ensure_handypunct() -> TISInputSourceRef {
        let Ok(home) = std::env::var("HOME") else {
            return std::ptr::null();
        };
        let dir = std::path::Path::new(&home).join("Library/Keyboard Layouts");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("HandyPunct.keylayout");
        let need_write = std::fs::read_to_string(&path)
            .map(|c| c != HANDYPUNCT_XML)
            .unwrap_or(true);
        if need_write && std::fs::write(&path, HANDYPUNCT_XML).is_err() {
            return std::ptr::null();
        }
        if let Some(url) = CFURL::from_path(&path, false) {
            // Idempotent and silent: re-registering an already-registered source
            // is harmless and does not prompt.
            TISRegisterInputSource(url.as_concrete_TypeRef() as CFURLRef);
        }

        // Already enabled? Use it as-is — no enable call, no prompt.
        let enabled = find_source_by_id(HANDYPUNCT_ID, 0);
        if !enabled.is_null() {
            return enabled;
        }

        // Installed but not enabled: enable once (this is what prompts the user).
        let installed = find_source_by_id(HANDYPUNCT_ID, 1);
        if !installed.is_null() {
            TISEnableInputSource(installed);
        }
        installed
    }

    // UCKeyTranslate constants.
    const K_UC_KEY_ACTION_DISPLAY: u16 = 3;
    const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS: u32 = 1; // (1 << kUCKeyTranslateNoDeadKeysBit=0)
                                                    // modifierKeyState is (Carbon modifiers >> 8): shift=0x0200>>8=2, option=0x0800>>8=8.
    const UCK_SHIFT: u32 = 2;
    const UCK_OPTION: u32 = 8;

    unsafe fn build_map(layout_ptr: *const u8, kbd_type: u32) -> HashMap<char, (u16, u8)> {
        let mut map: HashMap<char, (u16, u8)> = HashMap::new();
        let mod_states: [(u32, u8); 4] = [
            (0, 0),
            (UCK_SHIFT, MOD_SHIFT),
            (UCK_OPTION, MOD_OPTION),
            (UCK_SHIFT | UCK_OPTION, MOD_SHIFT | MOD_OPTION),
        ];
        for keycode in 0u16..128 {
            for &(uck_mods, our_mods) in &mod_states {
                let mut dead_state: u32 = 0;
                let mut buf = [0u16; 8];
                let mut len: usize = 0;
                let status = UCKeyTranslate(
                    layout_ptr,
                    keycode,
                    K_UC_KEY_ACTION_DISPLAY,
                    uck_mods,
                    kbd_type,
                    K_UC_KEY_TRANSLATE_NO_DEAD_KEYS,
                    &mut dead_state,
                    buf.len(),
                    &mut len,
                    buf.as_mut_ptr(),
                );
                if status == 0 && len == 1 {
                    if let Some(c) = char::from_u32(buf[0] as u32) {
                        if !c.is_control() {
                            map.entry(c).or_insert((keycode, our_mods));
                        }
                    }
                }
            }
        }
        map
    }

    /// Holds a reverse map per enabled keyboard layout and switches the active
    /// input source on demand. Restores the original input source on drop.
    pub struct RemoteTyper {
        layouts: Vec<(HashMap<char, (u16, u8)>, TISInputSourceRef)>,
        source_list: CFArrayRef,
        original: TISInputSourceRef,
        handypunct: TISInputSourceRef,
        selected: Option<usize>,
    }

    impl RemoteTyper {
        pub fn new() -> Option<RemoteTyper> {
            unsafe {
                let handypunct = ensure_handypunct();
                let list = TISCreateInputSourceList(std::ptr::null(), 0);
                if list.is_null() {
                    if !handypunct.is_null() {
                        CFRelease(handypunct);
                    }
                    return None;
                }
                let count = CFArrayGetCount(list);
                let kbd_type = LMGetKbdType() as u32;
                let mut layouts = Vec::new();
                let add_layout =
                    |layouts: &mut Vec<(HashMap<char, (u16, u8)>, TISInputSourceRef)>,
                     src: TISInputSourceRef| {
                        let data = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData);
                        if data.is_null() {
                            return; // not a uchr keyboard layout (e.g. an IME)
                        }
                        let bytes = CFDataGetBytePtr(data);
                        if bytes.is_null() {
                            return;
                        }
                        let map = build_map(bytes, kbd_type);
                        if !map.is_empty() {
                            layouts.push((map, src));
                        }
                    };

                // HandyPunct first so its base-key punctuation is preferred.
                if !handypunct.is_null() {
                    add_layout(&mut layouts, handypunct);
                }
                for i in 0..count {
                    let src = CFArrayGetValueAtIndex(list, i);
                    if src.is_null() {
                        continue;
                    }
                    // Skip HandyPunct if it also shows up in the enabled list.
                    let id = TISGetInputSourceProperty(src, kTISPropertyInputSourceID);
                    if cf_string_to_rust(id).as_deref() == Some(HANDYPUNCT_ID) {
                        continue;
                    }
                    add_layout(&mut layouts, src);
                }
                if layouts.is_empty() {
                    if !handypunct.is_null() {
                        CFRelease(handypunct);
                    }
                    CFRelease(list);
                    return None;
                }
                let original = TISCopyCurrentKeyboardInputSource(); // owned (+1)
                Some(RemoteTyper {
                    layouts,
                    source_list: list,
                    original,
                    handypunct,
                    selected: None,
                })
            }
        }

        /// Returns the (keycode, modifiers) for `c`, switching the active input
        /// source if needed. Prefers a layout where `c` is a BASE (unshifted) key,
        /// because Screen Sharing forwards the base char verbatim; Shift-accessed
        /// punctuation is dropped by the remote. Falls back to a shifted mapping
        /// (fine for letters, which the remote case-folds) and minimizes switches.
        pub fn get(&mut self, c: char) -> Option<(u16, u8)> {
            let is_base = |i: usize| matches!(self.layouts[i].0.get(&c), Some(&(_, 0)));
            let has = |i: usize| self.layouts[i].0.contains_key(&c);

            // 1. If currently selected layout already has this character (even shifted,
            // like comma in Russian-PC), stick with it to prevent unnecessary layout
            // switches and cross-process race conditions.
            let mut idx: Option<usize> = None;
            if let Some(s) = self.selected {
                if has(s) {
                    idx = Some(s);
                }
            }
            // 2. If current layout does not have the character, find a layout where it's a base key.
            if idx.is_none() {
                idx = (0..self.layouts.len()).find(|&i| is_base(i));
            }
            // 3. Otherwise find any layout that contains the character.
            if idx.is_none() {
                idx = (0..self.layouts.len()).find(|&i| has(i));
            }
            let idx = idx?;

            if self.selected != Some(idx) {
                unsafe {
                    TISSelectInputSource(self.layouts[idx].1);
                }
                self.selected = Some(idx);
                std::thread::sleep(std::time::Duration::from_millis(SWITCH_SETTLE_MS));
            }
            self.layouts[idx].0.get(&c).copied()
        }
    }

    impl Drop for RemoteTyper {
        fn drop(&mut self) {
            unsafe {
                // Restore the user's original input source if we changed it.
                if self.selected.is_some() && !self.original.is_null() {
                    TISSelectInputSource(self.original);
                }
                // Leave HandyPunct enabled: enabling prompts the user once, so we
                // keep it enabled to avoid re-prompting on every dictation.
                if !self.handypunct.is_null() {
                    CFRelease(self.handypunct);
                }
                if !self.original.is_null() {
                    CFRelease(self.original);
                }
                if !self.source_list.is_null() {
                    CFRelease(self.source_list);
                }
            }
        }
    }
}

/// Types `text` into the focused field as real keystroke events, so it can be
/// delivered into a remote-desktop session (e.g. macOS Screen Sharing) which
/// forwards *keycodes*, not the Unicode payload of synthetic events.
///
/// Each character is reverse-mapped to the (virtual keycode, modifiers) that
/// produces it under the current keyboard layout (via `UCKeyTranslate`). Modifiers
/// are sent as real Shift/Option key-down/up events surrounding the character —
/// modifier *flags* alone are dropped by Screen Sharing (the classic "A"→"a" bug).
/// Characters not present in the current layout fall back to a Unicode-string event
/// (correct locally; may be wrong over Screen Sharing) and are counted/logged.
///
/// `per_char_delay_ms` paces the events so a fast remote channel doesn't drop or
/// reorder them. Newlines are sent as a real Return keypress (keycode 36).
#[cfg(target_os = "macos")]
pub fn type_text_unicode(text: &str, per_char_delay_ms: u64) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    const RETURN_KEYCODE: CGKeyCode = 36;
    const SHIFT_KEYCODE: CGKeyCode = 56;
    const OPTION_KEYCODE: CGKeyCode = 58;
    // Gap between pressing a modifier and the key it modifies, so the remote
    // registers the modifier first (prevents Shift+/ arriving as "/").
    const MOD_SETTLE_MS: u64 = 8;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource".to_string())?;

    let mut typer = remote_keymap::RemoteTyper::new();
    if typer.is_none() {
        log::warn!("Remote typing: could not enumerate keyboard layouts; using Unicode fallback");
    }

    let post = |keycode: CGKeyCode,
                keydown: bool,
                flags: CGEventFlags,
                unicode: Option<&[u16]>|
     -> Result<(), String> {
        let event = CGEvent::new_keyboard_event(source.clone(), keycode, keydown)
            .map_err(|_| "Failed to create keyboard event".to_string())?;
        event.set_flags(flags);
        if let Some(units) = unicode {
            event.set_string_from_utf16_unchecked(units);
        }
        event.post(CGEventTapLocation::HID);
        Ok(())
    };

    let mut unmapped = 0usize;
    let mut buf = [0u16; 2];
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            post(RETURN_KEYCODE, true, CGEventFlags::CGEventFlagNull, None)?;
            post(RETURN_KEYCODE, false, CGEventFlags::CGEventFlagNull, None)?;
        } else if let Some((keycode, mods)) = typer.as_mut().and_then(|t| t.get(ch)) {
            let shift = mods & remote_keymap::MOD_SHIFT != 0;
            let option = mods & remote_keymap::MOD_OPTION != 0;
            let mut flags = CGEventFlags::CGEventFlagNull;
            if shift {
                flags |= CGEventFlags::CGEventFlagShift;
            }
            if option {
                flags |= CGEventFlags::CGEventFlagAlternate;
            }
            // Press modifiers as real key events (flags alone are lost over VNC).
            // Pace the modifier press/release around the key: without a gap, the
            // remote can process the key before the modifier registers, yielding
            // e.g. "/" instead of "?" (Shift+/).
            if shift {
                post(SHIFT_KEYCODE, true, CGEventFlags::CGEventFlagShift, None)?;
            }
            if option {
                post(OPTION_KEYCODE, true, flags, None)?;
            }
            if shift || option {
                std::thread::sleep(std::time::Duration::from_millis(MOD_SETTLE_MS));
            }
            post(keycode, true, flags, None)?;
            post(keycode, false, flags, None)?;
            if shift || option {
                std::thread::sleep(std::time::Duration::from_millis(MOD_SETTLE_MS));
            }
            if option {
                let f = if shift {
                    CGEventFlags::CGEventFlagShift
                } else {
                    CGEventFlags::CGEventFlagNull
                };
                post(OPTION_KEYCODE, false, f, None)?;
            }
            if shift {
                post(SHIFT_KEYCODE, false, CGEventFlags::CGEventFlagNull, None)?;
            }
        } else {
            // Fallback: keycode 0 + Unicode string. Correct locally; over Screen
            // Sharing this typically yields the wrong character, so it's counted.
            let units = ch.encode_utf16(&mut buf);
            post(0, true, CGEventFlags::CGEventFlagNull, Some(units))?;
            post(0, false, CGEventFlags::CGEventFlagNull, Some(units))?;
            unmapped += 1;
        }
        if per_char_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(per_char_delay_ms));
        }
    }

    if unmapped > 0 {
        log::warn!(
            "Remote typing: {} character(s) not in the current keyboard layout; used Unicode fallback (may render wrong over Screen Sharing). Switch the local layout to match the text.",
            unmapped
        );
    }

    Ok(())
}

/// Types `text` into the focused field as direct Unicode CGEvent characters (keycode 0 + string).
/// This is 100% layout-independent, does not switch system input sources, and correctly
/// preserves all punctuation, accents, and mixed alphabets (e.g. Cyrillic and Latin).
#[cfg(target_os = "macos")]
pub fn type_text_direct_unicode(text: &str, per_char_delay_ms: u64) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    const RETURN_KEYCODE: CGKeyCode = 36;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource".to_string())?;

    let post = |keycode: CGKeyCode,
                keydown: bool,
                flags: CGEventFlags,
                unicode: Option<&[u16]>|
     -> Result<(), String> {
        let event = CGEvent::new_keyboard_event(source.clone(), keycode, keydown)
            .map_err(|_| "Failed to create keyboard event".to_string())?;
        event.set_flags(flags);
        if let Some(units) = unicode {
            event.set_string_from_utf16_unchecked(units);
        }
        event.post(CGEventTapLocation::HID);
        Ok(())
    };

    let mut buf = [0u16; 2];
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            post(RETURN_KEYCODE, true, CGEventFlags::CGEventFlagNull, None)?;
            post(RETURN_KEYCODE, false, CGEventFlags::CGEventFlagNull, None)?;
        } else {
            let units = ch.encode_utf16(&mut buf);
            post(0, true, CGEventFlags::CGEventFlagNull, Some(units))?;
            post(0, false, CGEventFlags::CGEventFlagNull, Some(units))?;
        }
        if per_char_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(per_char_delay_ms));
        }
    }

    Ok(())
}

/// Non-macOS platforms don't have the remote-desktop typing path.
#[cfg(not(target_os = "macos"))]
pub fn type_text_unicode(_text: &str, _per_char_delay_ms: u64) -> Result<(), String> {
    Err("Remote-desktop typing is only implemented on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn type_text_direct_unicode(_text: &str, _per_char_delay_ms: u64) -> Result<(), String> {
    Err("Direct Unicode typing is only implemented on macOS".to_string())
}
