//! The game's replay recorder stream. The recorder appends every command it
//! executes to one buffer for the whole match: records of a five-byte header
//! (`u32` frame, `u8` size) followed by `size` bytes of `(storm slot,
//! command)` pairs, a frame longer than 254 bytes continuing in another
//! header of the same frame number. The frame the game has open ends where
//! the written bytes end — its size byte is only filled in when it closes —
//! and the close of an empty frame deletes its header, so the written length
//! can shrink. A new match records from the start of the buffer again.
//!
//! While a replay plays the recorder does not record: the buffer holds the
//! whole replay's stream, every size byte written. The game has executed
//! the records up to its current frame, so a walk treats the end of those
//! as the written end.

use bwrapi_layout::{read_pointer, recorder_command_length, Layout};
use bwrapi_memory::ProcessMemory;

/// Header bytes of one record: the frame number and the size byte.
const RECORD_HEADER_BYTES: usize = 5;

/// Where a walk of the recorder stream stands: the buffer offset of the next
/// command byte to read, and that of the header of the record it is in. The
/// offsets stay valid when the game moves the buffer, because it reallocates
/// the buffer while a match runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecorderCursor {
    next: usize,
    header: usize,
    /// The rest of the record at `header` is not walked: it held a command
    /// whose id the length rules do not cover.
    skip: bool,
    /// The game frame a replay walk last reached; a later walk at an earlier
    /// frame follows a seek back, which plays the replay from its start.
    played: u32,
}

impl RecorderCursor {
    /// Placed at the written end: only what the recorder writes from now on
    /// is walked.
    pub fn at_end(used: usize) -> Self {
        Self { next: used, header: used, ..Self::default() }
    }

    /// Buffer offset the next walk reads from: the header of the record being
    /// walked, or 0 when the buffer has started over below it.
    pub fn base(&self, used: usize) -> usize {
        if used < self.header { 0 } else { self.header }
    }

    /// Walks the commands recorded since the last call. `bytes` covers the
    /// buffer from `base` ([`Self::base`] of this cursor) to its written end,
    /// and `open` is the buffer offset of the open frame's first command
    /// byte (0 when the game holds no frame open). Every new command is
    /// handed to `emit` as its storm slot and its bytes, the id first.
    /// Returns whether the buffer had started over — a new match — so
    /// whatever was counted against the stream before resets.
    pub fn advance(&mut self, bytes: &[u8], base: usize, open: usize, mut emit: impl FnMut(u8, &[u8])) -> bool {
        let used = base + bytes.len();
        let started_over = used < self.header;
        if started_over {
            // A new match records from the start of the buffer again.
            *self = Self::default();
        } else if used < self.next {
            // An empty frame closed: the header this walk stood at was
            // deleted, so the next record starts where it stood.
            self.next = self.header;
            self.skip = false;
        }
        while self.next < used {
            if self.next == self.header {
                // A record header; one written mid-step is completed by the
                // next walk.
                if self.header.saturating_add(RECORD_HEADER_BYTES) > used {
                    break;
                }
                self.next = self.header + RECORD_HEADER_BYTES;
                self.skip = false;
            }
            // The open frame's size byte is not written yet; its commands run
            // to the written end.
            let open_frame = self.header + RECORD_HEADER_BYTES == open;
            let end = if open_frame {
                used
            } else {
                let size = bytes[self.header + 4 - base] as usize;
                self.header.saturating_add(RECORD_HEADER_BYTES + size).min(used)
            };
            if self.skip {
                self.next = end;
            } else {
                while self.next < end {
                    let at = self.next - base;
                    let (Some(&slot), Some(&id)) = (bytes.get(at), bytes.get(at + 1)) else {
                        if open_frame {
                            // The open record's last command is still being
                            // written; it waits for the walk that sees it
                            // whole.
                            return started_over;
                        }
                        self.next = end;
                        continue;
                    };
                    let second = bytes.get(at + 2).copied().unwrap_or(0);
                    match recorder_command_length(id, second) {
                        Some(length) if self.next + 1 + length <= end => {
                            emit(slot, &bytes[at + 1..at + 1 + length]);
                            self.next += 1 + length;
                        }
                        // A command the length rules do not cover, or one
                        // that runs past its record: the rest of the record
                        // is not walked. In the open frame a command that
                        // reaches past the written end is still being
                        // written, so it is left for the walk that sees it
                        // whole.
                        Some(_) if open_frame => return started_over,
                        _ if open_frame => {
                            self.skip = true;
                            self.next = end;
                        }
                        _ => self.next = end,
                    }
                }
            }
            // A closed record is behind the walk once it ends; the next one
            // starts right there.
            if !open_frame && self.next == end {
                self.header = self.next;
            }
        }
        started_over
    }
}

/// The recorder as one look sees it: where the buffer is, how much of it is
/// written, and at what buffer offset the open frame's commands start.
pub struct RecorderView {
    pub buffer: usize,
    pub used: usize,
    pub open: usize,
    /// The recorder records; otherwise the buffer holds a playing replay.
    pub recording: bool,
}

/// One look at the recorder's position and extent.
pub fn view(memory: &dyn ProcessMemory, layout: &Layout) -> Option<RecorderView> {
    let recorder = layout.recorder?;
    let width = layout.pointer_bytes;
    let at = memory.main_module().range_address(recorder.object, width)?;
    let mut raw = [0; 8];
    memory.read_into(at, &mut raw[..width]).ok()?;
    let object = read_pointer(&raw, 0, width).filter(|object| *object != 0)?;
    // One read covers the enabled flag, the buffer pointer, the bytes
    // written, and the open frame's pointer.
    let span = recorder.open_frame.checked_add(width)?.checked_sub(recorder.enabled)?;
    let mut tail = [0; 0x20];
    if span > tail.len() {
        return None;
    }
    memory.read_into(object.checked_add(recorder.enabled)?, &mut tail[..span]).ok()?;
    let field = |offset: usize| offset.checked_sub(recorder.enabled);
    let buffer = read_pointer(&tail, field(recorder.buffer)?, width).filter(|buffer| *buffer != 0)?;
    let used_at = field(recorder.used)?;
    let used = u32::from_le_bytes(tail.get(used_at..used_at + 4)?.try_into().ok()?) as usize;
    let open = read_pointer(&tail, field(recorder.open_frame)?, width)?;
    let open = open.checked_sub(buffer).filter(|open| *open <= used).unwrap_or(0);
    let recording = *tail.get(field(recorder.enabled)?)? != 0;
    Some(RecorderView { buffer, used, open, recording })
}

/// Where the records a playing replay has executed by `frame` end, hopping
/// the record headers from the header at `from`.
fn played_end(memory: &dyn ProcessMemory, view: &RecorderView, from: usize, frame: u32) -> Option<usize> {
    let mut header = from;
    while header + RECORD_HEADER_BYTES <= view.used {
        let mut raw = [0; RECORD_HEADER_BYTES];
        memory.read_into(view.buffer.checked_add(header)?, &mut raw).ok()?;
        if u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) > frame {
            break;
        }
        header += RECORD_HEADER_BYTES + raw[4] as usize;
    }
    Some(header.min(view.used))
}

/// The end of what the game has written — or, while a replay plays, executed
/// by `frame`: where a cursor that sees only what comes next starts.
pub fn written_end(memory: &dyn ProcessMemory, view: &RecorderView, frame: u32) -> Option<usize> {
    if view.recording { Some(view.used) } else { played_end(memory, view, 0, frame) }
}

/// Reads the window the cursor needs from `view` and walks it, handing every
/// command recorded since its last walk — or, while a replay plays, executed
/// by game `frame` — to `emit`. Returns whether the stream started over — a
/// new match, or a replay sought back —; `None` when the window could not be
/// read, which leaves the cursor where it was.
pub fn walk_view(
    memory: &dyn ProcessMemory,
    view: &RecorderView,
    cursor: &mut RecorderCursor,
    window: &mut Vec<u8>,
    frame: u32,
    emit: impl FnMut(u8, &[u8]),
) -> Option<bool> {
    let mut sought_back = false;
    let (end, open) = if view.recording {
        (view.used, view.open)
    } else {
        sought_back = frame < cursor.played;
        if sought_back {
            *cursor = RecorderCursor::default();
        }
        let end = played_end(memory, view, cursor.base(view.used), frame)?;
        cursor.played = frame;
        (end, 0)
    };
    let base = cursor.base(end);
    window.clear();
    window.resize(end.checked_sub(base)?, 0);
    memory.read_into(view.buffer.checked_add(base)?, window).ok()?;
    Some(cursor.advance(window, base, open, emit) || sought_back)
}

/// Walks the recorder through `memory`: reads the window the cursor needs and
/// hands every command recorded since its last walk to `emit`. Returns
/// whether the stream started over — a new match —; `None` when the recorder
/// could not be read, which leaves the cursor where it was.
pub fn walk(
    memory: &dyn ProcessMemory,
    layout: &Layout,
    cursor: &mut RecorderCursor,
    window: &mut Vec<u8>,
    frame: u32,
    emit: impl FnMut(u8, &[u8]),
) -> Option<bool> {
    let view = view(memory, layout)?;
    walk_view(memory, &view, cursor, window, frame, emit)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks the whole buffer once.
    fn drain(buffer: &[u8], open: usize, cursor: &mut RecorderCursor) -> Vec<(u8, Vec<u8>)> {
        let mut commands = Vec::new();
        let restarted = cursor.advance(buffer, 0, open, |slot, command| {
            commands.push((slot, command.to_vec()));
        });
        assert!(!restarted);
        commands
    }

    fn header(frame: u32, size: u8) -> Vec<u8> {
        let mut record = frame.to_le_bytes().to_vec();
        record.push(size);
        record
    }

    #[test]
    fn a_closed_and_an_open_frame_walk_command_by_command() {
        // A closed frame holds one transfer (0x63) of one entry; the next
        // frame is open, its size byte still garbage.
        let mut buffer = header(0x22e0, 7);
        buffer.extend_from_slice(&[0x00, 0x63, 0x01, 0x11, 0x2d, 0x00, 0x00]);
        buffer.extend_from_slice(&header(0x22ec, 0xf0));
        let open = buffer.len();
        buffer.extend_from_slice(&[0x00, 0x1f, 0x07, 0x00]);

        let mut cursor = RecorderCursor::default();
        // The closed frame arrives on its own first.
        let mut commands = drain(&buffer[..12], 0, &mut cursor);
        commands.extend(drain(&buffer, open, &mut cursor));
        assert_eq!(
            commands,
            vec![(0x00, vec![0x63, 0x01, 0x11, 0x2d, 0x00, 0x00]), (0x00, vec![0x1f, 0x07, 0x00])]
        );
        // Nothing new: the walk stays at the written end, reading from the
        // open record's header.
        assert!(drain(&buffer, open, &mut cursor).is_empty());
        assert_eq!(cursor.base(buffer.len()), 12);
    }

    #[test]
    fn the_open_frame_closes_and_later_frames_follow() {
        let mut buffer = header(10, 0xff);
        buffer.extend_from_slice(&[0x02, 0x18]);
        let open = buffer.len();
        buffer.extend_from_slice(&header(11, 2));
        buffer.extend_from_slice(&[0x01, 0x19]);

        let mut cursor = RecorderCursor::default();
        // The open frame's commands run to the written end, its size byte
        // still garbage.
        let mut commands = drain(&buffer[..open], open, &mut cursor);
        assert_eq!(commands, vec![(0x02, vec![0x18])]);
        // The frame closes — its size byte now governs — and frame 11
        // follows.
        buffer[4] = 2;
        commands.extend(drain(&buffer, 0, &mut cursor));
        assert_eq!(commands, vec![(0x02, vec![0x18]), (0x01, vec![0x19])]);
    }

    #[test]
    fn a_frame_split_over_the_size_byte_repeats_its_header() {
        let mut buffer = header(20, 254);
        buffer.extend_from_slice(&[0x00, 0x5a]);
        buffer.extend_from_slice(&[0x01; 252]);
        buffer.extend_from_slice(&header(20, 2));
        buffer.extend_from_slice(&[0x02, 0x2e]);

        let mut cursor = RecorderCursor::default();
        let commands = drain(&buffer, 0, &mut cursor);
        assert_eq!(commands, vec![(0x00, vec![0x5a]), (0x02, vec![0x2e])]);
    }

    #[test]
    fn an_empty_frame_close_takes_its_header_back() {
        let mut buffer = header(30, 2);
        buffer.extend_from_slice(&[0x00, 0x18]);
        buffer.extend_from_slice(&header(31, 0xff));
        let deleted_header_at = buffer.len() - 5;
        let open = buffer.len();

        let mut cursor = RecorderCursor::default();
        let commands = drain(&buffer, open, &mut cursor);
        assert_eq!(commands, vec![(0x00, vec![0x18])]);
        // The empty frame closes: its header is deleted and the next frame is
        // written over the place it stood.
        let mut after = buffer[..deleted_header_at].to_vec();
        after.extend_from_slice(&header(32, 2));
        after.extend_from_slice(&[0x03, 0x19]);
        let commands = drain(&after, 0, &mut cursor);
        assert_eq!(commands, vec![(0x03, vec![0x19])]);
    }

    #[test]
    fn a_new_match_starts_the_buffer_over() {
        let mut buffer = header(1000, 2);
        buffer.extend_from_slice(&[0x01, 0x18]);
        buffer.extend_from_slice(&header(1001, 2));
        buffer.extend_from_slice(&[0x01, 0x19]);
        let mut cursor = RecorderCursor::default();
        assert_eq!(
            drain(&buffer, 0, &mut cursor),
            vec![(0x01, vec![0x18]), (0x01, vec![0x19])]
        );

        // The next match's written bytes have not reached the cursor yet.
        let mut fresh = header(0, 2);
        fresh.extend_from_slice(&[0x04, 0x19]);
        let mut commands = Vec::new();
        let restarted = cursor.advance(&fresh, 0, 0, |slot, command| commands.push((slot, command.to_vec())));
        assert!(restarted);
        assert_eq!(commands, vec![(0x04, vec![0x19])]);
    }

    #[test]
    fn a_walk_window_covers_only_what_the_cursor_needs() {
        let mut buffer = header(40, 2);
        buffer.extend_from_slice(&[0x00, 0x18]);
        buffer.extend_from_slice(&header(41, 2));
        let open = buffer.len();
        buffer.extend_from_slice(&[0x05, 0x18]);

        let mut cursor = RecorderCursor::default();
        let first = drain(&buffer[..open], open, &mut cursor);
        assert_eq!(first, vec![(0x00, vec![0x18])]);
        // The next walk is handed a slice from the cursor's base, not the
        // whole buffer, and still sees the record it is inside.
        let base = cursor.base(buffer.len());
        assert_eq!(base, 7);
        let mut commands = Vec::new();
        cursor.advance(&buffer[base..], base, open, |slot, command| commands.push((slot, command.to_vec())));
        assert_eq!(commands, vec![(0x05, vec![0x18])]);
    }

    #[test]
    fn commands_the_rules_do_not_cover_skip_their_record() {
        let mut buffer = header(50, 8);
        buffer.extend_from_slice(&[0x00, 0x18, 0x66, 0x00, 0x18, 0x00, 0x19, 0x00]);
        buffer.extend_from_slice(&header(51, 2));
        buffer.extend_from_slice(&[0x01, 0x19]);

        let mut cursor = RecorderCursor::default();
        // 0x66 is never recorded, so the rest of frame 50 is skipped and
        // frame 51 still walks.
        assert_eq!(
            drain(&buffer, 0, &mut cursor),
            vec![(0x00, vec![0x18]), (0x01, vec![0x19])]
        );
    }

    #[test]
    fn a_command_being_written_waits_for_its_bytes() {
        let mut buffer = header(60, 0xff);
        let open = buffer.len();
        buffer.extend_from_slice(&[0x00, 0x09, 0x03, 0xaa, 0xbb]);

        let mut cursor = RecorderCursor::default();
        // The variable-length command (six payload bytes) is short: the walk
        // reads none of it.
        assert!(drain(&buffer, open, &mut cursor).is_empty());
        buffer.extend_from_slice(&[0xcc, 0xdd, 0xee, 0xff]);
        assert_eq!(
            drain(&buffer, open, &mut cursor),
            vec![(0x00, vec![0x09, 0x03, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])]
        );
    }

    #[test]
    fn a_cursor_at_end_sees_only_what_comes_next() {
        let mut buffer = header(70, 2);
        buffer.extend_from_slice(&[0x00, 0x18]);
        let mut cursor = RecorderCursor::at_end(buffer.len());
        assert!(drain(&buffer, 0, &mut cursor).is_empty());
        buffer.extend_from_slice(&header(71, 2));
        buffer.extend_from_slice(&[0x06, 0x18]);
        assert_eq!(drain(&buffer, 0, &mut cursor), vec![(0x06, vec![0x18])]);
    }

    #[test]
    fn a_playing_replay_walks_up_to_the_game_frame() {
        use bwrapi_memory::{InProcessMemory, ProcessModule};
        let mut buffer = Vec::new();
        for (frame, slot) in [(10u32, 0u8), (20, 1), (30, 2)] {
            buffer.extend_from_slice(&header(frame, 2));
            buffer.extend_from_slice(&[slot, 0x18]);
        }
        let memory = InProcessMemory::with_module(ProcessModule { base_address: 0, size: usize::MAX });
        let view = RecorderView { buffer: buffer.as_ptr() as usize, used: buffer.len(), open: 0, recording: false };
        let mut cursor = RecorderCursor::default();
        let mut window = Vec::new();
        let mut walk_to = |cursor: &mut RecorderCursor, frame| {
            let mut slots = Vec::new();
            let restarted = walk_view(&memory, &view, cursor, &mut window, frame, |slot, _| slots.push(slot)).unwrap();
            (restarted, slots)
        };
        // Only the records the game has executed are walked, each once.
        assert_eq!(walk_to(&mut cursor, 15), (false, vec![0]));
        assert_eq!(walk_to(&mut cursor, 25), (false, vec![1]));
        assert_eq!(walk_to(&mut cursor, 25), (false, vec![]));
        // A seek back plays the replay from its start again.
        assert_eq!(walk_to(&mut cursor, 12), (true, vec![0]));
        assert_eq!(walk_to(&mut cursor, 40), (false, vec![1, 2]));
        // A capture that starts mid-replay sees only what plays next.
        let end = written_end(&memory, &view, 20).unwrap();
        assert_eq!(end, 14);
        assert_eq!(walk_to(&mut RecorderCursor::at_end(end), 30), (false, vec![2]));
    }
}