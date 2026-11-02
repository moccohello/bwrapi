//! The game's chat for an in-game server. While capture is on, a line the
//! local player types is taken out of the chat box instead of being sent
//! (the hook table's chat dialog handler closes the box as Escape does),
//! and the lines the game records for the players it receives are read from
//! the replay recorder at each frame boundary. Both wait in one queue until
//! the server takes them.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Mutex;

/// [`ChatLine::kind`] values.
pub mod chat_kind {
    /// The local player typed the line; it was not sent.
    pub const SENT: u32 = 1;
    /// The game received the line from the player of
    /// [`ChatLine::player`].
    pub const RECEIVED: u32 = 2;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatLine {
    pub kind: u32,
    /// The game player number the line was received from; meaningless for a
    /// sent line.
    pub player: u32,
    pub text: Vec<u8>,
}

/// The chat command: the id, the sending player's byte, then 80 bytes of
/// NUL-terminated text.
const CHAT: u8 = 0x5c;

static CAPTURE: AtomicBool = AtomicBool::new(false);
static QUEUE: Mutex<VecDeque<ChatLine>> = Mutex::new(VecDeque::new());

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn capturing() -> bool {
    CAPTURE.load(Relaxed)
}

/// Turns capture on or off; turning it off drops the lines not taken yet,
/// which belong to the server that stopped.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn set_capture(enable: bool) {
    CAPTURE.store(enable, Relaxed);
    if !enable {
        queue().clear();
    }
}

fn queue() -> std::sync::MutexGuard<'static, VecDeque<ChatLine>> {
    QUEUE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn push(line: ChatLine) {
    queue().push_back(line);
}

/// The oldest line not taken yet.
pub fn next() -> Option<ChatLine> {
    queue().pop_front()
}

/// The received line of a chat command (the id, the sending player's byte,
/// and the NUL-terminated text), or `None` for another command, the local
/// player's own line, or an empty one. BWAPI reports only other players'
/// lines as received.
pub fn chat_line(command: &[u8], local: Option<u32>) -> Option<ChatLine> {
    if command.first() != Some(&CHAT) {
        return None;
    }
    let player = u32::from(*command.get(1)?);
    if Some(player) == local {
        return None;
    }
    let text = command.get(2..)?;
    let text = &text[..text.iter().position(|byte| *byte == 0).unwrap_or(text.len())];
    (!text.is_empty()).then(|| ChatLine { kind: chat_kind::RECEIVED, player, text: text.to_vec() })
}

#[cfg(windows)]
pub(crate) use game::{typed_line, ChatWatch};

#[cfg(windows)]
mod game {
    use bwrapi_engine::decode::scalars::{read_current_player_id_cluster, CURRENT_PLAYER_ID_CLUSTER_BYTES};
    use bwrapi_engine::recorder::{view, walk_view, written_end, RecorderCursor};

    use bwrapi_layout::{ChatLayout, Layout};
    use bwrapi_memory::{InProcessMemory, ProcessMemory};

    use super::{chat_line, capturing, push};

    /// Controls a dialog lists; a longer chain is not a dialog.
    const MAX_CONTROLS: usize = 64;
    /// The chat box holds at most this many bytes.
    const MAX_TEXT: usize = 0x100;

    fn read<const N: usize>(memory: &InProcessMemory, address: usize) -> Option<[u8; N]> {
        let mut raw = [0; N];
        memory.read_into(address, &mut raw).ok()?;
        Some(raw)
    }

    /// A pointer-wide word of the game.
    fn word(memory: &InProcessMemory, layout: &Layout, address: usize) -> Option<usize> {
        let mut raw = [0; 8];
        let raw = &mut raw[..layout.pointer_bytes];
        memory.read_into(address, raw).ok()?;
        bwrapi_layout::read_pointer(raw, 0, layout.pointer_bytes)
    }

    fn pointer(memory: &InProcessMemory, layout: &Layout, address: usize) -> Option<usize> {
        word(memory, layout, address).filter(|pointer| *pointer != 0)
    }

    /// The open chat box's dialog and the line typed into it. Reads go
    /// through `memory` outside the frame read scope, so a pointer the game
    /// has just freed fails the read instead of faulting.
    pub(crate) fn typed_line(memory: &InProcessMemory, layout: &Layout, chat: &ChatLayout) -> Option<(usize, Vec<u8>)> {
        let module = memory.main_module();
        let mode = read::<1>(memory, module.range_address(layout.scalars.chat_box_mode_offset, 1)?)?;
        if mode[0] == 0 {
            return None;
        }
        let dialog = pointer(memory, layout, module.range_address(chat.dialog, layout.pointer_bytes)?)?;
        let mut control = pointer(memory, layout, dialog.checked_add(chat.dialog_first_control)?);
        for _ in 0..MAX_CONTROLS {
            let at = control?;
            if u16::from_le_bytes(read::<2>(memory, at.checked_add(chat.control_id)?)?) == chat.text_box_id {
                let data = pointer(memory, layout, at.checked_add(chat.control_text)?)?;
                let length = word(memory, layout, at.checked_add(chat.control_text_length)?)?;
                let mut text = vec![0; length.min(MAX_TEXT)];
                memory.read_into(data, &mut text).ok()?;
                return Some((dialog, text));
            }
            control = pointer(memory, layout, at.checked_add(chat.control_next)?);
        }
        None
    }

    /// The local player's game number, from the same cluster the engine
    /// reads.
    fn local_player(memory: &InProcessMemory, layout: &Layout) -> Option<u32> {
        let at = memory
            .main_module()
            .range_address(layout.scalars.current_player_id_cluster_offset, CURRENT_PLAYER_ID_CLUSTER_BYTES)?;
        let mut raw = [0; CURRENT_PLAYER_ID_CLUSTER_BYTES];
        memory.read_into(at, &mut raw).ok()?;
        read_current_player_id_cluster(&raw).map(u32::from)
    }


    /// The recorder stream while chat capture is on; only the game thread
    /// uses it. Capture starts at the recorder's written end, so lines
    /// recorded before it are not reported.
    #[derive(Default)]
    pub(crate) struct ChatWatch {
        cursor: RecorderCursor,
        window: Vec<u8>,
        armed: bool,
    }

    impl ChatWatch {
        /// Queues the lines recorded — or, in a replay, played — since the
        /// last frame boundary while capture is on. `frame` is the match's
        /// game frame, `None` outside a match.
        pub(crate) fn poll(&mut self, memory: &InProcessMemory, layout: &Layout, frame: Option<u32>) {
            // Outside a match the recorder's stream is gone with its cursor;
            // the next match walks from where its capture starts.
            let (true, Some(frame)) = (capturing(), frame) else {
                *self = Self::default();
                return;
            };
            let Some(state) = view(memory, layout) else { return };
            if !self.armed {
                let Some(end) = written_end(memory, &state, frame) else { return };
                self.cursor = RecorderCursor::at_end(end);
                self.armed = true;
            }
            // A replay has no local player: every line is another player's.
            let local = state.recording.then(|| local_player(memory, layout)).flatten();
            // A window that cannot be read leaves the cursor alone; the next
            // boundary walks from the same place.
            let _ = walk_view(memory, &state, &mut self.cursor, &mut self.window, frame, |_slot, command| {
                if let Some(line) = chat_line(command, local) {
                    push(line);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(player: u8, text: &[u8]) -> Vec<u8> {
        let mut command = vec![CHAT, player];
        command.extend_from_slice(text);
        command.resize(82, 0);
        command
    }

    #[test]
    fn chat_commands_carry_their_player_and_text() {
        let line = chat_line(&chat(3, b"gl hf"), Some(0)).unwrap();
        assert_eq!((line.kind, line.player, line.text.as_slice()), (chat_kind::RECEIVED, 3, &b"gl hf"[..]));
        // The text ends at its NUL; the padding is not part of the line.
        let line = chat_line(&chat(1, b"gg\0\0"), None).unwrap();
        assert_eq!(line.text, b"gg");
        // Another command, the local player's own line, and an empty line
        // give nothing.
        assert_eq!(chat_line(&[0x18], None), None);
        assert_eq!(chat_line(&chat(0, b"own line"), Some(0)), None);
        assert_eq!(chat_line(&chat(2, b"\0"), None), None);
    }
}
